> **说明**：以下是 2026-09-06 交付的原始开发简报，逐字保留。之后的修订不改正文，只记在这里：
>
> - 2026-09-06：目标平台改为 **macOS、Windows、iOS 三端**（原为 macOS+Windows 优先、Linux 次之）。Linux 仅作为 CI 运行器跑核心库测试与互操作测试，不作为发布目标。
> - 2026-09-06：第 8 节的 `x-mtime` 扩展取消，改用协议 v2.1 起自带的 `metadata.modified / accessed`（官方 1.18 默认发送并写回文件时间戳）。
> - 2026-09-06：仓库公开托管于 GitHub（`TgolMsk/lan-send`），Windows 产物由 GitHub Actions 构建。
> - 2026-09-06：UI 视觉参考见 `docs/ui-style-reference.md`。

---

# 局域网互传工具 — 开发提示词

> 用途：交给 AI 编码助手（Claude Code / Cursor 等）作为项目级系统提示或首条任务提示。UI 方案稍后单独提供，本提示词只约束核心库、协议、平台集成与数据处理。

---

## 1. 项目定位

你正在开发一个开源的、跨平台（macOS + Windows 优先，Linux 次之）的局域网文件与剪贴板互传工具。它必须：

- **兼容 LocalSend Protocol v2**：能被官方 LocalSend 客户端发现并互相收发文件。协议规范以 LocalSend 仓库中的 `protocol.md` 为准，实现前先完整阅读，不要凭记忆猜接口。
- **在协议之上扩展私有能力**（剪贴板同步、媒体预览等），扩展字段必须做到"官方客户端忽略即可正常工作"，不能破坏兼容性。
- **面向两类用户**：开源社区用户，以及作者自己的日常使用。可靠性优先于功能数量。

许可证：MIT。所有依赖必须与 MIT 兼容，引入 GPL 依赖前必须停下来询问。

## 2. 技术栈与工程结构

```
repo/
├── crates/
│   ├── core/          # 纯 Rust 核心库，不依赖任何 GUI 框架
│   │   ├── discovery/ # 设备发现
│   │   ├── transport/ # HTTP/TLS 传输
│   │   ├── protocol/  # LocalSend v2 数据结构与私有扩展
│   │   ├── clipboard/ # 剪贴板抽象层
│   │   ├── media/     # 图片/音频探测、转码、缩略图
│   │   └── store/     # 传输历史、设备信任、设置持久化
│   └── cli/           # 命令行工具，用于协议验证与无 UI 场景
├── apps/
│   └── desktop/       # Tauri 2 壳（UI 方案后续提供，先只搭骨架与 IPC 命令）
├── docs/
│   ├── protocol-extensions.md  # 私有扩展字段的文档
│   └── adr/                    # 架构决策记录
└── tests/
    └── interop/       # 与官方 LocalSend 的互操作测试脚本
```

硬性规则：

- `core` crate 不得依赖 Tauri、任何窗口系统或平台 GUI 框架。它必须能在 CLI 中独立跑通全部功能。
- 所有跨平台差异通过 trait 抽象，平台实现放在 `#[cfg(target_os = ...)]` 模块中，禁止在业务逻辑里散落 `cfg`。
- 异步运行时统一使用 tokio。HTTP 服务端用 axum，客户端用 reqwest（启用 rustls，不用 openssl）。
- 错误处理：库内用 `thiserror` 定义具体错误类型，应用层用 `anyhow`。禁止 `unwrap()` 出现在非测试代码中，除非附带注释说明不变量。
- 每个模块先写 ADR（`docs/adr/NNNN-title.md`），再写代码。ADR 不超过一页。

## 3. 设备发现

- 实现 LocalSend 的 UDP 组播发现（组播地址与端口以协议文档为准，默认端口 53317），同时实现 HTTP 扫描回退（对 /24 网段内每个地址请求 `/api/localsend/v2/register`）。
- 在 macOS 上，组播需要声明 `com.apple.developer.networking.multicast`；非沙盒分发不需要，但代码要在启动时探测组播是否可用，不可用时自动降级到扫描，并在日志中明确记录原因。
- 设备身份 = TLS 证书指纹（SHA-256）。自签名证书在首次启动生成并持久化。
- 支持"收藏设备"：用户标记过的设备即使不在线也保留在列表中，按指纹匹配而非 IP。

## 4. 传输层

