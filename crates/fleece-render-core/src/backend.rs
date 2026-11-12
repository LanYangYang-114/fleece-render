use std::str::FromStr;

use image::{Rgba, RgbaImage};

use crate::camera::{Camera, Projected};
use crate::error::{CoreError, Result};
use crate::geometry::{Scene, TextureId, Triangle};
use crate::math::{Vec2, Vec3};

#[cfg(feature = "gpu")]
pub(crate) mod gpu;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BackendKind {
    #[default]
    Auto,
    Cpu,
    Gpu,
}

impl std::fmt::Display for BackendKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Auto => "auto",
            Self::Cpu => "cpu",
            Self::Gpu => "gpu",
        })
    }
}

impl FromStr for BackendKind {
    type Err = CoreError;
    fn from_str(value: &str) -> Result<Self> {
        match value.to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "cpu" | "software" => Ok(Self::Cpu),
            "gpu" => Ok(Self::Gpu),
            other => Err(CoreError::InvalidRequest(format!(
                "unknown backend {other:?}; expected auto, cpu, or gpu"
            ))),
        }
    }
}

/// Returns the compiled backends, without probing for a GPU device.
pub fn available_backends() -> &'static [&'static str] {
    &[
        "cpu",
        #[cfg(feature = "gpu")]
        "gpu",
    ]
}

pub trait RenderBackend: Send + Sync {
    fn id(&self) -> &'static str;
    fn render(&self, scene: &Scene, camera: &Camera, width: u32, height: u32) -> Result<RgbaImage>;
}

#[derive(Default)]
pub struct CpuBackend;

impl RenderBackend for CpuBackend {
    fn id(&self) -> &'static str {
        "cpu"
    }

    fn render(&self, scene: &Scene, camera: &Camera, width: u32, height: u32) -> Result<RgbaImage> {
        if width == 0 || height == 0 {
            return Err(CoreError::Render("zero-sized framebuffer".to_owned()));
        }
        let mut color = RgbaImage::from_pixel(width, height, Rgba([0, 0, 0, 0]));
        let mut depth = vec![f32::INFINITY; width as usize * height as usize];

        for triangle in prepare_triangles(scene, camera, width, height) {
            draw_triangle(&mut color, &mut depth, scene, &triangle, width);
        }
        draw_sprites(&mut color, camera, scene, width, height);
        Ok(color)
    }
}

struct ScreenTriangle {
    points: [Projected; 3],
    uvs: [Vec2; 3],
    texture: TextureId,
    area: f32,
    shade: f32,
    inv_z: [f32; 3],
    // Inclusive, clipped pixel bounds: min_x, min_y, max_x, max_y.
    bounds: [u32; 4],
}

fn prepare_triangles(
    scene: &Scene,
    camera: &Camera,
    width: u32,
    height: u32,
) -> Vec<ScreenTriangle> {
    let mut ordered: Vec<_> = scene
        .triangles
        .iter()
        .filter_map(|triangle| {
            let center =
                (triangle.positions[0] + triangle.positions[1] + triangle.positions[2]) / 3.0;
            Some((triangle, center, camera.project(center)?.depth))
        })
        .collect();
    // Stable painter's order for translucent layers, before per-pixel depth tests.
    ordered.sort_by(|a, b| b.2.total_cmp(&a.2));
    let light_direction = (camera.position * 0.25 + Vec3::new(-18.0, 28.0, 22.0)).normalized();
    ordered
        .into_iter()
        .filter_map(|(triangle, center, _)| {
            prepare_triangle(camera, triangle, center, width, height, light_direction)
        })
        .collect()
}

fn prepare_triangle(
    camera: &Camera,
    triangle: &Triangle,
    center: Vec3,
    width: u32,
    height: u32,
    light_direction: Vec3,
) -> Option<ScreenTriangle> {
    let projected = [
        camera.project(triangle.positions[0]),
        camera.project(triangle.positions[1]),
        camera.project(triangle.positions[2]),
    ];
    let (Some(p0), Some(p1), Some(p2)) = (projected[0], projected[1], projected[2]) else {
        return None;
    };

    let mut normal = (triangle.positions[1] - triangle.positions[0])
        .cross(triangle.positions[2] - triangle.positions[0])
        .normalized();
    let facing = normal.dot(camera.position - center);
    if facing <= 0.0 {
        if triangle.double_sided {
            normal = -normal;
        } else {
            return None;
        }
    }

    let area = edge(
        Vec2::new(p0.x, p0.y),
        Vec2::new(p1.x, p1.y),
        Vec2::new(p2.x, p2.y),
    );
    if area.abs() < 1e-5 {
        return None;
    }

    let min_x = p0.x.min(p1.x).min(p2.x).floor().max(0.0) as i32;
    let max_x = p0.x.max(p1.x).max(p2.x).ceil().min(width as f32 - 1.0) as i32;
    let min_y = p0.y.min(p1.y).min(p2.y).floor().max(0.0) as i32;
    let max_y = p0.y.max(p1.y).max(p2.y).ceil().min(height as f32 - 1.0) as i32;
    if min_x > max_x || min_y > max_y {
        return None;
    }

    let camera_dir = (camera.position - center).normalized();
    let camera_term = normal.dot(camera_dir).max(0.0) * 0.18;
    let key_term = normal.dot(light_direction).max(0.0) * 0.14;
    let shade = (0.68 + camera_term + key_term).clamp(0.0, 1.0);

    Some(ScreenTriangle {
        points: [p0, p1, p2],
        uvs: triangle.uvs,
        texture: triangle.texture,
        area,
        shade,
        inv_z: [1.0 / p0.depth, 1.0 / p1.depth, 1.0 / p2.depth],
        bounds: [min_x as u32, min_y as u32, max_x as u32, max_y as u32],
    })
}

