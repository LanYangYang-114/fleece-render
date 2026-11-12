//! Headless Vulkan compute rasterizer. The CPU prepares the same screen-space
//! triangles as the software renderer; one GPU invocation owns each pixel.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use bytemuck::{Pod, Zeroable, cast_slice};
use image::RgbaImage;
use wgpu::util::DeviceExt;

use super::{Camera, CoreError, RenderBackend, Result, Scene, ScreenTriangle};
use super::{draw_sprites, prepare_triangles};

const TILE_SIZE: u32 = 32;
const WORKGROUP_SIZE: u32 = 8;
const MAX_STORAGE_BYTES: u32 = 48_000_000 * 4;
const READBACK_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) struct GpuBackend {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    failed: Arc<AtomicBool>,
    // Bound device memory usage and keep per-device error scopes paired.
    render_lock: Mutex<()>,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuTriangle {
    // The fourth component stores U; V is stored alongside the triangle area.
    vertices: [[f32; 4]; 3],
    v_area: [f32; 4],
    texture_shade: [u32; 4],
    bounds: [u32; 4],
}

fn unavailable(message: impl std::fmt::Display) -> CoreError {
    CoreError::BackendUnavailable(format!("GPU: {message}"))
}

impl GpuBackend {
    pub(crate) fn new() -> Result<Self> {
        if !wgpu::Instance::enabled_backend_features().contains(wgpu::Backends::VULKAN) {
            return Err(unavailable("Vulkan is not supported on this target"));
        }
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..Default::default()
        });
        let mut adapters = instance.enumerate_adapters(wgpu::Backends::VULKAN);
        adapters.retain(|adapter| adapter.get_info().device_type != wgpu::DeviceType::Cpu);
        adapters.sort_by_key(|adapter| match adapter.get_info().device_type {
            wgpu::DeviceType::DiscreteGpu => 0,
            wgpu::DeviceType::IntegratedGpu => 1,
            _ => 2,
        });
        let mut failures = Vec::new();
        for adapter in adapters {
            let info = adapter.get_info();
            match Self::from_adapter(&adapter) {
                Ok(gpu) => {
                    tracing::info!(adapter = %info.name, "initialized Vulkan GPU rasterizer");
                    return Ok(gpu);
                }
                Err(error) => failures.push(format!("{}: {error}", info.name)),
            }
        }
        if failures.is_empty() {
            Err(unavailable("no Vulkan hardware adapter found"))
        } else {
            Err(unavailable(failures.join("; ")))
        }
    }

    fn from_adapter(adapter: &wgpu::Adapter) -> Result<Self> {
        let adapter_limits = adapter.limits();
        let limits = wgpu::Limits {
            max_storage_buffer_binding_size: adapter_limits
                .max_storage_buffer_binding_size
                .min(MAX_STORAGE_BYTES),
            ..wgpu::Limits::downlevel_defaults()
        };
        if !limits.check_limits(&adapter_limits) {
            return Err(unavailable(
                "adapter limits are insufficient for compute rasterization",
            ));
        }
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("fleece-render GPU"),
            required_limits: limits,
            ..Default::default()
        }))
        .map_err(unavailable)?;
        let failed = Arc::new(AtomicBool::new(false));
        let lost = failed.clone();
        device.set_device_lost_callback(move |reason, message| {
            if reason != wgpu::DeviceLostReason::Destroyed {
                lost.store(true, Ordering::Relaxed);
                tracing::warn!(?reason, %message, "GPU device lost");
            }
        });
        let uncaptured = failed.clone();
        device.on_uncaptured_error(Arc::new(move |error| {
            uncaptured.store(true, Ordering::Relaxed);
            tracing::error!(%error, "uncaptured GPU device error");
        }));
        let pipeline = scoped(&device, || {
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("fleece-render rasterizer"),
                source: wgpu::ShaderSource::Wgsl(include_str!("raster.wgsl").into()),
            });
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("fleece-render rasterizer"),
                layout: None,
                module: &shader,
                entry_point: Some("raster"),
                compilation_options: Default::default(),
                cache: None,
            })
        })?;
        Ok(Self {
            device,
            queue,
            pipeline,
            failed,
            render_lock: Mutex::new(()),
        })
    }
}

