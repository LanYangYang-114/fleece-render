# 部署指南

## 下载预编译版本

从 GitHub 仓库的 **Releases** 选择所需版本，再按系统和架构下载对应的附件。

| 平台 | 下载文件 | 解压后启动 |
| --- | --- | --- |
| Linux x86-64 (musl) | `fleece-render-linux-x86_64.tar.gz` | `./fleece-render` |
| Linux ARM64 (musl) | `fleece-render-linux-aarch64.tar.gz` | `./fleece-render` |
| Windows x86-64 (MSVC) | `fleece-render-windows-x86_64.zip` | `.\fleece-render.exe` |

三个包都是 CPU Release 版，附带 `fleece-render.toml` 和许可证文件。请在解压目录中启动，以加载同目录的配置文件。这些二进制包与 crates.io 发布相互独立。

## 通过 Cargo 安装

从 crates.io 安装 HTTP 服务：

```sh
cargo install --locked fleece-render-server
fleece-render --host 127.0.0.1
```

`fleece-render-core` 是库；如果只需要在 Rust 项目中使用渲染功能，用 `cargo add fleece-render-core` 添加依赖。GitHub 预发布标签不会同步到 crates.io。

## 从源码运行

使用满足根目录 [`Cargo.toml`](../Cargo.toml) 中 `[workspace.package].rust-version` 要求的 Rust 工具链。在仓库根目录构建：

```sh
cargo build --release --locked --workspace
```

Linux 二进制为 `target/release/fleece-render`，Windows 为 `target\release\fleece-render.exe`。启动时可以显式指定监听地址和端口：

```sh
cargo run --release --locked -p fleece-render-server --bin fleece-render -- --host 127.0.0.1 --port 57680
```

也可使用 `scripts/` 中的脚本。Windows 脚本会在仓库内已有隔离工具链时使用它，否则使用系统工具链；它们不会下载或安装 Rust。

| 平台 | 构建 | 启动 |
| --- | --- | --- |
| Windows PowerShell | `.\scripts\build.ps1` | `$env:FLEECE_RENDER_HOST = '127.0.0.1'; .\scripts\run.ps1` |
| Linux | `./scripts/build.sh` | `FLEECE_RENDER_HOST=127.0.0.1 ./scripts/run.sh` |

Windows 启动脚本的 `-Port` 参数默认为 `57680`。脚本在前台运行，使用 Ctrl+C 停止。访问 `http://127.0.0.1:57680/health` 检查服务状态；请求示例见 [HTTP API 文档](API.md)。

## 可选 GPU 后端

在 Windows 或 Linux 上，需要进程可见的 Vulkan 硬件设备及对应驱动。不需要桌面会话、窗口或显示器。以下命令构建含 GPU 后端的 Release 版；不传 `--features gpu` 则只构建 CPU 后端。

```sh
cargo build --release --locked -p fleece-render-server --features gpu --bin fleece-render
```

`--backend`（或 `FLEECE_RENDER_BACKEND` / `render.backend`）可选：

| 值 | 行为 |
| --- | --- |
| `auto`（默认） | 含 GPU 功能的构建优先使用硬件 GPU；未编译 GPU 功能、无可用设备或初始化失败时使用 CPU。GPU 渲染因后端不可用失败时，整个请求在 CPU 上重试，受同一 HTTP 超时限制。 |
| `cpu` | 强制使用 CPU，不探测 Vulkan 设备。 |
| `gpu` | 强制使用 GPU；启动时无法初始化则报错，请求期间后端不可用时返回 `503`，不静默回退。 |

用 `/health` 的 `backend` 查看启动时选定的 3D 后端，用图片的 `X-Fleece-Render-Backend` 或 JSON 的 `backendInfo.backend` 确认请求实际使用的后端。`avatar` 一直在 CPU 上合成。仓库自带的 Docker 构建保持 CPU-only。

## Docker

从 GitHub Container Registry 拉取 `latest` 镜像并启动服务：

```sh
docker run --pull=always -d --name fleece-render -p 127.0.0.1:57680:57680 ghcr.io/lanyangyang-114/fleece-render:latest
```

镜像支持 Linux x86-64 和 ARM64，Docker 会自动选择对应平台。正式版同时提供 `latest` 标签；预发布版只有版本标签。需要固定版本时，将命令中的 `latest` 替换为所需版本的 Git 标签。

也可以使用仓库中的 Compose 配置从源码构建和启动服务。镜像使用 `fleece-render:local` 作为本地标签，服务实际版本仍由构建时的 Cargo 版本决定。端口默认只发布到宿主机回环地址：

```sh
docker compose up --build -d
docker compose logs -f fleece-render
```

