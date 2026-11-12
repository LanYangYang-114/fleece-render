//! Pure-Rust Minecraft skin rendering core.
//!
//! Renders on the CPU by default, with optional headless Vulkan compute
//! via the `gpu` feature. No browser or window system is required.

mod avatar;
mod backend;
mod camera;
mod error;
mod geometry;
mod math;
mod model;
mod name_tag;
mod render;
mod texture;
mod types;

pub use avatar::AvatarStyle;
pub use backend::{BackendKind, CpuBackend, RenderBackend, available_backends};
pub use error::{CoreError, Result};
pub use geometry::{Scene, Sprite, TextureId, Triangle};
pub use math::{Mat4, Vec2, Vec3};
pub use model::{ModelKind, SkinModel};
pub use render::{RenderEngine, RenderStyle, StyleRegistry, ThreeDStyle};
pub use texture::Texture;
pub use types::{
    BackEquipment, ModelOverride, Parts, RenderInfo, RenderOptions, RenderOutput, RenderedImage,
    SkinAssets, View,
};