- 严格实现 v2 的 `prepare-upload` → `upload` → `cancel` 流程，以及 PIN 校验。
- 大文件：流式读写，内存占用不随文件大小增长。单文件 > 100 MB 时必须走流式，不允许整文件读入内存。
- 断点续传（私有扩展）：在 `prepare-upload` 响应中附带 `x-resume-token`，`upload` 支持 `Range` 头；对方是官方客户端时自动退回整文件重传。
- 完整性：每个文件传输完成后校验 SHA-256（发送方在元数据中携带，接收方计算后比对），不匹配则标记失败并保留临时文件供重试。
- 并发：同一会话内多文件并行上传，默认并发 3，可配置。
- 传输进度通过 `tokio::sync::watch` 或 broadcast channel 对外暴露，UI 层订阅即可，核心库不做任何 UI 假设。

## 5. 剪贴板同步

这是区别于 LocalSend 的核心功能，按以下要求实现。

### 5.1 数据模型

剪贴板内容统一抽象为 `ClipboardItem`：

```rust
enum ClipboardPayload {
    Text { plain: String, html: Option<String>, rtf: Option<String> },
    Image { format: ImageFormat, bytes: Bytes, width: u32, height: u32 },
    Files { paths: Vec<PathBuf> },   // 本地路径，跨设备时转为文件传输
}
struct ClipboardItem {
    id: Uuid,
    origin_device: DeviceId,
    created_at: DateTime<Utc>,
    payload: ClipboardPayload,
    content_hash: [u8; 32],
}
```

### 5.2 平台实现

- macOS：`NSPasteboard`，通过 `changeCount` 轮询（间隔 200–500 ms，可配置）检测变化。读取图片时优先取 PNG，其次 TIFF 转 PNG。
- Windows：`AddClipboardFormatListener` 事件驱动，不要轮询。读取图片优先 PNG（注册格式 `"PNG"`），其次 `CF_DIBV5` 转 PNG。
- 两端都要处理"剪贴板被其他进程锁住"的瞬时失败：指数退避重试 3 次后放弃并记录日志，不崩溃。

### 5.3 同步策略

- **回环防止**：写入远端内容到本地剪贴板前记录其 `content_hash`；本地监听器检测到变化时若 hash 与最近一次写入相同则忽略。保留最近 10 个 hash 的环形缓冲。
- **大小限制**：文本默认上限 1 MB，图片默认上限 10 MB，超限不自动同步，改为提示用户手动发送。
- **文件类型剪贴板**：不同步路径本身，而是转为一次常规文件传输（复用第 4 节）。对方收到后写入其剪贴板的文件列表（macOS `NSFilenamesPboardType` / Windows `CF_HDROP`）。
- **配对**：剪贴板同步只对"已配对"设备开启，配对流程 = 双向 PIN 确认一次，此后按证书指纹信任。未配对设备只能走手动发送。
- **传输通道**：走私有 HTTP 端点 `/api/ext/v1/clipboard`，请求体为 `ClipboardItem` 的 JSON（图片 bytes 用 base64 或 multipart，> 512 KB 时用 multipart）。
- **历史**：本地保留最近 N 条（默认 50）剪贴板记录，可查看、可重新复制、可删除、可一键清空。敏感内容（检测到形如密码/密钥的高熵字符串）默认不入历史，仅同步一次。

## 6. 图片支持

- 解码：支持 PNG、JPEG、GIF、WebP、BMP、TIFF、HEIC/HEIF、AVIF、SVG（仅识别与预览，不栅格化写回）。
- 库：优先 `image` crate；HEIC 用 `libheif-rs`；AVIF 用 `image` 的 avif feature。若某格式在某平台无法静态链接，退回为"按普通文件传输，不预览"，不阻断传输。
- 缩略图：接收方在传输完成后异步生成 256 px 缩略图，存入缓存目录，供传输历史展示。生成失败不影响主流程。
- 元数据：读取尺寸、EXIF 方向，预览时正确旋转。**不**在传输过程中剥离或修改原文件的 EXIF，保持字节级原样。
- 剪贴板图片默认以 PNG 传输；用户可在设置中选 JPEG（带质量参数）以减小体积。
- 提供可选的"发送前转换"：HEIC → JPEG（Windows 端接收更友好），默认关闭，开启后在发送方转换并保留原文件。

## 7. 音频支持

- 识别：MP3、AAC/M4A、FLAC、WAV、OGG/Opus、AIFF、WMA。用 `infer` 或 `symphonia` 探测，不信任扩展名。
- 元数据：用 `lofty` 读取标题、艺术家、专辑、时长、封面图，供传输历史展示。读取失败静默降级。
- 波形预览（可选，低优先级）：用 `symphonia` 解码后生成 200 点的峰值数组，供 UI 画波形。仅对 < 50 MB 的文件生成。
- 不做转码。音频一律原样传输。
- 语音消息快捷路径（可选）：允许从麦克风录一段 Opus 并直接发送，这是独立功能，放在最后一个里程碑。

## 8. 多格式文件支持

