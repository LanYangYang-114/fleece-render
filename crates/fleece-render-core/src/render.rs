use std::collections::BTreeMap;
use std::io::Cursor;
use std::sync::Arc;
use std::time::Instant;

use image::{ImageFormat, Rgba, RgbaImage};

use crate::avatar::AvatarStyle;
use crate::backend::{BackendKind, CpuBackend, RenderBackend};
use crate::camera::Camera;
use crate::error::{CoreError, Result};
use crate::geometry::Scene;
use crate::math::Vec3;
use crate::model::{ModelKind, build_scene};
use crate::types::{
    Parts, RenderInfo, RenderOptions, RenderOutput, RenderedImage, SkinAssets, View,
};

const FRONT_CAMERA: Vec3 = Vec3::new(17.9, 19.4, 37.6);
const BACK_CAMERA: Vec3 = Vec3::new(-19.0, 18.1, -37.8);

pub trait RenderStyle: Send + Sync {
    fn id(&self) -> &'static str;
    fn label(&self) -> &'static str;
    fn build(
        &self,
        _assets: &SkinAssets,
        _options: &RenderOptions,
        _view: View,
    ) -> Result<(Scene, ModelKind)> {
        Err(CoreError::Render(format!("{} has no 3D scene", self.id())))
    }

    /// Return a finished front image directly, bypassing the 3D rasterizer.
    fn render_flat(
        &self,
        _assets: &SkinAssets,
        _options: &RenderOptions,
    ) -> Result<Option<(RgbaImage, ModelKind)>> {
        Ok(None)
    }
}

#[derive(Default)]
pub struct ThreeDStyle;

impl RenderStyle for ThreeDStyle {
    fn id(&self) -> &'static str {
        "3d"
    }

    fn label(&self) -> &'static str {
        "skinview3d-compatible 3D character"
    }

    fn build(
        &self,
        assets: &SkinAssets,
        options: &RenderOptions,
        view: View,
    ) -> Result<(Scene, ModelKind)> {
        build_scene(assets, options, view)
    }
}

pub struct StyleRegistry {
    styles: BTreeMap<&'static str, Arc<dyn RenderStyle>>,
}

impl Default for StyleRegistry {
    fn default() -> Self {
        let mut registry = Self {
            styles: BTreeMap::new(),
        };
        registry.register(Arc::new(ThreeDStyle));
        registry.register(Arc::new(AvatarStyle));
        registry
    }
}

impl StyleRegistry {
    pub fn register(&mut self, style: Arc<dyn RenderStyle>) {
        self.styles.insert(style.id(), style);
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn RenderStyle>> {
        self.styles.get(id).cloned()
    }

    pub fn ids(&self) -> Vec<&'static str> {
        self.styles.keys().copied().collect()
    }

    pub fn descriptions(&self) -> Vec<(&'static str, &'static str)> {
        self.styles
            .values()
            .map(|style| (style.id(), style.label()))
            .collect()
    }
}

pub struct RenderEngine {
    styles: StyleRegistry,
    backend: Arc<dyn RenderBackend>,
    backend_kind: BackendKind,
    gpu_fallback: bool,
}

impl RenderEngine {
    pub fn new(backend_kind: BackendKind) -> Result<Self> {
        let (backend, resolved_backend): (Arc<dyn RenderBackend>, _) = match backend_kind {
            BackendKind::Cpu => (Arc::new(CpuBackend), BackendKind::Cpu),
            #[cfg(not(feature = "gpu"))]
            BackendKind::Auto => (Arc::new(CpuBackend), BackendKind::Cpu),
            #[cfg(not(feature = "gpu"))]
            BackendKind::Gpu => {
                return Err(CoreError::BackendUnavailable(
                    "GPU backend is not built; enable the gpu Cargo feature".to_owned(),
                ));
            }
            #[cfg(feature = "gpu")]
            BackendKind::Auto | BackendKind::Gpu => match crate::backend::gpu::GpuBackend::new() {
                Ok(gpu) => (Arc::new(gpu), BackendKind::Gpu),
                Err(error) if backend_kind == BackendKind::Auto => {
                    tracing::warn!(%error, "GPU initialization failed; using CPU");
                    (Arc::new(CpuBackend), BackendKind::Cpu)
                }
                Err(error) => return Err(error),
            },
        };
        Ok(Self {
            styles: StyleRegistry::default(),
            backend,
            backend_kind: resolved_backend,
            gpu_fallback: backend_kind == BackendKind::Auto && resolved_backend == BackendKind::Gpu,
        })
    }

