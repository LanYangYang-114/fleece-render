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

以下三种方式任选其一，默认都只在宿主机的 `127.0.0.1:57680` 上开放服务。其他设备默认无法通过宿主机的局域网 IP 访问；如需局域网或公网访问，请按[对外访问](#对外访问)配置，不要只修改容器内的监听地址。

### 直接拉取 GHCR 镜像

无需下载 Compose 文件，直接运行以下命令拉取镜像并启动服务：

```sh
docker run --pull=always -d --name fleece-render -p 127.0.0.1:57680:57680 ghcr.io/lanyangyang-114/fleece-render:latest
```

镜像支持 Linux x86-64 和 ARM64，Docker 会自动选择对应平台。正式版同时提供 `latest` 标签；预发布版只有版本标签。需要固定版本时，将命令中的 `latest` 替换为所需版本的 Git 标签。

### 通过 Compose 拉取 GHCR 镜像

把 [`docker-compose-ghcr.yml`](../docker-compose-ghcr.yml) 和 [`fleece-render.toml`](../fleece-render.toml) 放在同一目录。Docker 会下载现成的镜像，不用编译。以下命令都在这两个文件所在的目录运行。

#### 使用镜像默认用户

容器内默认用 `fleece-render` 用户运行，不是 `root`。这个用户需要以下权限：

- **配置文件 `fleece-render.toml`：** 文件必须已存在，服务只需要能读取它，不需要写入权限。
- **日志目录 `./logs/`：** 服务需要能读写。Docker 会把这个目录挂载到容器内的 `/logs`，如果 `./logs/` 不存在，Docker 会自动创建，但创建后不一定允许 `fleece-render` 写入。

如果日志目录不能写，在常规 Linux Docker 上可用下面的命令查出容器用户的 UID，并把目录交给这个用户：

```sh
uid=$(docker compose -f docker-compose-ghcr.yml run --rm --no-deps -T --entrypoint id fleece-render -u) && sudo chown "$uid" ./logs
```

#### 改用宿主机账号的 UID/GID（可选）

如果操作系统的默认权限策略只允许文件所有者读写文件（例如在飞牛 OS 上遇到这种情况），且配置文件属于你的宿主机账号，可以让容器直接使用该账号的 UID 和 GID，无需调整权限策略。

1. 编辑 `docker-compose-ghcr.yml`，在 `fleece-render` 下添加 `user`，和 `image` 对齐：

   ```yaml
       user: "${FLEECE_RENDER_USER:-fleece-render}"
   ```

2. 用自己的普通账号执行以下命令，不要加 `sudo`。把输出的整行复制到同目录的 `.env` 文件里。不设置时，仍使用镜像默认的 `fleece-render` 用户。

   ```sh
   printf 'FLEECE_RENDER_USER=%s:%s\n' "$(id -u)" "$(id -g)"
   ```

3. 如果 `./logs/` 还不存在，可以用当前账号先执行 `mkdir -p ./logs`，这样目录一开始就归你所有。如果 Docker 已经创建了目录，而当前账号不能写入，可执行 `sudo chown "$(id -u):$(id -g)" ./logs` 调整。

以上权限命令以常规 Linux Docker 为例；如果使用 rootless Docker 或修改过用户映射，请按实际权限设置。

#### 启动或更新

```sh
docker compose -f docker-compose-ghcr.yml up -d
```

启动后，在运行 Docker 的机器上打开 `http://127.0.0.1:57680/health` 检查服务是否正常。

### 通过 Compose 从源码构建

在仓库根目录使用 [`docker-compose.yml`](../docker-compose.yml) 从源码构建并启动。构建后的镜像标记为 `fleece-render:local`，服务实际版本由构建时的 Cargo 版本决定：

```sh
docker compose -f docker-compose.yml up --build -d
docker compose -f docker-compose.yml logs -f fleece-render
```

未覆盖镜像设置时，容器从 `/etc/fleece-render.toml` 读取内置配置，日志只写入标准错误。若要启用文件日志，可将 `FLEECE_RENDER_LOG_FILE` 设为运行用户可写的路径，例如 `/var/lib/fleece-render/logs/fleece-render.log`，每次启动会生成带日期和序号的新文件（见[日志](#日志)）；如需在容器重建后保留日志，请挂载持久化存储。

两份 Compose 配置都通过 `container_name: fleece-render` 固定容器名称。修改已有部署的配置后，重新运行对应的 `docker compose ... up -d` 命令即可重建容器并应用名称。同一 Docker 主机上的容器名称不能重复；如果已有同名容器，或需要同时运行多个实例，请修改或移除 `container_name`。

## 对外访问

默认配置只允许本机访问。这不是 API 的限制，而是为了避免在认证默认关闭、纹理输入可访问本地文件和远程 URL 的情况下，意外向不可信客户端开放服务。请区分“可信局域网直接访问”和“公网访问”两种场景；开放端口不等于已完成安全配置。

### 可信局域网直接访问

#### 直接运行二进制或从源码运行

在实际加载的配置文件中修改已有的 `[server]` 表：

```toml
[server]
host = "0.0.0.0"
port = 57680
```

也可以使用 `--host 0.0.0.0 --port 57680` 或环境变量 `FLEECE_RENDER_HOST=0.0.0.0`。命令行和环境变量会覆盖配置文件，检查启动参数中是否仍有 `--host 127.0.0.1`。修改后重启服务，并在宿主机防火墙中仅允许所需的局域网来源访问 TCP `57680`。

`0.0.0.0` 表示监听所有 IPv4 接口，不是客户端应访问的地址。其他设备应使用宿主机的实际 IP，例如 `http://192.168.3.2:57680/`。

#### Docker / Compose

**容器内监听地址和宿主机端口映射是两层独立设置。** 官方镜像通过 `FLEECE_RENDER_HOST=0.0.0.0` 在容器内监听，通常不需要修改；只修改 `fleece-render.toml` 中的 `host`，不能改变 Docker 在宿主机上的端口绑定。

将实际使用的 `docker-compose.yml` 或 `docker-compose-ghcr.yml` 中原有的 `ports` 项替换为：

```yaml
services:
  fleece-render:
    ports:
      - "57680:57680"
```

这是需要替换的配置片段，保留该服务原有的 `image` / `build`、挂载、环境变量等设置。三种映射方式的区别如下：

| 端口映射 | 访问范围 |
| --- | --- |
| `"127.0.0.1:57680:57680"` | 默认配置，仅宿主机本机访问，适合由宿主机上的反向代理提供入口。 |
| `"57680:57680"` | 默认发布到宿主机所有网络接口，不只限于局域网；需结合防火墙限制访问。 |
| `"192.168.3.2:57680:57680"` | 仅绑定宿主机的这个 IP；请替换为自己机器实际拥有的局域网 IP。 |

直接使用 `docker run` 时，对应地把 `-p 127.0.0.1:57680:57680` 改为 `-p 57680:57680`，或指定宿主机 IP。已创建容器的端口映射需要通过重新创建容器来更新。

Compose 修改后，使用原来的文件重新部署，**不要只执行 `docker compose restart` 或在 NAS 界面中点击“重启”**：

```sh
# GHCR 镜像部署
docker compose -f docker-compose-ghcr.yml up -d

# 从源码构建的部署（二选一，使用自己原先的方式）
docker compose -f docker-compose.yml up --build -d
```

通过 `docker port fleece-render` 检查实际映射。若仍显示 `127.0.0.1:57680`，说明运行中的容器仍只允许宿主机访问。Docker 的端口发布还会配置宿主机网络规则，不能仅凭某个防火墙管理界面中的设置判断访问范围；应从另一台设备实际验证。参见 [Docker 端口发布说明](https://docs.docker.com/engine/network/port-publishing/)。

### 公网访问

**不建议把 `57680` 直接转发到公网。** 推荐保留渲染服务的本机入口，由受控的反向代理或业务网关提供 HTTPS；当前服务本身不终止 TLS。仅设置 `host=0.0.0.0`、开放端口或开启 CORS，都不能替代下面的配置。

1. **配置受控入口与 HTTPS。** 配置域名、DNS 和有效证书，让公网客户端访问 HTTPS 入口。按实际网络情况配置路由器或云防火墙，仅开放入口需要的端口，通常为 TCP `443`；证书签发方式可能另需 `80`。同时检查 IPv6 防火墙，不要让客户端绕过入口直接访问后端 `57680`。
2. **让后端只对入口可达。** 代理运行在宿主机时，可继续保留 `127.0.0.1:57680:57680`，上游使用 `http://127.0.0.1:57680`。代理运行在另一个容器时，其 `127.0.0.1` 指向代理容器自身；应把两个容器加入同一用户自定义网络，以上游 `http://fleece-render:57680` 访问，并可移除渲染容器的宿主机 `ports`。不要为了让代理连上而无条件公开后端端口。
3. **开启认证并保护 Token。** 按[启用 Bearer 认证](#启用-bearer-认证)生成随机 Token，在实际加载的配置中设置非空的 `server.auth_token`，重启服务，并让代理转发客户端的 `Authorization` 头。仅向可信客户端发放 Token，通过 HTTPS 传输；不要放在查询参数中，也不要硬编码到公开前端。`/health` 仍不要求 Token，需要限制时由入口控制。共享 Bearer Token 并不是面向匿名用户的账户系统。
4. **限制纹理来源和进程权限。** 当前没有一个开关可以同时禁用本地文件输入、限制 URL 域名或禁止请求内网。Bearer 认证也不会改变这些能力。不要挂载不必要的宿主机目录或 Docker Socket，不要改为 root 运行，并限制进程的网络出口。面向不可信用户时，业务网关必须只接受允许的输入，例如只提供受控的玩家查询，不向用户透传任意纹理路径 / URL；如允许 URL，应校验皮肤和背部纹理来源、重定向及解析后的目标地址，阻止回环、内网和链路本地等目标访问，配合网络出口隔离。仅按路由名称放行不足以限制这些输入。
5. **配置访问额度和资源限制。** 在入口设置请求速率、并发和请求体限制，并为容器设置合适的 CPU / 内存限制。调整代理超时以覆盖实际渲染和上游请求耗时；服务自身的渲染并发、超时和纹理大小限制不能替代入口访问控制。
6. **按客户端类型处理跨域。** 服务器、机器人和普通 `curl` 调用不需要 CORS。浏览器优先使用同源业务入口；当前 `server.allow_cors=true` 是宽松 CORS，不是认证或来源白名单，而且开启 Bearer 认证后，跨域预检 `OPTIONS` 仍会因不携带 Token 而被拒绝，不能只开这个选项解决。必须跨域时，由受控入口对指定来源处理预检和响应头，并保留实际请求的认证，不要放宽所有业务请求。详见 [API 认证约定](API.md#bearer-token-认证)。

Docker 挂载配置时还要注意：GHCR Compose 已只读挂载 `./fleece-render.toml`，应修改这份实际挂载的配置并保护文件权限，不要提交真实 Token。源码 Compose 默认使用镜像内的配置，直接 `docker run` 也不会自动加载宿主机同名文件。

源码构建时，Dockerfile 会把仓库根目录的 `fleece-render.toml` 复制进镜像，**不要把真实 Token 写进这份构建输入**。应另备一份位于构建上下文之外的私有配置，在已有服务中增加只读挂载，保留其他设置：

```yaml
services:
  fleece-render:
    volumes:
      - type: bind
        source: /absolute/path/fleece-render.private.toml
        target: /etc/fleece-render.toml
        read_only: true
        bind:
          create_host_path: false
```

将 `source` 替换为宿主机上已存在的私有配置文件绝对路径。直接 `docker run` 时，在镜像名称之前添加 `--mount type=bind,src=/absolute/path/fleece-render.private.toml,dst=/etc/fleece-render.toml,readonly`。确保容器运行用户可读，重新创建容器后生效；权限要求见[使用镜像默认用户](#使用镜像默认用户)。

### 验证与排查

按以下顺序测试，能区分进程启动、端口发布和公网入口问题：

```sh
# 在服务器本机执行；/health 不需要认证
curl -i http://127.0.0.1:57680/health

# 在另一台局域网设备执行，替换为服务器实际 IP
curl -i http://192.168.3.2:57680/health

# 公网入口配置好后，从外部网络验证认证，不携带 Token 应为 401
curl -i https://render.example.com/

# 将实际 Token 放入当前终端的 FLEECE_RENDER_TOKEN 变量后测试；应为 200
curl -i https://render.example.com/ -H "Authorization: Bearer $FLEECE_RENDER_TOKEN"
```

`render.example.com` 是示例域名，请替换为自己的入口域名。`FLEECE_RENDER_TOKEN` 在此仅是客户端 `curl` 使用的变量，不是服务支持的认证配置环境变量。若只绑定指定的宿主机 IP，或使用不发布后端端口的容器代理方案，本机回环测试应改为对应地址或从代理所在网络测试。

- **本机成功，局域网连接被拒绝 / 超时：** 检查程序监听地址、运行中容器的端口映射、NAS / 宿主机防火墙及局域网路由。`Connection refused` 表示尚未建立 HTTP 连接，不是 `/` 路由返回空内容，也不是 CORS 错误。
- **收到 `401`：** 服务或入口已可达，继续检查 `Authorization` 头和配置的 Token；修改 Token 后要重启服务。
- **收到 `200`，只有浏览器跨域调用失败：** 检查入口的同源 / CORS 配置和预检响应，而不是继续开放端口。
- **不要用示例访问 URL 判断实际监听地址：** 当前启动日志中的 `root: http://127.0.0.1:...` 等行是固定的本机访问提示；实际监听地址看 `fleece-render API started` 日志中的 `address` 字段。Docker 的宿主机访问范围仍以端口映射为准。

## 配置

完整字段与默认值见 [`fleece-render.toml`](../fleece-render.toml)。未指定配置路径时，程序查找当前工作目录下的 `fleece-render.toml`；文件不存在时使用内置默认值。

| 用途 | 配置项 | 环境变量 | 命令行 |
| --- | --- | --- | --- |
| 配置文件路径 | — | `FLEECE_RENDER_CONFIG` | `--config` |
| 监听地址 | `server.host` | `FLEECE_RENDER_HOST` | `--host` |
| 端口 | `server.port` | `FLEECE_RENDER_PORT` | `--port` |
| 渲染后端 | `render.backend` | `FLEECE_RENDER_BACKEND` | `--backend` |
| Bearer 认证 | `server.auth_token`（默认空字符串） | — | — |
| 跨源访问 | `server.allow_cors`（默认 `false`） | — | — |
| 文件日志基础路径 | `server.log_file` | `FLEECE_RENDER_LOG_FILE` | `--log-file` |
| 最大日志文件数 | `server.log_max_files`（默认 `30`，至少 `1`） | — | — |
| 日志过滤器 | `server.log_filter` | `RUST_LOG`、`FLEECE_RENDER_LOG` | — |

同一选项通常按命令行、环境变量、配置文件的顺序取值。日志过滤器优先使用 `RUST_LOG`。`render.backend` 默认是 `auto`，选择规则见[可选 GPU 后端](#可选-gpu-后端)。`render.transparent = true` 将默认背景设为透明，可由单次请求覆盖。

### 启用 Bearer 认证

在实际使用的配置文件中设置：

```toml
[server]
auth_token = ""
```

修改已有的 `[server]` 配置，不要重复添加表头。将空字符串换成随机生成的高强度 Token（例如 32 个随机字节的十六进制编码），然后重启服务。Token 可以包含 ASCII 字母、数字和 `-._~+/`，末尾可带 `=`；不能包含空格。空字符串或省略该字段则关闭认证。除 `/health` 外，请求需带 `Authorization: Bearer <token>`，详见 [HTTP API 文档](API.md#bearer-token-认证)。使用 Docker 时，可把运行用户可读的私有配置文件只读挂载到 `/etc/fleece-render.toml`，不要将真实 Token 写入镜像或提交到 Git。

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

- 默认监听 `127.0.0.1:57680`。容器内监听 `0.0.0.0`，Compose 默认仅将端口发布到宿主机回环地址。局域网和公网部署需要额外配置，步骤见[对外访问](#对外访问)。
- 默认不允许浏览器跨源访问。仅在可信环境下显式启用 `server.allow_cors = true`；对外部署应由受控入口配置具体允许的来源。
- Bearer 认证默认关闭。启用后仅向可信客户端发放 Token，保护配置文件，并通过 HTTPS 传输凭据。认证不会限制纹理来源：纹理输入仍可读取进程有权访问的本地文件或请求远程 URL，不要将这些接口直接开放给不可信客户端。
- 如需对外服务，应先隔离渲染进程及其文件访问权限，限制网络出口，并在受控入口只接受允许的纹理来源。请求体和纹理大小限制、网络超时不能代替访问控制。纹理字节限制应用于解码后的 Base64/data URL、原始图片、本地文件及下载内容；图片解码另有 4096 像素边长和内存限制。
