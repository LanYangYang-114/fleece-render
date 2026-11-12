use fontdue::Font;
use image::{Rgba, RgbaImage};

use crate::error::{CoreError, Result};
use crate::texture::Texture;

const FONT_BYTES: &[u8] = include_bytes!("../assets/minecraft.ttf");
const FONT_PX: f32 = 48.0;
const MARGIN_X: u32 = 10;
const MARGIN_Y: u32 = 5;

/// Rasterize a Minecraft-style name tag. The result is cached by the style
/// layer when the same display name is reused across views.
pub fn render_name_tag(text: &str) -> Result<Texture> {
    let font = Font::from_bytes(FONT_BYTES, fontdue::FontSettings::default())
        .map_err(|error| CoreError::Render(format!("embedded font is invalid: {error}")))?;

    let mut glyphs = Vec::new();
    let mut pen_x = 0.0f32;
    let mut min_x = f32::MAX;
    let mut max_x = f32::MIN;
    let mut ascent = 0.0f32;
    let mut descent = 0.0f32;
    for character in text.chars() {
        let metrics = font.metrics(character, FONT_PX);
        let glyph_x = pen_x + metrics.xmin as f32;
        let glyph_top = metrics.ymin as f32 + metrics.height as f32;
        if metrics.width > 0 && metrics.height > 0 {
            min_x = min_x.min(glyph_x);
            max_x = max_x.max(glyph_x + metrics.width as f32);
            ascent = ascent.max(glyph_top);
            descent = descent.max((-metrics.ymin as f32).max(0.0));
        }
        glyphs.push((character, metrics, pen_x));
        pen_x += metrics.advance_width;
    }

    if glyphs.is_empty() || !min_x.is_finite() {
        return Ok(Texture::from_rgba(RgbaImage::from_pixel(
            1,
            1,
            Rgba([0, 0, 0, 0]),
        )));
    }

    let content_width = (max_x - min_x).ceil().max(1.0) as u32;
    let content_height = (ascent + descent).ceil().max(1.0) as u32;
    let width = content_width + MARGIN_X * 2;
    let height = content_height + MARGIN_Y * 2;
    let mut canvas = RgbaImage::from_pixel(width, height, Rgba([0, 0, 0, 64]));

    let baseline = MARGIN_Y as f32 + ascent;
    for (character, _metrics, advance) in glyphs {
        let (glyph_metrics, bitmap) = font.rasterize(character, FONT_PX);
        let draw_x = (MARGIN_X as f32 + advance + glyph_metrics.xmin as f32 - min_x).round() as i32;
        let draw_y =
            (baseline - (glyph_metrics.ymin + glyph_metrics.height as i32) as f32).round() as i32;
        for gy in 0..glyph_metrics.height {
            for gx in 0..glyph_metrics.width {
                let coverage = bitmap[gy * glyph_metrics.width + gx];
                if coverage == 0 {
                    continue;
                }
                let x = draw_x + gx as i32;
                let y = draw_y + gy as i32;
                if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
                    continue;
                }
                let alpha = coverage as u16;
                let inverse = 255u16 - alpha;
                let dst = canvas.get_pixel_mut(x as u32, y as u32);
                let out_alpha = alpha + (dst[3] as u16 * inverse) / 255;
                let blend = |src: u8, dst: u8| -> u8 {
                    ((src as u16 * alpha + dst as u16 * inverse) / 255) as u8
                };
                *dst = Rgba([
                    blend(255, dst[0]),
                    blend(255, dst[1]),
                    blend(255, dst[2]),
                    out_alpha.min(255) as u8,
                ]);
            }
        }
    }
    Ok(Texture::from_rgba(canvas))
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_font_has_ascii_metrics_and_bitmaps() {
        let font = Font::from_bytes(FONT_BYTES, fontdue::FontSettings::default()).unwrap();
        assert!(font.has_glyph('L'));
        let metrics = font.metrics('L', FONT_PX);
        let (raster_metrics, bitmap) = font.rasterize('L', FONT_PX);
        eprintln!(
            "metrics={metrics:?} raster={raster_metrics:?} bitmap={}",
            bitmap.len()
        );
        assert!(metrics.advance_width > 0.0);
        assert!(metrics.width > 0);
        assert!(!bitmap.is_empty());
        let tag = render_name_tag("LanYangYang_CN").unwrap();
        eprintln!("tag={}x{}", tag.width(), tag.height());
        assert!(tag.width() > 40);
    }
}
