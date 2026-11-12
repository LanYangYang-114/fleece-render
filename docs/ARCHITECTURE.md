# 架构设计

## 1. 目标和边界

fleece-render 被拆成两个主要 crate：

1. `fleece-render-core`：纯 Rust 的皮肤模型、场景构建、相机、CPU / 可选 GPU 光栅化、2D 头像合成和 PNG 输出。
2. `fleece-render-server`：Axum HTTP 路由、配置、profile provider、纹理输入、缓存、限流和日志。

核心 crate 不依赖 Axum、Reqwest、TOML、Mojang 或任何 HTTP 业务代码。这样可以在命令行、测试程序或未来其他服务中复用渲染引擎。

服务可在无图形界面的环境中运行。CPU 后端始终可用；Vulkan GPU 后端仅在启用 `gpu` Cargo feature 时编译。

## 2. Workspace 结构

```text
Cargo.toml
crates/
  fleece-render-core/
    src/
      avatar.rs        2D 头像图层合成
      backend.rs       RenderBackend、共享三角形预处理和 CPU 光栅化
      backend/gpu.rs   可选 Vulkan 设备、分块和像素读回
      backend/raster.wgsl  GPU 计算光栅化
      camera.rs        前视/后视透视相机
      geometry.rs      Box UV、三角形、Sprite 和场景数据
      math.rs          Vec2、Vec3、Mat4
      model.rs         classic/slim 模型、皮肤层和装备
      name_tag.rs      内嵌字体和名称标签
      render.rs        RenderEngine、风格注册和 PNG 编码
      texture.rs       图片解码、64×32 布局转换、alpha 和 slim 检测
      types.rs         RenderOptions、SkinAssets、RenderOutput
  fleece-render-server/
    src/
      config.rs        TOML 和环境变量
      error.rs         结构化 API 错误
      http.rs          路由、请求参数、响应头和 HTTP access log
      input.rs         URL、文件、data URL、Base64 和 raw body
      providers.rs     Mojang/LittleSkin profile provider
      lib.rs           AppState、缓存、并发和 spawn_blocking
      main.rs          CLI、日志、监听和优雅退出
scripts/               构建、测试、验收和运行脚本
docs/                  项目文档
output/                本地生成物，不提交 Git
```

## 3. 请求处理流程

```text
HTTP request
  -> TraceLayer 记录请求方法/路径/参数名
  -> 解析并校验 query 或 JSON
  -> 从 player、URL、文件、Base64 或 raw body 获取纹理
  -> reqwest 下载时执行超时、响应体字节限制和缓存
  -> 按风格准备纹理（3d 规范化，avatar 保留原图）
  -> RenderEngine
       -> 3d: build_scene(front/back) + CPU 或 GPU 光栅化
              + 最近邻 UV、超采样和阴影
       -> avatar: 直接从展开图采样基础脸部，可选合成帽子层
       -> 统一 PNG 编码
  -> PNG 或 JSON/Base64 response
  -> TraceLayer 记录状态码、响应大小和耗时
```

渲染在 `tokio::task::spawn_blocking` 中运行，`Semaphore` 默认限制同时运行的渲染任务为 4。即使 HTTP 超时，任务也会持有并发名额直到实际退出。

## 4. 核心扩展接口

### 新增风格

实现 `fleece_render_core::RenderStyle` 的 `id` 和 `label`，并通过 `StyleRegistry::register` 注册：

- 3D 风格实现 `build(assets, options, view)`，返回场景和模型类型，由后端负责光栅化；
- 平面风格可以实现 `render_flat(assets, options)`，直接返回一张前视 RGBA 图片和模型类型，无需构建 3D 场景。

HTTP 层负责按风格选择输入（如头像不加载披风），并将共用的 `showOuterLayer` 参数传给核心。是否构建外层网格或合成帽子层由核心决定。`avatar` 与 `3d` 共用下载缓存、渲染并发控制、PNG 编码和响应格式。

### 新增视角

当前 `Parts` 表示 `front`、`back`、`both`、`all`。新增视角时应：

1. 在核心层增加视图枚举或风格配置；
2. 保持现有 `Parts` 和已公开路由的语义不变；
3. 通过 `/api/v1` 扩展接口，保留已公开的 camelCase 请求字段；
4. 更新 `docs/API.md` 和验收测试。

### 渲染后端

`RenderBackend` 接口已经隔离后端：

```rust
pub trait RenderBackend: Send + Sync {
    fn id(&self) -> &'static str;
    fn render(&self, scene: &Scene, camera: &Camera, width: u32, height: u32)
        -> Result<RgbaImage>;
}
```

`gpu` feature 使用 `wgpu` 的原生 Vulkan 后端，不创建窗口或 surface。CPU 和 GPU 共用投影、背面剔除、排序与光照准备；GPU 将有序三角形按屏幕分块，每个计算线程负责一个像素的深度测试、最近邻采样和透明混合。一个渲染引擎复用 Vulkan 设备、队列与计算管线，并串行提交和读回。名牌、超采样降采样、阴影、背景和 PNG 编码仍使用共同的 CPU 路径。

`auto` 在初始化失败时选用 CPU，在请求期间遇到 GPU 后端错误时整次重试，避免拼图混用两个后端。显式 `gpu` 不回退。

## 5. 数据和确定性约束

- 纹理采样使用最近邻，沿用现有 box UV 和三角形绕序。
- 同一后端和设备上，相同纹理和选项必须产生字节级一致的 PNG，与 HTTP 输入形式无关。CPU 和 GPU（或不同 GPU 驱动）之间的浮点运算细节可能导致边缘像素不同，不要求跨后端 PNG 字节一致。
- 透明背景通过 RGBA 合成保留 alpha；白底只在 `transparent=false` 时生成。
- 3D 风格将 64×32 皮肤转换为 64×64 逻辑布局；头像直接读取原图，避免转换影响帽子层的判断。
- classic/slim 自动检测读取手臂 UV 的未使用区域，规则见[渲染实现](RENDERING.md#slim-检测)。
- 核心渲染不读取网络，不访问环境变量，也不写日志文件。

## 6. 跨平台策略

- Windows：项目脚本支持仓库内隔离 Rust/Cargo 和 LLVM-MinGW；标准 MSVC 工具链也可用。
- Linux：使用系统 Rust 或脚本提供的标准 Cargo 流程。
- Docker：使用多阶段构建，运行阶段不依赖图形环境或 GPU 驱动。
- TLS：Reqwest 使用 rustls，避免依赖系统 OpenSSL。
- Unix 的 SIGTERM 处理通过 `cfg(unix)`，Windows 使用 Ctrl+C 关闭。

## 7. 日志和可观测性

`TraceLayer` 对每个请求记录：

- HTTP 方法和 path；
- query 参数名，不记录参数值；
- request/response Content-Length；
- HTTP 状态码和响应耗时。

日志的存储位置和配置见[部署指南](DEPLOYMENT.md#日志)。