- MIME 探测：`infer` 按 magic bytes 探测，探测不到时用扩展名，两者都没有则 `application/octet-stream`。探测结果写入协议的 `fileType` 字段。
- 文件夹：递归打包为多文件会话，保留相对路径（协议 v2 的 `fileName` 允许含路径分隔符时使用，否则退回为 zip）。发送前统计总大小并向用户显示。
- 特殊文件：符号链接不跟随（记录警告并跳过）；macOS 的 `.app` 包和 Windows 的 `.lnk` 按普通文件/文件夹处理；隐藏文件默认跳过，可配置。
- 文件名安全：接收方对文件名做净化——去除路径穿越（`..`）、控制字符、平台保留名（Windows 的 `CON`、`NUL` 等），过长则截断并保留扩展名。同名冲突默认追加 `(1)`、`(2)`，可配置为覆盖或询问。
- 接收目录：默认系统下载目录，可按发送设备、按日期、按类型自动分子目录。
- 文本类文件（`.txt`、`.md`、代码文件）：接收后提供"复制内容到剪贴板"的快捷操作。
- 保存原始时间戳：尽量恢复文件的修改时间（协议扩展字段 `x-mtime`，对方是官方客户端时忽略）。

## 9. 安全

- 全部传输走 HTTPS，自签名证书，指纹即身份。首次连接未知设备时要求用户确认指纹。
- PIN 模式与 LocalSend 一致；剪贴板同步额外要求配对。
- 私有扩展端点必须校验请求来源的证书指纹在信任列表中，否则返回 403。
- 接收目录之外的任何路径写入都是 bug，写文件前必须校验最终路径在接收目录内。
- 不收集任何遥测。日志默认不记录文件名以外的内容，剪贴板文本内容永不进日志。

## 10. 持久化

- 设置：`~/.config/<app>/settings.json`（macOS 用 `~/Library/Application Support/<app>/`，Windows 用 `%APPDATA%\<app>\`），用 `directories` crate 取路径。
- 历史与设备信任：SQLite（`rusqlite`，bundled），单文件，schema 版本化，启动时自动迁移。
- 缓存（缩略图、波形）：系统缓存目录，可随时清空。

## 11. CLI 要求

`cli` crate 必须在 UI 完成前就能独立使用，作为互操作测试与自用工具：

```
lan-send discover                 # 列出局域网设备
lan-send send <device> <paths..>  # 发送文件/文件夹
lan-send receive [--dir DIR]      # 前台接收模式
lan-send clip watch <device>      # 开启剪贴板同步
lan-send clip push <device>       # 手动推送当前剪贴板
lan-send history [--limit N]
```

## 12. 测试

- 单元测试覆盖：文件名净化、回环防止、MIME 探测、断点续传偏移计算。
- 互操作测试：`tests/interop/` 下提供脚本，启动官方 LocalSend（headless 或 CLI 模式），验证双向收发。CI 中至少在 Linux 上跑通。
- 剪贴板平台实现用集成测试标记 `#[ignore]`，本地手动跑。

## 13. UI 接口约定（UI 方案后续提供）

在 UI 方案到位前，`apps/desktop` 只做以下事情：

- 搭 Tauri 2 骨架，注册托盘、全局快捷键（默认 `Cmd/Ctrl+Shift+V` 触发"发送剪贴板"，可配置）。
- 暴露 IPC 命令，一一对应 `core` 的公共 API，命名为 `cmd_<模块>_<动作>`。
- 事件推送：`event:transfer-progress`、`event:device-found`、`event:device-lost`、`event:clipboard-received`、`event:transfer-completed`、`event:error`。
- 不写任何视觉层。收到 UI 方案后再实现前端。

## 14. 工作方式

- 每次只做一个里程碑，完成后停下来汇报：做了什么、没做什么、下一步建议、需要我决策的问题。
- 遇到协议文档与实际官方客户端行为不一致时，以实际行为为准，并在 `docs/protocol-extensions.md` 中记录差异。
- 不确定的产品决策（默认值、限制、交互）先问，不要自行拍板。
- 每个 PR 附带 changelog 条目。

## 15. 里程碑顺序

1. `core::protocol` + `core::discovery` + `core::transport`，CLI 能与官方 LocalSend 互传单文件。
2. 多文件、文件夹、完整性校验、断点续传、传输历史。
3. 剪贴板：文本同步 → 图片同步 → 文件列表同步 → 历史。
4. 媒体层：MIME 探测、图片缩略图、音频元数据。
5. Tauri 骨架与 IPC。
6. （等 UI 方案）前端实现。
7. 可选：波形预览、语音消息、HEIC 转换、自动更新。

从里程碑 1 开始。先阅读 LocalSend 的 `protocol.md`，输出一份你理解的接口清单让我确认，再动手写代码。
