use fleece_render_core::{
    AvatarStyle, BackendKind, CoreError, Parts, RenderEngine, RenderOptions, RenderOutput,
    SkinAssets, Texture,
};
use image::{Rgba, RgbaImage};

const FACE: Rgba<u8> = Rgba([40, 80, 120, 255]);
const HAT: Rgba<u8> = Rgba([240, 0, 20, 128]);
const CLEAR: Rgba<u8> = Rgba([0, 0, 0, 0]);
const WHITE: Rgba<u8> = Rgba([255, 255, 255, 255]);

fn assets(image: RgbaImage) -> SkinAssets {
    SkinAssets {
        skin: Texture::from_rgba(image),
        back: None,
        display_name: Some("Ignored name tag".to_owned()),
    }
}

fn options(transparent: bool) -> RenderOptions {
    RenderOptions {
        style: AvatarStyle::ID.to_owned(),
        parts: Parts::Front,
        definition: 5.0,
        transparent,
        ..RenderOptions::default()
    }
}

fn render(skin: RgbaImage, transparent: bool) -> RenderOutput {
    RenderEngine::new(BackendKind::Cpu)
        .unwrap()
        .render(&assets(skin), &options(transparent))
        .unwrap()
}

fn decode(output: &RenderOutput) -> RgbaImage {
    image::load_from_memory(&output.primary().unwrap().bytes)
        .unwrap()
        .to_rgba8()
}

#[test]
fn avatar_centers_the_two_pixel_art_layers_and_preserves_alpha() {
    let mut skin = RgbaImage::new(64, 64);
    image::imageops::replace(&mut skin, &RgbaImage::from_pixel(8, 8, FACE), 8, 8);
    skin.put_pixel(40, 8, HAT);
    skin.put_pixel(47, 15, HAT);

    let output = render(skin.clone(), true);
    assert_eq!(output.info.style, "avatar");
    assert_eq!(output.info.parts, "front");
    assert_eq!(output.info.triangles, 0);
    assert!(output.back.is_none() && output.both.is_none());
    let png = decode(&output);
    assert_eq!(png.dimensions(), (64, 64));
    assert_eq!(*png.get_pixel(3, 4), CLEAR);
    assert_eq!(*png.get_pixel(4, 4), HAT);
    assert_eq!(*png.get_pixel(10, 4), HAT);
    assert_eq!(*png.get_pixel(11, 4), CLEAR);
    assert_eq!(*png.get_pixel(8, 8), Rgba([140, 40, 70, 255]));
    assert_eq!(*png.get_pixel(7, 14), CLEAR);
    assert_eq!(*png.get_pixel(8, 14), FACE);
    assert_eq!(*png.get_pixel(55, 14), FACE);
    assert_eq!(*png.get_pixel(56, 14), CLEAR);
    assert_eq!(*png.get_pixel(59, 59), HAT);
    assert_eq!(*png.get_pixel(60, 59), CLEAR);

    let opaque = decode(&render(skin, false));
    assert_eq!(opaque.dimensions(), (64, 64));
    assert_eq!(*opaque.get_pixel(0, 0), WHITE);
    assert_eq!(*opaque.get_pixel(4, 4), Rgba([247, 127, 137, 255]));
    assert_eq!(*opaque.get_pixel(8, 8), *png.get_pixel(8, 8));
    assert!(opaque.pixels().all(|pixel| pixel[3] == 255));
}

#[test]
fn avatar_can_hide_the_hat_without_resizing_the_face() {
    let engine = RenderEngine::new(BackendKind::Cpu).unwrap();
    for height in [32, 64] {
        let mut skin = RgbaImage::new(64, height);
        image::imageops::replace(&mut skin, &RgbaImage::from_pixel(8, 8, FACE), 8, 8);
        image::imageops::replace(&mut skin, &RgbaImage::from_pixel(8, 8, HAT), 40, 8);
        let assets = assets(skin);
        for (transparent, background) in [(true, CLEAR), (false, WHITE)] {
            let mut options = options(transparent);
            let layered = decode(&engine.render(&assets, &options).unwrap());
            assert_ne!(*layered.get_pixel(4, 4), background);
            assert_ne!(*layered.get_pixel(8, 8), FACE);

            options.show_outer_layer = false;
            let base = decode(&engine.render(&assets, &options).unwrap());
            assert_eq!(base.dimensions(), (64, 64));
            for (x, y, pixel) in base.enumerate_pixels() {
                let on_face = (8..56).contains(&x) && (8..56).contains(&y);
                assert_eq!(*pixel, if on_face { FACE } else { background });
            }
        }
    }
}

#[test]
fn avatar_uses_pcl_legacy_hat_heuristic_on_the_original_layout() {
    for height in [32, 64] {
        let mut skin = RgbaImage::from_pixel(64, height, WHITE);
        image::imageops::replace(&mut skin, &RgbaImage::from_pixel(8, 8, FACE), 8, 8);
        let without_hat = decode(&render(skin.clone(), true));
        assert_eq!(*without_hat.get_pixel(4, 4), CLEAR);
        assert_eq!(*without_hat.get_pixel(8, 8), FACE);

        let hat = Rgba([20, 10, 30, 255]);
        image::imageops::replace(&mut skin, &RgbaImage::from_pixel(8, 8, hat), 40, 8);
        let with_hat = decode(&render(skin, true));
        assert_eq!(*with_hat.get_pixel(4, 4), hat);
        assert_eq!(*with_hat.get_pixel(8, 8), hat);
    }
}

#[test]
fn avatar_scales_uvs_for_hd_and_hd_legacy_skins() {
    for height in [64, 128] {
        let mut skin = RgbaImage::new(128, height);
        skin.put_pixel(16, 16, FACE);
        skin.put_pixel(80, 16, HAT);
        let png = decode(&render(skin, true));
        assert_eq!(*png.get_pixel(4, 4), HAT);
        assert_eq!(*png.get_pixel(8, 8), FACE);
        assert_eq!(*png.get_pixel(10, 8), FACE);
        assert_eq!(*png.get_pixel(11, 8), CLEAR);
    }
}

#[test]
fn avatar_rejects_invalid_skin_layout() {
    let engine = RenderEngine::new(BackendKind::Cpu).unwrap();
    for (width, height) in [(32, 32), (64, 48), (65, 65)] {
        assert!(matches!(
            engine.render(&assets(RgbaImage::new(width, height)), &options(false)),
            Err(CoreError::Texture(_))
        ));
    }
}
