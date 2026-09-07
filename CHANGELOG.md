# Changelog

格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，版本号遵循语义化版本。

## [Unreleased]

### Changed
- 应用标识符改为 `com.wangsheng.lansend`（与 Apple 开发者账号里已注册的 App ID 一致；0.2.0 的 macOS 应用用的是旧标识符，升级后设置目录不变，但系统权限记录会重新询问）。

## [0.2.0] - 2026-09-07

首个带界面的版本：macOS 与 Windows 桌面应用（Tauri 2），iOS 通过 TestFlight 分发；命令行工具改名 `lan-send-cli-*` 继续随发布提供。

### Added
- 发布流程：桌面应用包（macOS 通用 `.dmg` / `.app.zip`，Windows NSIS `-setup.exe` 与 `.msi`）与 iOS TestFlight 上传（配置 App Store Connect API Key secrets 后自动签名、打 `.ipa`、`altool` 上传；构建号取 GitHub 运行序号）；命令行产物改名 `lan-send-cli-*`。iOS 工程补上本地网络权限说明、Files 共享、出口合规声明；无 Downloads 目录的平台（iOS）默认把文件收到“文稿”。
- 里程碑 6：正式界面（`apps/app/src`）——按 `docs/ui-style-reference.md` 的金融风格实现：深色对角渐变底、半透明卡片与白色重点卡片、薄荷绿单一强调色、单色速度曲线；桌面侧栏布局，窄屏（iOS）自动切到底部标签栏；设备（在线/收藏/配对、发送、配对、重命名、忘记、按地址发送、拖放发送）、传输（进行中的大数字进度卡、速度与剩余时间、逐文件进度、取消/移除/在文件夹中显示）、剪贴板（同步开关、立即推送、历史与重新复制）、历史、设置（常规/网络/剪贴板/应用）；接收请求、PIN、同名冲突、配对（双向）、发送弹窗与提示条；中英文界面，深/浅色主题。浏览器假数据层可脱离 Tauri 预览。
- 里程碑 5：`lan_send_core::runtime`——把 CLI 里的编排（接受请求、落盘、续传、历史、配对、剪贴板同步、发送重试）下沉为核心库的事件式运行时：`Runtime::start` 启动服务、发现、探活与可选的剪贴板同步；所有需要用户决定的事情变成 `RuntimeEvent` + 应答方法（`respond_incoming`、`respond_conflict`、`provide_pin`、`respond_pair_request`、`pair_confirm`）；设备列表合并发现结果与数据库；每 30 s 探活、两次失败发 `device-lost`；进程内双实例集成测试覆盖 PIN、接受、拒绝、配对与解除配对。ADR-0013。
- 里程碑 5：Tauri 2 壳 `apps/app`（crate `lan-send-app`，加入工作区）：`cmd_<模块>_<动作>` 命令一一对应运行时 API（app / devices / transfer / pair / history / clipboard），运行时事件转发为 `event:<name>`（含简报要求的六个），桌面端托盘菜单、全局快捷键（默认 `CmdOrCtrl+Shift+V` 推送剪贴板，可配置）、关窗隐藏到托盘；`platform/{desktop,macos,windows,ios}.rs`；前端骨架 Vite + React 19 + TypeScript、类型化 IPC 层与浏览器假数据层（里程碑 6 换成正式界面）。
- 设置新增 `clipboard.syncEnabled`（默认开）与 `app { globalShortcut, closeToTray, theme(默认 dark), autoAcceptPaired, notifications }`；历史、设备、剪贴板记录可序列化供 IPC 使用。
- `deny.toml` 把许可证检查限定到 macOS / Windows / iOS 目标三元组（Tauri 在 Linux 会链接 LGPL 的 GTK，Linux 不是目标平台）。

## [0.1.0] - 2026-09-07

首个版本：命令行客户端，覆盖里程碑 1–3（协议、发现、传输、断点续传、IPv6、配对、剪贴板同步）。