impl RenderBackend for GpuBackend {
    fn id(&self) -> &'static str {
        "gpu"
    }

    fn render(&self, scene: &Scene, camera: &Camera, width: u32, height: u32) -> Result<RgbaImage> {
        if width == 0 || height == 0 {
            return Err(CoreError::Render("zero-sized framebuffer".to_owned()));
        }
        let guard = self.render_lock.lock().map_err(unavailable)?;
        if self.failed.load(Ordering::Relaxed) {
            return Err(unavailable("device is no longer usable"));
        }
        let limits = self.device.limits();
        let max_bytes = limits.max_storage_buffer_binding_size as usize;
        let output_size = u64::from(width) * u64::from(height);
        let groups_x = width.div_ceil(WORKGROUP_SIZE);
        let groups_y = height.div_ceil(WORKGROUP_SIZE);
        if output_size > (max_bytes / 4) as u64
            || groups_x.max(groups_y) > limits.max_compute_workgroups_per_dimension
        {
            return Err(unavailable("framebuffer exceeds device limits"));
        }
        let output_bytes = output_size * 4;
        let triangles = prepare_triangles(scene, camera, width, height);
        let mut image = if triangles.is_empty() {
            RgbaImage::new(width, height)
        } else {
            let tiles_x = width.div_ceil(TILE_SIZE);
            let tiles_y = height.div_ceil(TILE_SIZE);
            if triangles.len() > max_bytes / size_of::<GpuTriangle>() {
                return Err(unavailable("triangle buffer exceeds device limits"));
            }
            let bins = tile_bins(&triangles, tiles_x, tiles_y, max_bytes / 4)?;
            let (triangles, texels) = pack_scene(scene, &triangles, max_bytes)?;
            let device = &self.device;
            let (readback, submission) = scoped(device, || {
                let params = [width, height, tiles_x, TILE_SIZE];
                let upload = |label, bytes, usage| {
                    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some(label),
                        contents: bytes,
                        usage,
                    })
                };
                let storage = wgpu::BufferUsages::STORAGE;
                let uniform = upload(
                    "raster params",
                    cast_slice(&params),
                    wgpu::BufferUsages::UNIFORM,
                );
                let triangles = upload("triangles", cast_slice(&triangles), storage);
                let bins = upload("triangle bins", cast_slice(&bins), storage);
                let texels = upload("texture pixels", &texels, storage);
                let buffer = |label, usage| {
                    device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some(label),
                        size: output_bytes,
                        usage,
                        mapped_at_creation: false,
                    })
                };
                let output = buffer("raster output", storage | wgpu::BufferUsages::COPY_SRC);
                let readback = buffer(
                    "raster readback",
                    wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                );
                let entries = [&uniform, &triangles, &bins, &texels, &output]
                    .map(wgpu::Buffer::as_entire_binding);
                let entries: Vec<_> = entries
                    .into_iter()
                    .enumerate()
                    .map(|(binding, resource)| wgpu::BindGroupEntry {
                        binding: binding as u32,
                        resource,
                    })
                    .collect();
                let bindings = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("raster bindings"),
                    layout: &self.pipeline.get_bind_group_layout(0),
                    entries: &entries,
                });
                let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("raster encoder"),
                });
                {
                    let mut pass =
                        encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
                    pass.set_pipeline(&self.pipeline);
                    pass.set_bind_group(0, &bindings, &[]);
                    pass.dispatch_workgroups(groups_x, groups_y, 1);
                }
                encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, output_bytes);
                (readback, self.queue.submit([encoder.finish()]))
            })?;
            let (sender, receiver) = mpsc::sync_channel(1);
            let slice = readback.slice(..);
            slice.map_async(wgpu::MapMode::Read, move |result| {
                let _ = sender.send(result);
            });
            if let Err(error) = device.poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(READBACK_TIMEOUT),
            }) {
                self.failed.store(true, Ordering::Relaxed);
                return Err(unavailable(error));
            }
            receiver
                .try_recv()
                .map_err(unavailable)?
                .map_err(unavailable)?;
            let pixels = slice.get_mapped_range().to_vec();
            readback.unmap();
            RgbaImage::from_raw(width, height, pixels)
                .ok_or_else(|| unavailable("incorrect framebuffer size after readback"))?
        };
        drop(guard);
        // Name tags, supersampling, shadows, backgrounds and PNG encoding use
        // the existing CPU paths; only the 3D triangle rasterization is offloaded.
        draw_sprites(&mut image, camera, scene, width, height);
        Ok(image)
    }
}