    pub fn with_styles(mut self, styles: StyleRegistry) -> Self {
        self.styles = styles;
        self
    }

    pub fn backend_id(&self) -> &'static str {
        self.backend.id()
    }

    pub fn backend_kind(&self) -> BackendKind {
        self.backend_kind
    }

    pub fn styles(&self) -> &StyleRegistry {
        &self.styles
    }

    pub fn render(&self, assets: &SkinAssets, options: &RenderOptions) -> Result<RenderOutput> {
        options.validate()?;
        let started = Instant::now();
        let style = self.styles.get(&options.style).ok_or_else(|| {
            CoreError::UnsupportedStyle(format!(
                "unknown style {:?}; available styles: {}",
                options.style,
                self.styles.ids().join(", ")
            ))
        })?;
        if let Some((image, model)) = style.render_flat(assets, options)? {
            if options.parts != Parts::Front {
                return Err(CoreError::InvalidRequest(format!(
                    "{} style requires front parts",
                    style.id()
                )));
            }
            let front = encode_image(&image)?;
            return Ok(RenderOutput {
                info: RenderInfo {
                    backend: "cpu".to_owned(),
                    style: style.id().to_owned(),
                    model: model.to_string(),
                    parts: options.parts.to_string(),
                    panel_width: front.width,
                    panel_height: front.height,
                    output_width: front.width,
                    output_height: front.height,
                    elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
                    ..RenderInfo::default()
                },
                front: Some(front),
                ..RenderOutput::default()
            });
        }
        let render = |backend: &dyn RenderBackend| {
            Self::render_3d(backend, style.as_ref(), assets, options, started)
        };
        match render(self.backend.as_ref()) {
            Err(error @ CoreError::BackendUnavailable(_)) if self.gpu_fallback => {
                tracing::warn!(%error, "GPU render failed; retrying the entire request on CPU");
                render(&CpuBackend)
            }
            result => result,
        }
    }

    fn render_3d(
        backend: &dyn RenderBackend,
        style: &dyn RenderStyle,
        assets: &SkinAssets,
        options: &RenderOptions,
        started: Instant,
    ) -> Result<RenderOutput> {
        let (panel_width, panel_height) = options.panel_output_size();
        let supersample = adaptive_supersample(options, panel_width, panel_height);
        let mut info = RenderInfo {
            backend: backend.id().to_owned(),
            style: style.id().to_owned(),
            parts: options.parts.to_string(),
            panel_width,
            panel_height,
            ..RenderInfo::default()
        };
        let mut model = ModelKind::Classic;
        let mut triangles = 0usize;

        let front = if options.parts.wants_front() {
            let (scene, kind) = style.build(assets, options, View::Front)?;
            model = kind;
            triangles += scene.triangles.len();
            Some(Self::render_panel(
                backend,
                &scene,
                View::Front,
                options,
                panel_width,
                panel_height,
                supersample,
            )?)
        } else {
            None
        };
        let back = if options.parts.wants_back() {
            let (scene, kind) = style.build(assets, options, View::Back)?;
            model = kind;
            triangles += scene.triangles.len();
            Some(Self::render_panel(
                backend,
                &scene,
                View::Back,
                options,
                panel_width,
                panel_height,
                supersample,
            )?)
        } else {
            None
        };

        let both = match (&front, &back) {
            (Some(front_panel), Some(back_panel)) => Some(join_panels(front_panel, back_panel)?),
            _ => None,
        };

        let encode = |image: Option<&RgbaImage>| -> Result<Option<RenderedImage>> {
            image.map(encode_image).transpose()
        };
        let front = if matches!(options.parts, Parts::Front | Parts::All) {
            encode(front.as_ref())?
        } else {
            None
        };
        let back = if matches!(options.parts, Parts::Back | Parts::All) {
            encode(back.as_ref())?
        } else {
            None
        };
        let both = match options.parts {
            Parts::Both | Parts::All => encode(both.as_ref())?,
            _ => None,
        };

        info.model = model.to_string();
        info.output_width = match &both {
            Some(image) => image.width,
            None => panel_width,
        };
        info.output_height = panel_height;
        info.triangles = triangles;
        info.elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
        Ok(RenderOutput {
            front,
            back,
            both,
            info,
        })
    }

    fn render_panel(
        backend: &dyn RenderBackend,
        scene: &Scene,
        view: View,
        options: &RenderOptions,
        width: u32,
        height: u32,
        supersample: u32,
    ) -> Result<RgbaImage> {
        let internal_width = width.saturating_mul(supersample);
        let internal_height = height.saturating_mul(supersample);
        let camera_position = match view {
            View::Front => FRONT_CAMERA / options.zoom,
            View::Back => BACK_CAMERA / options.zoom,
        };
        let camera = Camera::new(
            camera_position,
            Vec3::new(0.0, options.target_y, 0.0),
            options.fov,
            internal_width,
            internal_height,
        );
        let rendered = backend.render(scene, &camera, internal_width, internal_height)?;
        let panel = if supersample == 1 {
            rendered
        } else {
            image::imageops::resize(
                &rendered,
                width,
                height,
                image::imageops::FilterType::Triangle,
            )
        };
        let composite = composite_panel(
            &panel,
            options.transparent,
            options.shadow,
            options.definition,
        );
        Ok(composite)
    }
}

