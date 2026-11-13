# HTTP API 文档

本文档描述 fleece-render 的 HTTP 接口。以下示例使用 `http://127.0.0.1:57680`；启动与配置见[部署指南](DEPLOYMENT.md)。PNG 响应的 `X-Fleece-Render-*` 字段见[图片响应头](#8-图片响应头)。

## 1. 基本约定

- 请求的查询参数和 JSON 字段使用 **camelCase**。响应字段以各接口定义为准。
- 图片接口返回 `image/png`。
- JSON 接口返回 UTF-8 JSON；图片内容使用标准 Base64 编码。
- `3d` 风格的 `definition` 范围是 `0.25..=5.0`。
- `3d` 默认单视角逻辑尺寸为 `350x670`，默认 `definition=1.5` 时输出为 `525x1005`。
- `front`、`back` 是单视角；`both` 是左右拼接的正反视图；`all` 在 JSON 接口中返回三张图，在 PNG 接口中返回正反拼图。
- `transparent=false` 使用白色背景；`transparent=true` 保留 PNG alpha 通道。

### Bearer token 认证

`[server]` 下的 `auth_token` 默认留空（也可省略），此时无需认证。设置非空值并重启服务后，除精确路径 `/health` 外，所有请求都必须提供 `Authorization: Bearer <token>`，包括服务信息、渲染接口、未知路径及 `OPTIONS` 请求。Token 区分大小写；不要通过查询参数传递。本文其余示例省略认证头，启用认证后请自行添加。例如：

```powershell
Invoke-WebRequest -UseBasicParsing 'http://127.0.0.1:57680/api/v1/styles' `
  -Headers @{ Authorization = 'Bearer <token>' }
```

将 `<token>`（包括尖括号）替换为配置中的值。认证头缺失、格式无效、重复或 Token 不匹配时，返回 `401 Unauthorized`、`WWW-Authenticate: Bearer realm="fleece-render"`，以及 `code` 为 `unauthorized` 的错误响应。受保护的响应带有 `Cache-Control: no-store`，不允许缓存。跨源浏览器请求的预检不会携带 Token；即使启用了 `server.allow_cors`，预检仍会被拒绝。浏览器客户端可通过同源入口访问。

## 2. 服务信息

`version` 为构建时的项目版本。以下响应示例用 `<version>` 表示实际值。

### `GET /`

返回服务标识：

```json
{
  "status": 200,
  "msg": "fleece-render",
  "package": "fleece-render",
  "version": "<version>",
  "api": "/api/v1"
}
```

### `GET /health`

不需要认证，也不访问 Mojang，仅返回进程和渲染器状态：

```json
{
  "status": 200,
  "ok": true,
  "package": {"name": "fleece-render", "version": "<version>", "implementation": "rust"},
  "styles": ["3d", "avatar"],
  "sources": ["mojang", "littleskin"],
  "backend": "cpu",
  "availableBackends": ["cpu"],
  "uptimeSeconds": 12.3,
  "render": {
    "definition": 1.5,
    "panelWidth": 350,
    "panelHeight": 670,
    "fov": 70.0,
    "supersample": 2,
    "concurrency": 4
  }
}
```

`backend` 是进程启动时选定的 3D 后端。`availableBackends` 列出二进制中编译的后端；含 `gpu` feature 时为 `["cpu", "gpu"]`，不表示已探测到 Vulkan 设备。单次请求实际使用的后端见 `X-Fleece-Render-Backend` 或 `backendInfo.backend`。

### `GET /api/v1`

返回服务名称、版本和端点索引。

### `GET /api/v1/styles`

返回 `styles` 数组。每个条目包含风格标识 `id` 和用于展示的描述 `label`。请求中使用 `id`：`3d` 为全身图，`avatar` 为 64×64 头像。

## 3. 通用渲染参数

以下参数可用于 URL、raw、Mojang、LittleSkin 路由；v1 使用同名 JSON 字段。

| 参数 | 类型 | 默认值 | 说明 |
| --- | --- | --- | --- |
| `style` | string | `3d` | `3d` 渲染全身；`avatar` 渲染 64×64 头像，仅支持 `front`。 |
| `definition` | number | `render.definition`（默认 `1.5`） | 输出缩放倍数，范围 `0.25..=5.0`。 |
| `transparent` | bool | 配置值 | `true` 保留透明背景，`false` 使用白底。 |
| `timeout` | number | `render.timeout_seconds`（默认 `30`） | CPU 渲染阶段的超时，单位秒；网络请求使用独立的超时设置。 |
| `showOuterLayer` | bool | `true` | `false` 只画基础皮肤层；`true` 允许绘制外层。3D 控制帽子、外套、袖子和裤腿外层，不影响披风、鞘翅或名称标签；头像只控制帽子层。 |
| `showName` | bool | `true` | 是否显示名称标签。 |
| `nameTag` | string | 自动 | 覆盖名称标签文本；没有该字段时使用玩家名称。 |
| `model` | string | `auto-detect` | `auto-detect`、`default/classic` 或 `slim`。 |
| `backEquipment` | string | `cape` | `cape` 或 `elytra`。 |

### 2D 头像（`style=avatar`）

返回一张 **64×64 PNG**，以最近邻缩放并居中绘制 **48×48 基础脸部**；启用外层且检测到有效帽子纹理时，叠加居中的 **56×56 帽子层**。支持 `64×64`、旧版 `64×32` 皮肤及其整数倍高清展开图。

头像支持 `transparent`（保留 alpha 或使用纯白背景，省略时使用配置值）和 `showOuterLayer`。`showOuterLayer=false` 不改变画布和基础脸部大小；`true` 仍保留帽子层检测，避免将旧皮肤的填充色画到脸上。`definition`、名称标签、模型覆盖和背部装备不会影响头像，披风和鞘翅不会被下载。头像只支持 `front`，使用 `back`、`both` 或 `all` 会返回 `400 invalid_request`。

| 输入 | 请求 |
| --- | --- |
| HTTP(S) URL 或服务端本地文件 | `GET /url/image/front?style=avatar&transparent=true&showOuterLayer=false&skinUrl=<编码后的URL或路径>` |
| Base64 或 data URL | `GET /url/image/front?style=avatar&skinData=<编码后的数据>` |
| 原始图片或 Base64/data URL 请求体 | `POST /raw/image/front?style=avatar&transparent=true` |
| Mojang / LittleSkin 玩家名称或 UUID | `GET /mojang/image/front?style=avatar&name=...` 或 `GET /littleskin/image/front?style=avatar&name=...` |
| v1 JSON | `POST /api/v1/render/front?format=png`，请求体如 `{"style":"avatar","transparent":true,"showOuterLayer":false,"skin":{"url":"https://example.com/skin.png"}}`，也可使用 `skin.data`、`skin.path` 或 `player`。 |

v1 使用 `format=json` 时，头像在 `front_b64` 中，`back_b64` 和 `both_b64` 为空字符串。PNG 响应包含 `X-Fleece-Render-Style: avatar` 和 `X-Fleece-Render-Size: 64x64`。同一展开图使用相同选项从不同来源输入时，生成的 PNG 逐字节一致。

## 4. 展开图 URL 输入

### `GET /url/image/{parts}`

`parts` 可为 `front`、`back`、`both` 或 `all`。该路由返回主图 PNG；`all` 的主图仍为 `both`。

必须提供 `skinUrl` 或 `skinData` 之一。优先级为 `skinData` > `skinUrl`。

| 参数 | 说明 |
| --- | --- |
| `skinUrl` | HTTP(S) URL、本地路径或 `file://` 路径。 |
| `skinData` | 原始 Base64 或 data URL。 |
| `capeUrl` / `capeData` | 披风纹理；按 `backEquipment` 选择。 |
| `elytraUrl` / `elytraData` | 鞘翅纹理。 |
| `backEquipment` | `elytra` 时优先使用鞘翅，否则回退到披风；`cape` 反之。 |

示例：

```text
GET /url/image/both?skinUrl=https%3A%2F%2Fexample.com%2Fskin.png&definition=1.5
```

本机运行服务时，也可以使用服务端能够访问的绝对路径。以下 PowerShell 示例均从仓库根目录运行：

```powershell
New-Item -ItemType Directory -Force output | Out-Null
$path = (Resolve-Path 'crates/fleece-render-core/tests/fixtures/source_skin.png').Path
$skin = [Uri]::EscapeDataString($path)
Invoke-WebRequest -UseBasicParsing `
  "http://127.0.0.1:57680/url/image/front?skinUrl=$skin&transparent=true" `
  -OutFile output/local-front-transparent.png
```

### `GET /url/json/all`

一次渲染并返回三张图片：

```json
{
  "front_b64": "...",
  "back_b64": "...",
  "both_b64": "...",
  "backendInfo": {
    "backend": "cpu",
    "style": "3d",
    "model": "classic",
    "parts": "all",
    "panel_width": 525,
    "panel_height": 1005,
    "outputWidth": 1050,
    "outputHeight": 1005,
    "elapsedMs": 161.2,
    "triangles": 168,
    "cached": false
  }
}
```

## 5. raw 图片输入

### `POST /raw/image/{parts}`

请求体可以是：

1. PNG、JPEG 或 WebP 二进制；
2. data URL 文本；
3. Base64 文本。

请求体提供皮肤纹理，不需要 `skinUrl` 或 `skinData`。通用渲染参数以及 `cape*`、`elytra*` 仍从查询参数读取。

以透明背景输出 3D 前视图：

```powershell
New-Item -ItemType Directory -Force output | Out-Null
Invoke-WebRequest -UseBasicParsing -Method Post `
  -ContentType image/png `
  -InFile crates/fleece-render-core/tests/fixtures/source_skin.png `
  'http://127.0.0.1:57680/raw/image/front?definition=1.0&transparent=true&showName=false' `
  -OutFile output/raw-front-transparent.png
```

从同一张皮肤生成透明头像：

```powershell
New-Item -ItemType Directory -Force output | Out-Null
Invoke-WebRequest -UseBasicParsing -Method Post `
  -ContentType image/png `
  -InFile crates/fleece-render-core/tests/fixtures/source_skin.png `
  'http://127.0.0.1:57680/raw/image/front?style=avatar&transparent=true' `
  -OutFile output/avatar.png
```

## 6. Mojang 和 LittleSkin

### `GET /mojang/image/{parts}?name=...`

`name` 可以是玩家名称或 UUID。服务会查询 Mojang profile，再下载皮肤和可用披风。

### `GET /littleskin/image/{parts}?name=...`

语义与 Mojang 路由相同，但 profile 来源为 LittleSkin。

额外参数：

| 参数 | 默认值 | 说明 |
| --- | --- | --- |
| `name` | 必填 | 玩家名称或 UUID。 |
| `useCape` | `true` | 是否下载并渲染 profile 中的披风。 |
| 其余参数 | - | 使用第 3 节中的通用渲染参数。 |

示例：将 `YourPlayerName` 或 `YourPlayerUuid` 替换为实际值。

```powershell
New-Item -ItemType Directory -Force output | Out-Null
Invoke-WebRequest -UseBasicParsing `
  'http://127.0.0.1:57680/mojang/image/front?name=YourPlayerName' `
  -OutFile output/mojang-front.png
Invoke-WebRequest -UseBasicParsing `
  'http://127.0.0.1:57680/mojang/image/both?name=YourPlayerUuid' `
  -OutFile output/mojang-both.png
```

## 7. `/api/v1` JSON API

### `POST /api/v1/render/{parts}?format=json|png`

请求体必须提供有效的 `skin` 或 `player`。同时提供时，优先使用 `skin` 的纹理来源。

纹理对象支持三种来源，优先级为 `data` > `url` > `path`：

```json
{
  "url": "https://example.com/skin.png",
  "data": "data:image/png;base64,...",
  "path": "C:/skins/player.png"
}
```

按玩家查询：`source` 可用 `mojang`（默认）或 `littleskin`，`uuid` 与 `name` 至少提供一个，两者都有时优先使用 `uuid`。

```json
{
  "player": {
    "name": "YourPlayerName",
    "source": "mojang",
    "useCape": true
  },
  "definition": 1.5,
  "transparent": true,
  "showName": true,
  "nameTag": "Player",
  "model": "auto-detect",
  "backEquipment": "cape"
}
```

按本地皮肤数据渲染：

```json
{
  "skin": {"data": "iVBORw0KGgoAAA..."},
  "cape": {"path": "C:/skins/cape.png"},
  "definition": 1.5,
  "transparent": true,
  "showName": false
}
```

`format=json` 是默认值，响应为：

```json
{
  "ok": true,
  "front_b64": "...",
  "back_b64": "...",
  "both_b64": "...",
  "backendInfo": {},
  "profile": null
}
```

`format=png` 或 `format=image` 返回当前 `parts` 对应的 PNG。以下示例将本地皮肤编码为 Base64 放入 JSON，然后保存 PNG 响应：

```powershell
New-Item -ItemType Directory -Force output | Out-Null
$path = (Resolve-Path 'crates/fleece-render-core/tests/fixtures/source_skin.png').Path
$body = @{
  skin = @{ data = [Convert]::ToBase64String([IO.File]::ReadAllBytes($path)) }
  transparent = $true
  showName = $false
} | ConvertTo-Json -Depth 3
Invoke-WebRequest -UseBasicParsing -Method Post -ContentType application/json -Body $body `
  'http://127.0.0.1:57680/api/v1/render/both?format=png' `
  -OutFile output/v1-both.png
```

## 8. 图片响应头

PNG 响应会包含：

| Header | 含义 |
| --- | --- |
| `Content-Type` | `image/png`。 |
| `X-Fleece-Render-Style` | 实际风格。 |
| `X-Fleece-Render-Parts` | 实际渲染部件。 |
| `X-Fleece-Render-Model` | `classic` 或 `slim`。 |
| `X-Fleece-Render-Size` | 输出尺寸，例如 `1050x1005`。 |
| `X-Fleece-Render-Elapsed` | 核心渲染耗时，单位秒。 |
| `X-Fleece-Render-Backend` | 实际使用的后端，`cpu` 或 `gpu`。头像和自动回退到 CPU 的结果标为 `cpu`。 |

## 9. 错误格式

服务处理过程中产生的结构化错误包含 `code`、`message` 和 `details`。HTTP 框架拒绝请求时（例如请求体超过上限）可能直接返回 HTTP 错误，不包含这些字段。

```json
{
  "code": "invalid_request",
  "message": "skinUrl or skinData is required",
  "details": {}
}
```

常见状态码：

| HTTP 状态码 | `code` | 场景 |
| ---: | --- | --- |
| 400 | `invalid_request` / `texture` / `unknown_style` | 参数、图片或风格无效。 |
| 401 | `unauthorized` | 已启用 Bearer 认证，请求未提供有效凭据。 |
| 404 | `player_not_found` | profile 不存在。 |
| 504 | `timeout` | 上游下载或渲染超时。 |
| 502 | `upstream` | Mojang、LittleSkin 或纹理上游错误。 |
| 503 | `backend` | 请求的渲染后端不可用。 |
| 500 | `internal_error` | 未预期的服务内部错误。 |

## 10. 限制和安全边界

- 全局请求体限制 `server.body_limit_bytes` 默认 16 MiB；raw 和 JSON 接口还受 HTTP 框架的 2 MiB 请求体限制，以较小值为准。单张纹理的默认字节上限为 8 MiB。
- profile 请求默认超时 15 秒，远程纹理请求默认超时 20 秒。
- 渲染任务使用 `render_concurrency` 控制并发，默认 4。GPU 光栅化和读回在同一引擎内串行执行。
- 输出超过 48 megapixels 会被拒绝。
- 纹理输入允许服务端本地路径和远程 HTTP(S) URL。只应向可信客户端开放，详见[部署安全边界](DEPLOYMENT.md#安全边界)。
- 玩家档案与远程纹理请求不使用 `HTTP(S)_PROXY`。

## 11. HTTP 日志

日志字段、存储位置及配置见[部署指南](DEPLOYMENT.md#日志)。
