# 开发与验证

本文面向项目维护者和贡献者。项目概览和快速开始见[中文 README](../README.md)或 [English README](../README_en.md)。开始修改前，请阅读[仓库维护规则](../AGENTS.md)、[架构设计](ARCHITECTURE.md)和 [HTTP API 文档](API.md)。部署和日志配置见[部署指南](DEPLOYMENT.md)。

## 构建与检查

GitHub Actions 的 `ci` 在分支推送或 Pull Request 更新时只检查格式，不编译项目或执行测试。完整验收可在本地运行：

```sh
cargo fmt --all -- --check
cargo test --locked --workspace
cargo check --locked --workspace
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
```

Windows 上如果使用仓库内已有的隔离工具链，可先在当前 PowerShell 会话中执行 `. .\scripts\dev-env.ps1`。测试和 Release 构建也可使用 `.\scripts\test.ps1` 和 `.\scripts\build.ps1`；Linux 对应脚本为 `./scripts/test.sh` 和 `./scripts/build.sh`。

GPU 构建还需验证可选依赖与硬件路径：

```sh
cargo check --locked --workspace --features gpu
cargo test --locked --workspace --features gpu
```

没有 Vulkan 硬件时会跳过 GPU 实机用例；设置 `FLEECE_RENDER_REQUIRE_GPU=1` 可要求测试实际执行。初始化错误（除未找到硬件外）仍会使 GPU 测试失败。

## 版本维护

项目版本在根目录 [`Cargo.toml`](../Cargo.toml) 的 `[workspace.package].version` 中定义。两个 crate 继承该值；同时应在服务 crate 的 `fleece-render-core` 依赖声明中同步版本，以便脱离 workspace 打包。CLI 的 `--version`、HTTP 服务信息和默认 `User-Agent` 都使用构建时的包版本。手动配置 `[network].user_agent` 时会覆盖默认值。

调整版本后，先运行 `cargo check --workspace` 同步 `Cargo.lock`，再运行上面的 `--locked` 检查。`CHANGELOG.md` 中的版本号是历史记录，Compose 的 `local` 标签不代表发布版本。

先执行 `cargo package --locked --workspace` 生成并验证本地包，不会上传。在提交前检查工作树时可加 `--allow-dirty`。服务包在 workspace 内用相对路径构建；归档里的清单使用 Core 的注册表版本，不依赖仓库目录结构。

### 版本标签与发布渠道

`release` 在推送版本标签时复用 `build-binaries`，生成三个平台的二进制包并上传到 GitHub Releases。正式标签的版本号必须与两个 Cargo 包一致；预发布标签只发布 GitHub 预发行版和容器镜像，不发布到 crates.io。

GitHub Release 创建成功后，crates.io 和镜像各自发布。镜像复用本次构建的 Linux CPU 二进制，支持 `linux/amd64` 和 `linux/arm64`，使用 Git 标签作为镜像标签。正式版同时更新 `latest`，预发布版只发布版本标签。某个渠道失败不会撤回其他渠道已发布的内容。

### 发布配置（维护者）

- **crates.io**：在仓库 **Settings → Environments** 中创建 `crates-io` 环境，将允许发布两个包的 API token 保存为环境密钥 `CARGO_REGISTRY_TOKEN`。首次创建包时，token 需要允许发布新包。建议设置较短有效期，并限制该环境只接受版本标签。
- **GHCR**：使用仓库的 `GITHUB_TOKEN` 推送镜像，无需 Docker Hub 账号或额外密钥。若要允许匿名拉取，在 GitHub **Packages** 中将 `fleece-render` 镜像包设为 Public。

发布前可以用 `cargo publish --dry-run --locked --registry crates-io -p fleece-render-core -p fleece-render-server` 模拟发布，不会上传。正式发布时 Cargo 会按依赖顺序先上传 Core，等待索引可解析后再上传 Server。如果 Core 已发布而 Server 失败，确认 Core 的对应版本已进入索引后，在相同标签下用 `cargo publish --locked --registry crates-io -p fleece-render-server` 补发，不要重新发布 Core。

## 测试覆盖

