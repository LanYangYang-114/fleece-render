# fleece-render-server

The `fleece-render` HTTP server renders Minecraft skins from Mojang and
LittleSkin profiles, or from textures supplied by a trusted client. The
optional `gpu` feature enables Vulkan acceleration for 3D rendering.

The server listens on `127.0.0.1:57680` by default. Do not expose it directly
on the public internet: it accepts server-local paths and remote URLs as
texture sources, and has no built-in authentication. See the
[deployment guide](https://github.com/LanYangYang-114/fleece-render/blob/main/docs/DEPLOYMENT.md)
for configuration and the [API guide](https://github.com/LanYangYang-114/fleece-render/blob/main/docs/API.md)
for request examples.