fn draw_triangle(
    color: &mut RgbaImage,
    depth_buffer: &mut [f32],
    scene: &Scene,
    triangle: &ScreenTriangle,
    width: u32,
) {
    let [p0xy, p1xy, p2xy] = triangle.points.map(|p| Vec2::new(p.x, p.y));
    let [min_x, min_y, max_x, max_y] = triangle.bounds;
    let ScreenTriangle {
        area, shade, inv_z, ..
    } = *triangle;
    let texture = scene.texture(triangle.texture);

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let sample = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
            let mut w0 = edge(p1xy, p2xy, sample) / area;
            let mut w1 = edge(p2xy, p0xy, sample) / area;
            let mut w2 = edge(p0xy, p1xy, sample) / area;
            if w0 < -0.000_01 || w1 < -0.000_01 || w2 < -0.000_01 {
                continue;
            }
            w0 = w0.clamp(0.0, 1.0);
            w1 = w1.clamp(0.0, 1.0);
            w2 = w2.clamp(0.0, 1.0);
            let sum = w0 + w1 + w2;
            if sum <= 0.0 {
                continue;
            }
            w0 /= sum;
            w1 /= sum;
            w2 /= sum;

            let interpolated_inv_z = w0 * inv_z[0] + w1 * inv_z[1] + w2 * inv_z[2];
            if interpolated_inv_z <= 0.0 {
                continue;
            }
            let z = 1.0 / interpolated_inv_z;
            let index = y as usize * width as usize + x as usize;
            if z > depth_buffer[index] + 1e-4 {
                continue;
            }
            let u = (w0 * triangle.uvs[0].x * inv_z[0]
                + w1 * triangle.uvs[1].x * inv_z[1]
                + w2 * triangle.uvs[2].x * inv_z[2])
                / interpolated_inv_z;
            let v = (w0 * triangle.uvs[0].y * inv_z[0]
                + w1 * triangle.uvs[1].y * inv_z[1]
                + w2 * triangle.uvs[2].y * inv_z[2])
                / interpolated_inv_z;
            let texel = texture.sample_nearest(u, v);
            if texel[3] == 0 {
                continue;
            }
            depth_buffer[index] = z;
            let lit = [
                (texel[0] as f32 * shade).round().clamp(0.0, 255.0) as u8,
                (texel[1] as f32 * shade).round().clamp(0.0, 255.0) as u8,
                (texel[2] as f32 * shade).round().clamp(0.0, 255.0) as u8,
                texel[3],
            ];
            let dst = color.get_pixel_mut(x, y);
            *dst = blend(*dst, lit);
        }
    }
}

fn draw_sprites(color: &mut RgbaImage, camera: &Camera, scene: &Scene, width: u32, height: u32) {
    for sprite in &scene.sprites {
        let Some(center) = camera.project(sprite.center) else {
            continue;
        };
        let texture = scene.texture(sprite.texture);
        let aspect = texture.width() as f32 / texture.height().max(1) as f32;
        let mut screen_height = camera.projected_size(sprite.world_height, center.depth);
        if screen_height <= 0.0 {
            continue;
        }
        let mut screen_width = screen_height * aspect;
        // This is the viewport width in screen pixels, not a world-space
        // width. The perspective projection already happened above.
        let visible_width = width as f32;
        let max_width = visible_width * sprite.max_width_ratio;
        if screen_width > max_width && screen_width > 0.0 {
            let factor = max_width / screen_width;
            screen_width *= factor;
            screen_height *= factor;
        }
        let left = center.x - screen_width * 0.5;
        let top = center.y - screen_height * 0.5;
        let x0 = left.floor().max(0.0) as i32;
        let x1 = (left + screen_width).ceil().min(width as f32) as i32;
        let y0 = top.floor().max(0.0) as i32;
        let y1 = (top + screen_height).ceil().min(height as f32) as i32;
        for y in y0..y1 {
            let v = ((y as f32 + 0.5 - top) / screen_height).clamp(0.0, 0.999_999);
            for x in x0..x1 {
                let u = ((x as f32 + 0.5 - left) / screen_width).clamp(0.0, 0.999_999);
                let texel = texture.sample_nearest(u, v);
                if texel[3] == 0 {
                    continue;
                }
                let dst = color.get_pixel_mut(x as u32, y as u32);
                *dst = blend(*dst, texel.0);
            }
        }
    }
}

fn edge(a: Vec2, b: Vec2, c: Vec2) -> f32 {
    (c.x - a.x) * (b.y - a.y) - (c.y - a.y) * (b.x - a.x)
}

fn blend(dst: Rgba<u8>, src: [u8; 4]) -> Rgba<u8> {
    let sa = src[3] as f32 / 255.0;
    if sa >= 1.0 {
        return Rgba(src);
    }
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
