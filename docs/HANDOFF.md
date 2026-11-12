# 开发与验证

本文面向项目维护者和贡献者。开始修改前，请阅读[仓库维护规则](../AGENTS.md)、[架构设计](ARCHITECTURE.md)和 [HTTP API 文档](API.md)。部署和日志配置见[部署指南](DEPLOYMENT.md)。

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

仓库发布和 crates.io 发布彼此独立。如果同一版本要在 crates.io 发布两个包，先发 `fleece-render-core`，确认索引可解析该版本后再发 `fleece-render-server`。只发 Core 也可以，但此时服务只能经源码、二进制或容器等其他渠道分发，不能从 crates.io 安装。项目名和服务可执行文件仍为 `fleece-render`。

推送 `v0.1.0` 格式的版本标签后，`release` 会使用同一套构建步骤生成三个平台的二进制包，全部成功后自动发布到 GitHub Releases。如 `v0.1.0-rc.1` 的标签会发布为预发布版。该流程不发布 Cargo 包。

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

## 依赖公告

发布前检查 `Cargo.lock` 中的依赖。`fontdue` 当前间接依赖 `ttf-parser`，后者有停止维护公告 `RUSTSEC-2026-0192`，并非已知漏洞。服务只解析编译时内嵌的 Minecraft 字体，不接受上传的字体；后续升级渲染库时复核此依赖。

## 生成物

`output/` 不纳入 Git。默认服务日志位于 `output/logs/fleece-render.log`；HTTP 验收保存到 `output/acceptance/<时间戳>/`。手动生成的图片和性能采样也放在 `output/` 下。

## 维护约定

- 改动 HTTP 合约时更新 [API 文档](API.md)和对应测试；保留已公开路由及 camelCase 请求字段。
- 改动 crate 边界或请求处理流程时更新[架构设计](ARCHITECTURE.md)和本文。
- 改动模型、UV、相机或采样时更新[渲染实现](RENDERING.md)，并验证输出尺寸、确定性和 CPU 性能目标。
- 新的网络输入必须配置超时及字节限制。GPU 后端必须保持可选；修改光栅化时同时检查 CPU 构建和 GPU feature 构建。
