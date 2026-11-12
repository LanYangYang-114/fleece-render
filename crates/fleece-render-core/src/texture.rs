use std::io::Cursor;

use image::{ImageFormat, ImageReader, Limits, Rgba, RgbaImage};

use crate::error::{CoreError, Result};
use crate::model::ModelKind;

/// An RGBA texture with a small Minecraft-specific validation layer.
#[derive(Clone)]
pub struct Texture {
    image: RgbaImage,
}

impl Texture {
    pub fn from_bytes(bytes: &[u8], role: &str) -> Result<Self> {
        if bytes.is_empty() {
            return Err(CoreError::Texture(format!("{role} is empty")));
        }
        let decode = || -> image::ImageResult<_> {
            let mut reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
            let mut limits = Limits::default();
            limits.max_image_width = Some(4096);
            limits.max_image_height = Some(4096);
            limits.max_alloc = Some(128 * 1024 * 1024);
            reader.limits(limits);
            reader.decode()
        };
        let decoded = decode()
            .map_err(|error| CoreError::Texture(format!("cannot decode {role}: {error}")))?;
        Ok(Self {
            image: decoded.to_rgba8(),
        })
    }

    pub fn from_rgba(image: RgbaImage) -> Self {
        Self { image }
    }

    pub fn width(&self) -> u32 {
        self.image.width()
    }

    pub fn height(&self) -> u32 {
        self.image.height()
    }

    pub fn pixels(&self) -> &RgbaImage {
        &self.image
    }

    pub fn sample_nearest(&self, u: f32, v: f32) -> Rgba<u8> {
        let w = self.image.width().max(1);
        let h = self.image.height().max(1);
        let x = (u.clamp(0.0, 0.999_999) * w as f32).floor() as u32;
        let y = (v.clamp(0.0, 0.999_999) * h as f32).floor() as u32;
        *self.image.get_pixel(x.min(w - 1), y.min(h - 1))
    }

    /// Normalize modern/HD/legacy skin textures to the same 1.8+ layout.
    pub fn prepare_skin(mut self) -> Result<Self> {
        self.validate_skin_layout()?;
        if self.width() != self.height() {
            self.image = upgrade_legacy_skin(&self.image)?;
        }
        fix_opaque_skin(&mut self.image);
        Ok(self)
    }

    pub(crate) fn validate_skin_layout(&self) -> Result<()> {
        let w = self.width();
        let h = self.height();
        if w != h && w != h.saturating_mul(2) {
            return Err(CoreError::Texture(format!(
                "bad skin size: {w}x{h}; expected square or 2:1 legacy skin"
            )));
        }
        if w < 64 || !w.is_multiple_of(64) {
            return Err(CoreError::Texture(format!(
                "bad skin width: {w}; expected a multiple of 64"
            )));
        }
        Ok(())
    }

    pub fn prepare_back(&self, role: &str) -> Result<Self> {
        let w = self.image.width();
        let h = self.image.height();
        if w == 0 || h == 0 {
            return Err(CoreError::Texture(format!("{role} has no pixels")));
        }
        // Cape and elytra textures are commonly 64x32, 22x17 or HD multiples.
        if w != 2 * h && !(w == h && w.is_multiple_of(64)) && !(w == 22 && h == 17) {
            return Err(CoreError::Texture(format!(
                "bad {role} size: {w}x{h}; expected 2:1, square HD, or 22x17"
            )));
        }
        Ok(self.clone())
    }

    pub fn infer_model(&self) -> ModelKind {
        let width = self.image.width();
        let scale = width as f32 / 64.0;
        let has_transparency =
            |x: f32, y: f32, w: f32, h: f32| area_has_transparency(&self.image, x, y, w, h, scale);
        let is_black =
            |x: f32, y: f32, w: f32, h: f32| area_is_black(&self.image, x, y, w, h, scale);
        let is_white =
            |x: f32, y: f32, w: f32, h: f32| area_is_white(&self.image, x, y, w, h, scale);

        let unused = [
            (50.0, 16.0, 2.0, 4.0),
            (54.0, 20.0, 2.0, 12.0),
            (42.0, 48.0, 2.0, 4.0),
            (46.0, 52.0, 2.0, 12.0),
        ];
        let slim = unused
            .iter()
            .any(|&(x, y, w, h)| has_transparency(x, y, w, h))
            || unused.iter().all(|&(x, y, w, h)| is_black(x, y, w, h))
            || unused.iter().all(|&(x, y, w, h)| is_white(x, y, w, h));
        if slim {
            ModelKind::Slim
        } else {
            ModelKind::Classic
        }
    }

