use fleece_render_core::{BackendKind, CoreError, RenderEngine, available_backends};

#[cfg(not(feature = "gpu"))]
#[test]
fn cpu_only_build_does_not_offer_gpu() {
    assert_eq!(available_backends(), ["cpu"]);
    assert_eq!(
        RenderEngine::new(BackendKind::Auto).unwrap().backend_kind(),
        BackendKind::Cpu
    );
    assert!(matches!(
        RenderEngine::new(BackendKind::Gpu),
        Err(CoreError::BackendUnavailable(_))
    ));
}

#[cfg(feature = "gpu")]
#[test]
fn gpu_matches_cpu_visually_and_is_repeatable() {
    use fleece_render_core::{
        BackEquipment, ModelOverride, Parts, RenderOptions, SkinAssets, Texture,
    };

    assert_eq!(available_backends(), ["cpu", "gpu"]);
    let gpu = match RenderEngine::new(BackendKind::Gpu) {
        Ok(engine) => engine,
        Err(CoreError::BackendUnavailable(message))
            if message.contains("no Vulkan hardware adapter found") =>
        {
            assert_eq!(
                RenderEngine::new(BackendKind::Auto).unwrap().backend_kind(),
                BackendKind::Cpu
            );
            assert!(
                std::env::var_os("FLEECE_RENDER_REQUIRE_GPU").is_none(),
                "{message}"
            );
            eprintln!("Skipping GPU render test: {message}");
            return;
        }
        Err(error) => panic!("GPU initialization failed: {error}"),
    };
    assert_eq!(gpu.backend_kind(), BackendKind::Gpu);
    assert_eq!(
        RenderEngine::new(BackendKind::Auto).unwrap().backend_kind(),
        BackendKind::Gpu
    );
    let cpu = RenderEngine::new(BackendKind::Cpu).unwrap();
    let assets = SkinAssets {
        skin: Texture::from_bytes(include_bytes!("fixtures/source_skin.png"), "skin").unwrap(),
        back: Some(
            Texture::from_bytes(include_bytes!("fixtures/source_cape.png"), "cape").unwrap(),
        ),
        display_name: Some("GPU Test".to_owned()),
    };
    for (equipment, model, transparent) in [
        (BackEquipment::Cape, ModelOverride::Classic, true),
        (BackEquipment::Elytra, ModelOverride::Slim, false),
    ] {
        let options = RenderOptions {
            parts: Parts::All,
            panel_width: 175,
            panel_height: 335,
            definition: 1.0,
            back_equipment: equipment,
            model,
            transparent,
            ..RenderOptions::default()
        };
        let expected = cpu.render(&assets, &options).unwrap();
        let actual = gpu.render(&assets, &options).unwrap();
        let repeated = gpu.render(&assets, &options).unwrap();
        assert_eq!(actual.info.backend, "gpu");
        for (expected, actual, repeated) in [
            (expected.front, actual.front, repeated.front),
            (expected.back, actual.back, repeated.back),
            (expected.both, actual.both, repeated.both),
        ] {
            let (expected, actual, repeated) =
                (expected.unwrap(), actual.unwrap(), repeated.unwrap());
            assert_eq!(actual.bytes, repeated.bytes);
            let decode = |bytes: &[u8]| image::load_from_memory(bytes).unwrap().to_rgba8();
            let cpu = decode(&expected.bytes);
            let gpu = decode(&actual.bytes);
            assert_eq!(gpu.dimensions(), cpu.dimensions());
            assert_eq!(gpu.get_pixel(0, 0)[3], if transparent { 0 } else { 255 });
            let different = cpu
                .pixels()
                .zip(gpu.pixels())
                .filter(|(a, b)| a.0.iter().zip(b.0).any(|(a, b)| a.abs_diff(b) > 2))
                .count();
            let pixels = gpu.width() as usize * gpu.height() as usize;
            // GPU floating-point behavior can change samples at triangle/texel boundaries.
            assert!(
                different * 200 < pixels,
                "{different}/{pixels} pixels differ materially"
            );
        }
    }
}