容器默认从 `/etc/fleece-render.toml` 读取配置，日志只写入标准错误。若要启用文件日志，可将 `FLEECE_RENDER_LOG_FILE` 设为运行用户可写的路径，例如 `/var/lib/fleece-render/logs/fleece-render.log`，每次启动会生成带日期和序号的新文件（见[日志](#日志)）；如需在容器重建后保留日志，请挂载持久化存储。

## 配置

完整字段与默认值见 [`fleece-render.toml`](../fleece-render.toml)。未指定配置路径时，程序查找当前工作目录下的 `fleece-render.toml`；文件不存在时使用内置默认值。

| 用途 | 配置项 | 环境变量 | 命令行 |
| --- | --- | --- | --- |
| 配置文件路径 | — | `FLEECE_RENDER_CONFIG` | `--config` |
| 监听地址 | `server.host` | `FLEECE_RENDER_HOST` | `--host` |
| 端口 | `server.port` | `FLEECE_RENDER_PORT` | `--port` |
| 渲染后端 | `render.backend` | `FLEECE_RENDER_BACKEND` | `--backend` |
| 跨源访问 | `server.allow_cors`（默认 `false`） | — | — |
| 文件日志基础路径 | `server.log_file` | `FLEECE_RENDER_LOG_FILE` | `--log-file` |
| 最大日志文件数 | `server.log_max_files`（默认 `30`，至少 `1`） | — | — |
| 日志过滤器 | `server.log_filter` | `RUST_LOG`、`FLEECE_RENDER_LOG` | — |

同一选项通常按命令行、环境变量、配置文件的顺序取值。日志过滤器优先使用 `RUST_LOG`。`render.backend` 默认是 `auto`，选择规则见[可选 GPU 后端](#可选-gpu-后端)。`render.transparent = true` 将默认背景设为透明，可由单次请求覆盖。

## 日志

服务默认同时写入标准错误和**可执行文件同级的 `logs/` 目录**，不再使用 `output/logs/`。每次启动创建独立文件，例如：

```text
fleece-render.exe
logs/
  fleece-render-2026-11-13-1.log
  fleece-render-2026-11-13-2.log
```

- 默认 `server.log_file = "logs/fleece-render.log"` 是日志的**基础路径**。程序会在扩展名前添加 `-YYYY-MM-DD-N`；日期取启动时的系统本地日期，`N` 为该目录下相同基础文件名当天已有的最大序号加一，从 `1` 开始，不补回删除文件留下的空号。
- 每次启动使用新文件，不覆盖或追加到旧日志；同时启动多个实例也会使用不同文件。一轮运行跨过午夜时继续写入启动时创建的文件，不按日切换。
- `server.log_max_files = 30` 控制最多保留的日志文件数，必须至少为 `1`。**每次启动创建新文件后**检查数量，超过上限则按文件名中的日期、数字序号从旧到新删除，直至满足上限；数量包含本次启动的新文件，本次文件始终保留。序号按数值比较，例如 `2` 比 `10` 旧，不受文件修改时间影响。
- 清理只针对当前日志目录中、相同基础文件名和扩展名、符合 `基础名-YYYY-MM-DD-N.扩展名` 格式的普通文件，不递归、不删除子目录、符号链接、其他文件或不同基础名的日志。关闭文件日志时也不执行清理。多个长期运行的实例建议配置不同的日志基础路径，避免相互清理旧日志。
- **相对路径以可执行文件目录为基准**，与启动工作目录或配置文件位置无关；绝对路径按原样使用。例如 `--log-file /var/log/fleece-render/service.log` 生成 `/var/log/fleece-render/service-2026-11-13-1.log`。
- 配置文件中的 `log_file = ""`，或将 `--log-file` / `FLEECE_RENDER_LOG_FILE` 设为空字符串，都可关闭文件日志；标准错误输出仍保留。优先级仍为命令行 > 环境变量 > 配置文件 > 内置默认值。
- 日志目录不存在时自动创建；无写入权限或无法删除超限旧日志时启动会明确报错。安装在只读目录时，请指定可写的绝对路径，或关闭文件日志。Docker 默认仍只写标准错误。

用 `RUST_LOG=info` 查看请求日志；日志记录方法、路径、参数名、状态和耗时，不记录查询参数值。启动日志会显示本次实际使用的日志路径。

## 安全边界

- 默认监听 `127.0.0.1:57680`。容器内监听 `0.0.0.0`，Compose 默认仅将端口发布到宿主机回环地址。
- 默认不允许浏览器跨源访问。仅在可信环境下显式启用 `server.allow_cors = true`；对外部署应由受控入口配置具体允许的来源。
- 服务没有内置身份验证。纹理输入可让服务读取进程有权访问的本地文件，也可让服务向远程 URL 发起请求；不要把这些接口直接开放给不可信客户端。
- 如需对外服务，应先隔离渲染进程及其文件访问权限，限制网络出口，并在受控入口只接受允许的纹理来源。请求体和纹理大小限制、网络超时不能代替访问控制。纹理字节限制应用于解码后的 Base64/data URL、原始图片、本地文件及下载内容；图片解码另有 4096 像素边长和内存限制。
