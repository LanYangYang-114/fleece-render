# fleece-render-core

A headless Rust renderer for Minecraft skins: 3D front/back views, capes,
elytra, name tags, and 64×64 layered avatars. CPU rendering requires no GPU;
the optional `gpu` feature adds a Vulkan compute backend for 3D views.

Use `RenderEngine`, `RenderOptions`, and `SkinAssets` to render PNG images.
See [`examples/render_file.rs`](examples/render_file.rs) for a CLI example.
For the HTTP service and project documentation, see the
[fleece-render repository](https://github.com/LanYangYang-114/fleece-render).
