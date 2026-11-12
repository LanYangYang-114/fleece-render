use std::time::Instant;

use fleece_render_core::{
    BackEquipment, BackendKind, ModelKind, Parts, RenderEngine, RenderOptions, SkinAssets, Texture,
};
use image::{Rgba, RgbaImage};

const SKIN: &[u8] = include_bytes!("fixtures/source_skin.png");
const CAPE: &[u8] = include_bytes!("fixtures/source_cape.png");

fn assets(name: Option<&str>, back: bool) -> SkinAssets {
    let skin = Texture::from_bytes(SKIN, "skin")
        .unwrap()
        .prepare_skin()
        .unwrap();
    let back = if back {
        Some(
            Texture::from_bytes(CAPE, "cape")
                .unwrap()
                .prepare_back("cape")
                .unwrap(),
        )
    } else {
        None
    };
    SkinAssets {
        skin,
        back,
        display_name: name.map(str::to_owned),
    }
}

fn engine() -> RenderEngine {
    RenderEngine::new(BackendKind::Cpu).unwrap()
}

#[test]
fn renders_front_back_and_both_with_expected_dimensions_and_content() {
    let engine = engine();
    let started = Instant::now();
    let result = engine
        .render(
            &assets(Some("LanYangYang_CN"), false),
            &RenderOptions {
                parts: Parts::All,
                ..RenderOptions::default()
            },
        )
        .unwrap();
    let elapsed = started.elapsed().as_secs_f64();
    eprintln!(
        "all parts rendered in {elapsed:.3}s: {}",
        result.info.elapsed_ms
    );
    assert!(elapsed < 5.0, "render exceeded five seconds: {elapsed:.3}s");

    let front = result.front.as_ref().unwrap();
    let back = result.back.as_ref().unwrap();
    let both = result.both.as_ref().unwrap();
    assert_eq!((front.width, front.height), (525, 1005));
    assert_eq!((back.width, back.height), (525, 1005));
    assert_eq!((both.width, both.height), (1050, 1005));
    assert_ne!(front.bytes, back.bytes);
    assert!(front.bytes.len() > 10_000);
    assert!(both.bytes.len() > front.bytes.len());
    assert_eq!(result.info.model, "classic");
    assert_eq!(result.info.backend, "cpu");
}

#[test]
fn transparent_background_alpha_and_name_tag_are_effective() {
    let engine = engine();
    let transparent = engine
        .render(
            &assets(None, false),
            &RenderOptions {
                parts: Parts::Front,
                transparent: true,
                show_name: false,
                definition: 0.5,
                ..RenderOptions::default()
            },
        )
        .unwrap();
    let image = image::load_from_memory(&transparent.front.unwrap().bytes)
        .unwrap()
        .to_rgba8();
    assert!(image.pixels().any(|pixel| pixel[3] == 0));
    assert!(image.pixels().any(|pixel| pixel[3] == 255));

    let named = engine
        .render(
            &assets(Some("LanYangYang_CN"), false),
            &RenderOptions {
                parts: Parts::Front,
                definition: 0.5,
                ..RenderOptions::default()
            },
        )
        .unwrap();
    let unnamed = engine
        .render(
            &assets(None, false),
            &RenderOptions {
                parts: Parts::Front,
                definition: 0.5,
                ..RenderOptions::default()
            },
        )
        .unwrap();
    assert_ne!(named.front.unwrap().bytes, unnamed.front.unwrap().bytes);
}

#[test]
fn cape_and_elytra_take_different_geometry_paths() {
    let engine = engine();
    let cape = engine
        .render(
            &assets(None, true),
            &RenderOptions {
                parts: Parts::Back,
                show_name: false,
                definition: 0.5,
                back_equipment: BackEquipment::Cape,
                ..RenderOptions::default()
            },
        )
        .unwrap();
    let elytra = engine
        .render(
            &assets(None, true),
            &RenderOptions {
                parts: Parts::Back,
                show_name: false,
                definition: 0.5,
                back_equipment: BackEquipment::Elytra,
                ..RenderOptions::default()
            },
        )
        .unwrap();
    assert_ne!(cape.back.unwrap().bytes, elytra.back.unwrap().bytes);
}

#[test]
fn single_layer_omits_all_six_3d_overlays_but_keeps_the_cape() {
    let engine = engine();
    let skin = assets(Some("Player"), true);
    let mut options = RenderOptions {
        parts: Parts::All,
        definition: 0.5,
        ..RenderOptions::default()
    };
    let layered = engine.render(&skin, &options).unwrap();
    options.show_outer_layer = false;
    let base = engine.render(&skin, &options).unwrap();

    // Two views, 12 triangles per box, six base boxes and one cape per view.
    assert_eq!(base.info.triangles, 2 * 12 * (6 + 1));
    assert_eq!(layered.info.triangles - base.info.triangles, 2 * 12 * 6);
    for (base, layered) in [
        (base.front, layered.front),
        (base.back, layered.back),
        (base.both, layered.both),
    ] {
        let (base, layered) = (base.unwrap(), layered.unwrap());
        assert_eq!((base.width, base.height), (layered.width, layered.height));
        assert_ne!(base.bytes, layered.bytes);
    }
}

#[test]
fn repeated_render_is_deterministic() {
    let engine = engine();
    let options = RenderOptions {
        parts: Parts::Front,
        show_name: false,
        definition: 0.5,
        ..RenderOptions::default()
    };
    let first = engine.render(&assets(None, false), &options).unwrap();
    let second = engine.render(&assets(None, false), &options).unwrap();
    assert_eq!(first.front.unwrap().bytes, second.front.unwrap().bytes);
}

#[test]
fn slim_detection_uses_skinview_utils_rules() {
    let mut classic = RgbaImage::from_pixel(64, 64, Rgba([20, 30, 40, 255]));
    let classic_texture = Texture::from_rgba(classic.clone()).prepare_skin().unwrap();
    assert_eq!(classic_texture.infer_model(), ModelKind::Classic);

    classic.put_pixel(50, 16, Rgba([0, 0, 0, 0]));
    let slim_texture = Texture::from_rgba(classic).prepare_skin().unwrap();
    assert_eq!(slim_texture.infer_model(), ModelKind::Slim);
}

#[test]
fn legacy_skin_is_upgraded_to_square_layout() {
    let legacy = RgbaImage::from_pixel(64, 32, Rgba([15, 25, 35, 255]));
    let texture = Texture::from_rgba(legacy).prepare_skin().unwrap();
    assert_eq!((texture.width(), texture.height()), (64, 64));
}
