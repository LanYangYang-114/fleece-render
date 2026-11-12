use image::{Rgba, RgbaImage};

use crate::error::Result;
use crate::model::ModelKind;
use crate::render::{RenderStyle, blend_pixel};
use crate::types::{RenderOptions, SkinAssets};

const SIZE: u32 = 64;

/// PCL-style avatar with a larger hat layer centered over the face.
#[derive(Default)]
pub struct AvatarStyle;

impl AvatarStyle {
    pub const ID: &str = "avatar";
}

impl RenderStyle for AvatarStyle {
    fn id(&self) -> &'static str {
        Self::ID
    }

    fn label(&self) -> &'static str {
        "PCL-style 2D avatar (64x64)"
    }

    fn render_flat(
        &self,
        assets: &SkinAssets,
        options: &RenderOptions,
    ) -> Result<Option<(RgbaImage, ModelKind)>> {
        let skin = &assets.skin;
        skin.validate_skin_layout()?;
        let scale = skin.width() / 64;
        let model = if skin.width() == skin.height() {
            skin.infer_model()
        } else {
            ModelKind::Classic
        };
        let pixels = skin.pixels();
        let background = if options.transparent {
            Rgba([0, 0, 0, 0])
        } else {
            Rgba([255, 255, 255, 255])
        };
        let mut image = RgbaImage::from_pixel(SIZE, SIZE, background);
        draw_layer(&mut image, pixels, 8, scale, 48);
        if options.show_outer_layer && has_hat_layer(pixels, scale) {
            draw_layer(&mut image, pixels, 40, scale, 56);
        }
        Ok(Some((image, model)))
    }
}

fn has_hat_layer(skin: &RgbaImage, scale: u32) -> bool {
    // Match PCL's heuristic for old skins with an opaque filler instead of a hat.
    let samples = [
        skin.get_pixel(1, 1),
        skin.get_pixel(skin.width() - 1, skin.height() - 1),
        skin.get_pixel(skin.width() - 2, skin.height() / 2 - 2),
    ];
    let hat = skin.get_pixel(41 * scale, 9 * scale);
    samples.iter().any(|pixel| pixel[3] == 0) || !samples.contains(&hat)
}

fn draw_layer(output: &mut RgbaImage, skin: &RgbaImage, x: u32, scale: u32, side: u32) {
    let inset = (SIZE - side) / 2;
    let source_side = 8 * scale;
    // Sample the source texel nearest the center of each destination pixel.
    let nearest =
        |p: u32| (u64::from(2 * p + 1) * u64::from(source_side) / u64::from(2 * side)) as u32;
    for dy in 0..side {
        for dx in 0..side {
            let src = skin.get_pixel(x * scale + nearest(dx), 8 * scale + nearest(dy));
            let dst = output.get_pixel_mut(inset + dx, inset + dy);
            *dst = blend_pixel(*dst, src.0);
        }
    }
}