    pub fn encode_png(&self) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        self.image
            .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)?;
        Ok(bytes)
    }
}

fn upgrade_legacy_skin(source: &RgbaImage) -> Result<RgbaImage> {
    let width = source.width();
    if width == 0 || source.height() != width / 2 {
        return Err(CoreError::Texture(
            "invalid legacy skin dimensions".to_owned(),
        ));
    }
    let mut output = RgbaImage::new(width, width);
    for y in 0..source.height() {
        for x in 0..width {
            output.put_pixel(x, y, *source.get_pixel(x, y));
        }
    }
    let scale = width as f32 / 64.0;
    let copies = [
        (4.0, 16.0, 4.0, 4.0, 20.0, 48.0),
        (8.0, 16.0, 4.0, 4.0, 24.0, 48.0),
        (0.0, 20.0, 4.0, 12.0, 24.0, 52.0),
        (4.0, 20.0, 4.0, 12.0, 20.0, 52.0),
        (8.0, 20.0, 4.0, 12.0, 16.0, 52.0),
        (12.0, 20.0, 4.0, 12.0, 28.0, 52.0),
        (44.0, 16.0, 4.0, 4.0, 36.0, 48.0),
        (48.0, 16.0, 4.0, 4.0, 40.0, 48.0),
        (40.0, 20.0, 4.0, 12.0, 40.0, 52.0),
        (44.0, 20.0, 4.0, 12.0, 36.0, 52.0),
        (48.0, 20.0, 4.0, 12.0, 32.0, 52.0),
        (52.0, 20.0, 4.0, 12.0, 44.0, 52.0),
    ];
    for (sx, sy, w, h, dx, dy) in copies {
        copy_flipped(
            &mut output,
            source,
            (sx * scale) as u32,
            (sy * scale) as u32,
            (w * scale) as u32,
            (h * scale) as u32,
            (dx * scale) as u32,
            (dy * scale) as u32,
        );
    }
    Ok(output)
}

#[allow(clippy::too_many_arguments)] // The source/destination rectangle is naturally eight values.
fn copy_flipped(
    dst: &mut RgbaImage,
    src: &RgbaImage,
    sx: u32,
    sy: u32,
    w: u32,
    h: u32,
    dx: u32,
    dy: u32,
) {
    for y in 0..h {
        for x in 0..w {
            let src_x = sx.saturating_add(w.saturating_sub(1).saturating_sub(x));
            let src_y = sy + y;
            if src_x < src.width() && src_y < src.height() {
                let pixel = *src.get_pixel(src_x, src_y);
                if dx + x < dst.width() && dy + y < dst.height() {
                    dst.put_pixel(dx + x, dy + y, pixel);
                }
            }
        }
    }
}

