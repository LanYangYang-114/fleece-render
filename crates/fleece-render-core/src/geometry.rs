use crate::math::{Mat4, Vec2, Vec3};
use crate::texture::Texture;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextureId(pub usize);

#[derive(Clone, Copy, Debug)]
pub struct Triangle {
    pub positions: [Vec3; 3],
    pub uvs: [Vec2; 3],
    pub texture: TextureId,
    pub double_sided: bool,
    pub alpha_blend: bool,
}

#[derive(Clone, Debug)]
pub struct Sprite {
    pub texture: TextureId,
    pub center: Vec3,
    pub world_height: f32,
    pub max_width_ratio: f32,
}

#[derive(Default)]
pub struct Scene {
    pub textures: Vec<Texture>,
    pub triangles: Vec<Triangle>,
    pub sprites: Vec<Sprite>,
}

impl Scene {
    pub fn add_texture(&mut self, texture: Texture) -> TextureId {
        let id = TextureId(self.textures.len());
        self.textures.push(texture);
        id
    }

    pub fn texture(&self, id: TextureId) -> &Texture {
        &self.textures[id.0]
    }
}

#[derive(Clone, Copy, Debug)]
struct Uv {
    x: f32,
    y: f32,
}

#[derive(Clone, Copy, Debug)]
struct FaceUvs {
    top_left: Uv,
    top_right: Uv,
    bottom_left: Uv,
    bottom_right: Uv,
}

/// Reproduces skinview3d's `setUVs` box layout.
#[derive(Clone, Copy, Debug)]
pub struct BoxUvs {
    top: FaceUvs,
    bottom: FaceUvs,
    left: FaceUvs,
    front: FaceUvs,
    right: FaceUvs,
    back: FaceUvs,
}

impl BoxUvs {
    pub fn new(
        u: f32,
        v: f32,
        width: f32,
        height: f32,
        depth: f32,
        texture_width: f32,
        texture_height: f32,
    ) -> Self {
        let rect = |x1: f32, y1: f32, x2: f32, y2: f32| FaceUvs {
            top_left: Uv {
                x: x1 / texture_width,
                y: y1 / texture_height,
            },
            top_right: Uv {
                x: x2 / texture_width,
                y: y1 / texture_height,
            },
            bottom_left: Uv {
                x: x1 / texture_width,
                y: y2 / texture_height,
            },
            bottom_right: Uv {
                x: x2 / texture_width,
                y: y2 / texture_height,
            },
        };
        Self {
            top: rect(u + depth, v, u + width + depth, v + depth),
            bottom: rect(u + width + depth, v, u + width * 2.0 + depth, v + depth),
            left: rect(u, v + depth, u + depth, v + depth + height),
            front: rect(u + depth, v + depth, u + width + depth, v + depth + height),
            right: rect(
                u + width + depth,
                v + depth,
                u + width + depth * 2.0,
                v + height + depth,
            ),
            back: rect(
                u + width + depth * 2.0,
                v + depth,
                u + width * 2.0 + depth * 2.0,
                v + height + depth,
            ),
        }
    }
}

pub struct BoxStyle {
    pub uvs: BoxUvs,
    pub double_sided: bool,
    pub alpha_blend: bool,
}

pub fn push_box(
    scene: &mut Scene,
    texture: TextureId,
    transform: Mat4,
    size: Vec3,
    style: &BoxStyle,
) {
    let half = size * 0.5;
    let p = |x: f32, y: f32, z: f32| {
        transform.transform_point(Vec3::new(x * half.x, y * half.y, z * half.z))
    };

    // Each face is ordered top-left, top-right, bottom-left, bottom-right
    // when viewed from outside the box, all in world space.
    let faces = [
        (
            [
                p(-1.0, 1.0, 1.0),
                p(1.0, 1.0, 1.0),
                p(-1.0, -1.0, 1.0),
                p(1.0, -1.0, 1.0),
            ],
            [
                style.uvs.front.top_left,
                style.uvs.front.top_right,
                style.uvs.front.bottom_left,
                style.uvs.front.bottom_right,
            ],
        ),
        (
            [
                p(1.0, 1.0, -1.0),
                p(-1.0, 1.0, -1.0),
                p(1.0, -1.0, -1.0),
                p(-1.0, -1.0, -1.0),
            ],
            [
                style.uvs.back.top_left,
                style.uvs.back.top_right,
                style.uvs.back.bottom_left,
                style.uvs.back.bottom_right,
            ],
        ),
        (
            [
                p(1.0, 1.0, 1.0),
                p(1.0, 1.0, -1.0),
                p(1.0, -1.0, 1.0),
                p(1.0, -1.0, -1.0),
            ],
            [
                style.uvs.right.top_left,
                style.uvs.right.top_right,
                style.uvs.right.bottom_left,
                style.uvs.right.bottom_right,
            ],
        ),
        (
            [
                p(-1.0, 1.0, -1.0),
                p(-1.0, 1.0, 1.0),
                p(-1.0, -1.0, -1.0),
                p(-1.0, -1.0, 1.0),
            ],
            [
                style.uvs.left.top_left,
                style.uvs.left.top_right,
                style.uvs.left.bottom_left,
                style.uvs.left.bottom_right,
            ],
        ),
        (
            [
                p(-1.0, 1.0, -1.0),
                p(1.0, 1.0, -1.0),
                p(-1.0, 1.0, 1.0),
                p(1.0, 1.0, 1.0),
            ],
            [
                style.uvs.top.top_left,
                style.uvs.top.top_right,
                style.uvs.top.bottom_left,
                style.uvs.top.bottom_right,
            ],
        ),
        (
            [
                p(-1.0, -1.0, 1.0),
                p(1.0, -1.0, 1.0),
                p(-1.0, -1.0, -1.0),
                p(1.0, -1.0, -1.0),
            ],
            [
                style.uvs.bottom.top_left,
                style.uvs.bottom.top_right,
                style.uvs.bottom.bottom_left,
                style.uvs.bottom.bottom_right,
            ],
        ),
    ];

    for (positions, uvs) in faces {
        let uvs = uvs.map(|uv| Vec2::new(uv.x, uv.y));
        for indices in [[0usize, 2, 1], [2, 3, 1]] {
            scene.triangles.push(Triangle {
                positions: [
                    positions[indices[0]],
                    positions[indices[1]],
                    positions[indices[2]],
                ],
                uvs: [uvs[indices[0]], uvs[indices[1]], uvs[indices[2]]],
                texture,
                double_sided: style.double_sided,
                alpha_blend: style.alpha_blend,
            });
        }
    }
}

pub fn skin_uvs(u: f32, v: f32, width: f32, height: f32, depth: f32) -> BoxUvs {
    BoxUvs::new(u, v, width, height, depth, 64.0, 64.0)
}

pub fn cape_uvs(u: f32, v: f32, width: f32, height: f32, depth: f32) -> BoxUvs {
    BoxUvs::new(u, v, width, height, depth, 64.0, 32.0)
}