### Added
- 发布流程：推送 `v*` 标签在 GitHub Actions 构建安装包并发布到 Releases——macOS 通用 `.pkg`（有证书时签名与公证）、Windows `.msi`（WiX，加入 PATH）、各平台免安装压缩包、`SHA256SUMS.txt`；ADR-0012，流程见 `docs/release.md`。
- 里程碑 3（第三部分）：剪贴板文件列表同步——复制文件时转为带 `x-lanext.intent = "clipboard"` 的普通传输，已配对的接收方自动接受并把收到的文件写入自己的剪贴板；`receive` 与 `clip watch` 共用同一套接收逻辑；`send` 的传输流程抽成可复用的 `transfer`。
- 里程碑 3（第二部分）：剪贴板同步——`ClipboardItem` 模型，`POST /api/ext/v1/clipboard`（JSON / multipart，仅限已配对设备，大小上限），同步引擎（回环防止环形缓冲、超限提示、并行推送），macOS `NSPasteboard` 后端（`changeCount` 轮询，PNG 优先、TIFF 转 PNG，文件 URL），Windows `clipboard-win` 后端（事件监听、PNG 优先、CF_DIB 转 PNG、CF_HDROP），敏感文本启发式，剪贴板历史（默认 50 条，图片存缓存目录），CLI `clip watch / push / history`。
- 里程碑 3（第一部分）：设备配对——`POST /api/ext/v1/pair` / `unpair`，双方各自显示由指纹派生的 6 位校验码并确认；服务端维护已配对指纹集合供私有端点鉴权；CLI `pair <device>`、`receive` 处理配对请求、`devices --unpair`，设备列表标记 P。
- 里程碑 2（第三部分）：IPv6——每个有 IPv6 的接口按索引加入 `ff12::fd3a:e420` 组播组，HTTP 服务同时监听 `[::]`（v6-only），链路本地地址带 scope 回拨（自定义 DNS 解析器编码 `fe80::1%3`），命令行目标支持 `[fe80::1%en1]:53317`；设置 `ipv6` 可关闭。
- 里程碑 2（第二部分）：断点续传扩展——`x-resume-token` / `x-resume-offsets`、带 `Range` 的上传、`GET /api/ext/v1/resume` 断点查询、会话内与跨会话恢复、上传空闲超时（30 秒）与会话闲置回收（10 分钟）；进程内端到端测试覆盖中断后续传与官方式发送方的兼容路径。
- 里程碑 2（第一部分）：`settings.json` 与 SQLite 持久化（传输历史默认 200 条、已知设备与收藏、断点记录表）；发送文件夹（递归、跳过符号链接与隐藏文件、保留相对路径、发送前汇总）；接收目录按设备 / 日期 / 类型分子目录，同名策略 rename / overwrite / ask；CLI 新增 `history`、`devices`，`send` 与 `receive` 读取设置并写入历史；发现阶段探测收藏与最近 7 天见过的设备地址。
- 里程碑 1：`lan-send-core` 的 `protocol`（v2.2 DTO、`x-lanext` 扩展字段、指纹）、`transport`（RSA-2048 自签名身份、强制/可选客户端证书的 rustls 策略、reqwest 客户端、axum 上传 API 服务端、流式落盘与 SHA-256/字节数校验、文件名净化）、`discovery`（每接口一个组播 socket、公告脉冲、HTTP register 回应、已知地址探测、/24 子网扫描回退）、`store`（应用目录、私钥文件权限）。
- CLI：`lan-send discover / send / receive / identity`，PIN 交互、进度条、`--auto-accept`、`--config-dir`、`--client-certs`。
- 互操作测试 `tests/interop/run.py`：与官方 `localsend-cli` 1.18.2 双向收发（pexpect 驱动官方 TUI），CI 在 Linux 与 macOS 上运行。
- 仓库骨架：Cargo 工作区、`lan-send-core` 与 `lan-send-cli` 空壳、命令行参数定义。
- 文档：开发简报、LocalSend v2.2 接口核对清单、协议差异与扩展、三端平台约束、UI 视觉参考、ADR-0001 仓库布局。
- CI：macOS / Windows / Linux 格式与 clippy 与测试，iOS 交叉检查，Windows 与 macOS CLI 产物，cargo-deny 许可证检查，标签发布。

[Unreleased]: https://github.com/TgolMsk/lan-send/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/TgolMsk/lan-send/releases/tag/v0.2.0
[0.1.0]: https://github.com/TgolMsk/lan-send/releases/tag/v0.1.0