fn fix_opaque_skin(image: &mut RgbaImage) {
    let width = image.width();
    let height = image.height();
    if area_has_transparency(image, 0.0, 0.0, width as f32, height as f32, 1.0) {
        return;
    }
    let scale = width as f32 / 64.0;
    let regions = [
        (40.0, 0.0, 8.0, 8.0),
        (48.0, 0.0, 8.0, 8.0),
        (32.0, 8.0, 8.0, 8.0),
        (40.0, 8.0, 8.0, 8.0),
        (48.0, 8.0, 8.0, 8.0),
        (56.0, 8.0, 8.0, 8.0),
    ];
    let overlay_regions = [
        (4.0, 32.0, 4.0, 4.0),
        (8.0, 32.0, 4.0, 4.0),
        (0.0, 36.0, 4.0, 12.0),
        (4.0, 36.0, 4.0, 12.0),
        (8.0, 36.0, 4.0, 12.0),
        (12.0, 36.0, 4.0, 12.0),
        (20.0, 32.0, 8.0, 4.0),
        (28.0, 32.0, 8.0, 4.0),
        (16.0, 36.0, 4.0, 12.0),
        (20.0, 36.0, 8.0, 12.0),
        (28.0, 36.0, 4.0, 12.0),
        (32.0, 36.0, 8.0, 12.0),
        (44.0, 32.0, 4.0, 4.0),
        (48.0, 32.0, 4.0, 4.0),
        (40.0, 36.0, 4.0, 12.0),
        (44.0, 36.0, 4.0, 12.0),
        (48.0, 36.0, 4.0, 12.0),
        (52.0, 36.0, 12.0, 12.0),
        (4.0, 48.0, 4.0, 4.0),
        (8.0, 48.0, 4.0, 4.0),
        (0.0, 52.0, 4.0, 12.0),
        (4.0, 52.0, 4.0, 12.0),
        (8.0, 52.0, 4.0, 12.0),
        (12.0, 52.0, 4.0, 12.0),
        (52.0, 48.0, 4.0, 4.0),
        (56.0, 48.0, 4.0, 4.0),
        (48.0, 52.0, 4.0, 12.0),
        (52.0, 52.0, 4.0, 12.0),
        (56.0, 52.0, 4.0, 12.0),
        (60.0, 52.0, 4.0, 12.0),
    ];
    for region in regions.iter().chain(overlay_regions.iter()) {
        clear_region(image, region.0, region.1, region.2, region.3, scale);
    }
}

fn clear_region(image: &mut RgbaImage, x: f32, y: f32, w: f32, h: f32, scale: f32) {
    let x0 = (x * scale).floor() as u32;
    let y0 = (y * scale).floor() as u32;
    let x1 = ((x + w) * scale).ceil() as u32;
    let y1 = ((y + h) * scale).ceil() as u32;
    for py in y0..y1.min(image.height()) {
        for px in x0..x1.min(image.width()) {
            image.put_pixel(px, py, Rgba([0, 0, 0, 0]));
        }
    }
}

fn area_has_transparency(image: &RgbaImage, x: f32, y: f32, w: f32, h: f32, scale: f32) -> bool {
    let x0 = (x * scale).floor() as u32;
    let y0 = (y * scale).floor() as u32;
    let x1 = ((x + w) * scale).ceil() as u32;
    let y1 = ((y + h) * scale).ceil() as u32;
    for py in y0..y1.min(image.height()) {
        for px in x0..x1.min(image.width()) {
            if image.get_pixel(px, py)[3] != 255 {
                return true;
            }
        }
    }
    false
}

fn area_is_black(image: &RgbaImage, x: f32, y: f32, w: f32, h: f32, scale: f32) -> bool {
    area_is_uniform(image, x, y, w, h, scale, [0, 0, 0, 255])
}

fn area_is_white(image: &RgbaImage, x: f32, y: f32, w: f32, h: f32, scale: f32) -> bool {
    area_is_uniform(image, x, y, w, h, scale, [255, 255, 255, 255])
}

fn area_is_uniform(
    image: &RgbaImage,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    scale: f32,
    expected: [u8; 4],
) -> bool {
    let x0 = (x * scale).floor() as u32;
    let y0 = (y * scale).floor() as u32;
    let x1 = ((x + w) * scale).ceil() as u32;
    let y1 = ((y + h) * scale).ceil() as u32;
    for py in y0..y1.min(image.height()) {
        for px in x0..x1.min(image.width()) {
            if image.get_pixel(px, py).0 != expected {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texture_decoder_rejects_oversized_dimensions() {
        let image = Texture::from_rgba(RgbaImage::from_pixel(4097, 1, Rgba([255; 4])));
        let encoded = image.encode_png().unwrap();
        assert!(matches!(
            Texture::from_bytes(&encoded, "skin"),
            Err(CoreError::Texture(_))
        ));
    }
}