fn scoped<T>(device: &wgpu::Device, op: impl FnOnce() -> T) -> Result<T> {
    for filter in [
        wgpu::ErrorFilter::OutOfMemory,
        wgpu::ErrorFilter::Internal,
        wgpu::ErrorFilter::Validation,
    ] {
        device.push_error_scope(filter);
    }
    let result = op();
    let mut error = None;
    for _ in 0..3 {
        error = pollster::block_on(device.pop_error_scope()).or(error);
    }
    error.map_or(Ok(result), |error| Err(unavailable(error)))
}

/// Each tile has a half-open range into a flat list of triangle indices. The
/// indices retain painter's order, even for overlapping translucent triangles.
fn tile_bins(
    triangles: &[ScreenTriangle],
    tiles_x: u32,
    tiles_y: u32,
    max_words: usize,
) -> Result<Vec<u32>> {
    let header = tiles_x as usize * tiles_y as usize * 2;
    if header > max_words {
        return Err(unavailable("triangle bins exceed device limits"));
    }
    let mut bins = vec![0u32; header];
    let for_tiles = |triangle: &ScreenTriangle, visit: &mut dyn FnMut(usize)| {
        let [x0, y0, x1, y1] = triangle.bounds.map(|v| v / TILE_SIZE);
        for y in y0..=y1 {
            for x in x0..=x1 {
                visit((y as usize * tiles_x as usize + x as usize) * 2);
            }
        }
    };
    let mut size = header;
    for triangle in triangles {
        let [x0, y0, x1, y1] = triangle.bounds.map(|v| v / TILE_SIZE);
        size += (x1 - x0 + 1) as usize * (y1 - y0 + 1) as usize;
        if size > max_words {
            return Err(unavailable("triangle bins exceed device limits"));
        }
        for_tiles(triangle, &mut |tile| bins[tile + 1] += 1);
    }
    let mut offset = header as u32;
    for range in bins.as_chunks_mut::<2>().0 {
        let count = range[1];
        range.fill(offset);
        offset += count;
    }
    bins.resize(size, 0);
    for (i, triangle) in triangles.iter().enumerate() {
        for_tiles(triangle, &mut |tile| {
            let index = bins[tile + 1] as usize;
            bins[index] = i as u32;
            bins[tile + 1] += 1;
        });
    }
    Ok(bins)
}

fn pack_scene(
    scene: &Scene,
    triangles: &[ScreenTriangle],
    max_bytes: usize,
) -> Result<(Vec<GpuTriangle>, Vec<u8>)> {
    let mut textures = vec![None; scene.textures.len()];
    let mut texels = Vec::new();
    let triangles = triangles
        .iter()
        .map(|triangle| {
            let layout = &mut textures[triangle.texture.0];
            let [offset, width, height] = if let Some(layout) = *layout {
                layout
            } else {
                let texture = scene.texture(triangle.texture);
                let pixels = texture.pixels().as_raw();
                if pixels.is_empty() || pixels.len() > max_bytes - texels.len() {
                    return Err(unavailable(
                        "empty texture or texture buffer exceeds device limits",
                    ));
                }
                let values = [(texels.len() / 4) as u32, texture.width(), texture.height()];
                texels.extend_from_slice(pixels);
                *layout = Some(values);
                values
            };
            Ok(GpuTriangle {
                vertices: std::array::from_fn(|i| {
                    let point = triangle.points[i];
                    [point.x, point.y, triangle.inv_z[i], triangle.uvs[i].x]
                }),
                v_area: [
                    triangle.uvs[0].y,
                    triangle.uvs[1].y,
                    triangle.uvs[2].y,
                    triangle.area,
                ],
                texture_shade: [offset, width, height, triangle.shade.to_bits()],
                bounds: triangle.bounds,
            })
        })
        .collect::<Result<_>>()?;
    Ok((triangles, texels))
}