- `fleece-render-core`：前视、后视、拼图尺寸与非空内容，classic/slim 模型，64×32 皮肤，3D 皮肤外层开关，名称标签、披风、鞘翅、透明通道及重复渲染的字节一致性。
- 头像：检查 64×64 输出、单层和双层排布、高清纹理的 UV、透明合成及帽子层判断。
- `fleece-render-server`：检查 HTTP 状态和响应头，以及两种风格在单层和双层模式下，相同纹理经本地路径、URL、Base64、data URL、raw 和 v1 JSON 输入时的 PNG 一致性；通过缓存档案测试玩家名称和 UUID 两种输入。

查看透明 PNG 时应检查 alpha 通道，而不是依据查看器的底色判断。需要生成本地测试图片时，可使用 [API 文档中的原始图片示例](API.md#5-raw-图片输入)。

## HTTP 验收

Windows 提供端到端验收脚本，负责构建 Release 二进制、启动临时服务并保存测试输出：

```powershell
# 使用仓库内的测试纹理，不要求 Mojang 在线
powershell -ExecutionPolicy Bypass -File .\scripts\acceptance.ps1

# 要求 Vulkan GPU；对 3D 和头像检查实际后端及输入一致性
powershell -ExecutionPolicy Bypass -File .\scripts\acceptance.ps1 -Gpu

# 额外验证在线 Mojang 玩家名称和 UUID 的结果一致
powershell -ExecutionPolicy Bypass -File .\scripts\acceptance.ps1 -Online
```

验收检查默认尺寸下请求的五秒目标，并比较不同输入形式生成图片的 SHA-256。Linux 可以运行 workspace 测试，再启动服务，按 [API 文档](API.md)手动验证。

## 依赖检查

发布前以 [`Cargo.lock`](../Cargo.lock) 为准，核对直接和间接依赖的安全公告及维护状态。名称标签只使用编译时内嵌的字体，服务不接受上传的字体。

## 生成物

`output/` 不纳入 Git。服务默认将日志保存到可执行文件同级的 `logs/`，每次启动生成 `fleece-render-YYYY-MM-DD-N.log`；不会将渲染图片保存到本地。日期取启动时的系统本地日期，序号在该目录、同一基础文件名和日期下递增，并通过排他创建避免并发覆盖。配置、环境变量及命令行仍可覆盖日志基础路径或关闭文件日志；相对日志路径以可执行文件目录为基准。`server.log_max_files` 默认 `30`（至少 `1`），每次启动创建新文件后按日期、数字序号删除超限的旧日志，包含本次新文件且始终保留它；只处理当前基础名匹配的普通文件，不递归清理其他内容。详情见[部署指南](DEPLOYMENT.md#日志)。

HTTP 验收通过绝对日志基础路径覆盖默认位置，将编号日志和图片保存在 `output/acceptance/<时间戳>/`。手动生成的图片和性能采样也放在 `output/` 下。`*.log` 均不纳入 Git。

日志测试覆盖启动编号、跨日期重置、缺号不复用、历史文件保留、并发排他创建、自定义路径、空值禁用、错误路径、保留数量默认值和校验、按日期和数字序号清理、单文件限制及无关文件保护。启动集成测试还会从其他工作目录启动独立二进制，验证默认日志确实位于可执行文件旁的 `logs/`，并验证配置文件 / 环境变量 / 命令行的路径覆盖与禁用、默认保留 30 份、自定义保留数量，以及禁用文件日志时不清理旧文件。

## 维护约定

- 修改 README 的共用内容（如功能、快速开始、部署安全说明或许可）时，在同一次变更中更新 [`README.md`](../README.md) 和 [`README_en.md`](../README_en.md)，确保示例命令、参数和文档链接一致。
- 改动 HTTP 合约时更新 [API 文档](API.md)和对应测试；保留已公开路由及 camelCase 请求字段。
- 改动 crate 边界或请求处理流程时更新[架构设计](ARCHITECTURE.md)和本文。
- 改动模型、UV、相机或采样时更新[渲染实现](RENDERING.md)，并验证输出尺寸、确定性和 CPU 性能目标。
- 新的网络输入必须配置超时及字节限制。GPU 后端必须保持可选；修改光栅化时同时检查 CPU 构建和 GPU feature 构建。