fn adaptive_supersample(options: &RenderOptions, width: u32, height: u32) -> u32 {
    let mut supersample = options.supersample.max(1);
    let base_pixels = u64::from(width) * u64::from(height);
    while supersample > 1
        && base_pixels.saturating_mul(u64::from(supersample) * u64::from(supersample)) > 32_000_000
    {
        supersample -= 1;
    }
    supersample
}

fn composite_panel(
    panel: &RgbaImage,
    transparent: bool,
    shadow: bool,
    definition: f32,
) -> RgbaImage {
    let width = panel.width();
    let height = panel.height();
    let mut output = RgbaImage::from_pixel(
        width,
        height,
        if transparent {
            Rgba([0, 0, 0, 0])
        } else {
            Rgba([255, 255, 255, 255])
        },
    );
    if shadow {
        let alpha: Vec<f32> = panel
            .pixels()
            .map(|pixel| pixel[3] as f32 / 255.0)
            .collect();
        let radius = (7.0 * definition).round().max(1.0) as i32;
        let blurred = box_blur_alpha(&alpha, width as usize, height as usize, radius);
        let offset_y = (7.0 * definition).round() as i32;
        for y in 0..height as i32 {
            let src_y = y - offset_y;
            if src_y < 0 || src_y >= height as i32 {
                continue;
            }
            for x in 0..width {
                let shadow_alpha = blurred[src_y as usize * width as usize + x as usize] * 0.30;
                if shadow_alpha <= 0.0 {
                    continue;
                }
                let dst = output.get_pixel_mut(x, y as u32);
                *dst = blend_pixel(*dst, [0, 0, 0, (shadow_alpha * 255.0).round() as u8]);
            }
        }
    }
    for (x, y, pixel) in panel.enumerate_pixels() {
        let dst = output.get_pixel_mut(x, y);
        *dst = blend_pixel(*dst, pixel.0);
    }
    output
}

