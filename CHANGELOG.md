# Changelog

## 0.1.0 (unreleased)

- First release under the fleece-render name: `fleece-render` executable, `fleece-render-core` and `fleece-render-server` crates, `fleece-render.toml`, `FLEECE_RENDER_*` environment variables, and `X-Fleece-Render-*` response headers. Routes and request fields are unchanged.
- Loopback-only listener and no permissive CORS by default; the Docker container listens on all container interfaces but Compose publishes only to host loopback. Texture and profile downloads are capped while reading.
- Optional Vulkan acceleration for 3D rendering, with the CPU-only build retained as the default.

- CPU rendering of classic and slim Minecraft models, including skin layers, capes, elytra, name tags, and shadows.
- 64×64 layered avatars with transparent or white backgrounds, plus a skin outer-layer toggle for both render styles.
- Front, back, and side-by-side PNG output, with a structured `/api/v1` interface for JSON and PNG responses.
- Mojang and LittleSkin profile lookup, plus URL, local file, Base64, data URL, and raw image input.
- Preserved the original bundled name-tag font and documented its upstream sources.
- HTTP request logging and configurable file logging.
- Windows and Linux build scripts, Docker deployment, and offline and online acceptance tests.