fn box_blur_alpha(input: &[f32], width: usize, height: usize, radius: i32) -> Vec<f32> {
    let mut horizontal = vec![0.0f32; input.len()];
    let mut output = vec![0.0f32; input.len()];
    for y in 0..height {
        let mut sum = 0.0;
        let row = y * width;
        for x in -radius..=radius {
            let xx = x.clamp(0, width as i32 - 1) as usize;
            sum += input[row + xx];
        }
        let divisor = (radius * 2 + 1) as f32;
        for x in 0..width {
            horizontal[row + x] = sum / divisor;
            let remove = (x as i32 - radius).clamp(0, width as i32 - 1) as usize;
            let add = (x as i32 + radius + 1).clamp(0, width as i32 - 1) as usize;
            sum += input[row + add] - input[row + remove];
        }
    }
    for x in 0..width {
        let mut sum = 0.0;
        for y in -radius..=radius {
            let yy = y.clamp(0, height as i32 - 1) as usize;
            sum += horizontal[yy * width + x];
        }
        let divisor = (radius * 2 + 1) as f32;
        for y in 0..height {
            output[y * width + x] = sum / divisor;
            let remove = (y as i32 - radius).clamp(0, height as i32 - 1) as usize;
            let add = (y as i32 + radius + 1).clamp(0, height as i32 - 1) as usize;
            sum += horizontal[add * width + x] - horizontal[remove * width + x];
        }
    }
    output
}

fn join_panels(front: &RgbaImage, back: &RgbaImage) -> Result<RgbaImage> {
    if front.height() != back.height() {
        return Err(CoreError::Render(
            "front and back panels have different heights".to_owned(),
        ));
    }
    let mut output = RgbaImage::new(front.width() + back.width(), front.height());
    image::imageops::replace(&mut output, front, 0, 0);
    image::imageops::replace(&mut output, back, front.width() as i64, 0);
    Ok(output)
}

fn encode_image(image: &RgbaImage) -> Result<RenderedImage> {
    let mut bytes = Vec::new();
    image.write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)?;
    Ok(RenderedImage {
        bytes,
        width: image.width(),
        height: image.height(),
    })
}

pub(crate) fn blend_pixel(dst: Rgba<u8>, src: [u8; 4]) -> Rgba<u8> {
    if src[3] == 255 {
        return Rgba(src);
    }
    let sa = src[3] as f32 / 255.0;
    let da = dst[3] as f32 / 255.0;
    let out_a = sa + da * (1.0 - sa);
    if out_a <= f32::EPSILON {
        return Rgba([0, 0, 0, 0]);
    }
    let channel = |s: u8, d: u8| -> u8 {
        (((s as f32 * sa + d as f32 * da * (1.0 - sa)) / out_a)
            .round()
            .clamp(0.0, 255.0)) as u8
    };
    Rgba([
        channel(src[0], dst[0]),
        channel(src[1], dst[1]),
        channel(src[2], dst[2]),
        (out_a * 255.0).round().clamp(0.0, 255.0) as u8,
    ])
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::Texture;

    #[test]
    fn automatic_fallback_rerenders_all_panels_and_reports_cpu() {
        // Simulate device loss after the first panel succeeded, without needing hardware.
        #[derive(Default)]
        struct FailingGpu(AtomicUsize);

        impl RenderBackend for FailingGpu {
            fn id(&self) -> &'static str {
                "gpu"
            }

            fn render(&self, _: &Scene, _: &Camera, width: u32, height: u32) -> Result<RgbaImage> {
                if self.0.fetch_add(1, Ordering::Relaxed) == 0 {
                    Ok(RgbaImage::new(width, height))
                } else {
                    Err(CoreError::BackendUnavailable("test device lost".to_owned()))
                }
            }
        }

        let assets = SkinAssets {
            skin: Texture::from_rgba(RgbaImage::from_pixel(64, 64, Rgba([80, 120, 190, 255]))),
            back: None,
            display_name: None,
        };
        let options = RenderOptions {
            parts: Parts::Both,
            definition: 0.25,
            supersample: 1,
            ..RenderOptions::default()
        };
        let mut engine = RenderEngine::new(BackendKind::Cpu).unwrap();
        let expected = engine.render(&assets, &options).unwrap();
        engine.backend = Arc::new(FailingGpu::default());
        engine.backend_kind = BackendKind::Gpu;
        assert!(matches!(
            engine.render(&assets, &options),
            Err(CoreError::BackendUnavailable(_))
        ));

        engine.backend = Arc::new(FailingGpu::default());
        engine.gpu_fallback = true;
        let actual = engine.render(&assets, &options).unwrap();
        assert_eq!(actual.info.backend, "cpu");
        assert_eq!(
            actual.primary().unwrap().bytes,
            expected.primary().unwrap().bytes
        );
    }
}
