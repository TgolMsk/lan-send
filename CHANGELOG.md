# Changelog

格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，版本号遵循语义化版本。

## [Unreleased]

### Added
- 界面支持八种语言（简中、繁中、英、日、韩、德、法、西），文案表移到 `apps/app/src/locales/*.json`，`pnpm build` 前校验键集合与占位符；语言偏好成为设置项 `app.language`（默认跟随系统），托盘菜单与文件对话框标题跟随该设置，iOS / macOS 的系统权限弹窗按 `*.lproj/InfoPlist.strings` 本地化（ADR-0016）。
- 核心层新增 `runtime::ErrorCode`：传输失败、配对失败、IPC 命令错误都带语言无关的编码，前端据此显示翻译后的提示，未编码的保留原文。
- 商店文案八种语言的草稿（`docs/store-listing.md` §五）。
- iOS 与 Mac App Store 沙盒版加上 `com.apple.developer.networking.multicast` 权利（Apple 于 2026-09-13 批准申请 HTFYV6DZUK）。此前这两种构建收不到也发不出组播公告，手机端只能靠电脑主动探测才会互相出现；现在与桌面版一样走组播发现。沙盒版的描述文件需重建后更新 `MAS_PROVISIONING_PROFILE`，步骤见 `docs/release.md`。

### Fixed
- Mac App Store 沙盒版的默认接收目录显式指向真实的 `~/Downloads`：沙盒把 `$HOME` 及所有 Foundation 主目录查询都重定向到容器，`directories` crate 算出的是容器路径（只靠沙盒创建的符号链接才写到真实目录，设置页显示的也是容器路径）。macOS 上改用 `getpwuid_r` 取真实主目录。App Review 2.4.5(i) 的静态扫描看不见 Rust `std::fs` 的写入，曾判定 `com.apple.security.files.downloads.read-write` 无对应功能；显式路径加上审核备注里的说明是应对办法。
- 支持页（`docs/support.md`）补上电子邮件联系方式与更完整的 FAQ；App Review 按 1.5 认为只有 GitHub issues 链接不算可用的支持渠道。

### Changed
- 新图标：深蓝圆角方块内衬白底的衬线 L 与薄荷绿纸飞机（源图 `docs/brand/logo.jpeg`）。`apps/app/scripts/make-icons.py` 从源图裁出满铺方形（iOS / Windows）、Apple 圆角模板（macOS）与侧栏品牌标，再由 `cargo tauri icon` 生成各尺寸；iOS 图标去掉 alpha 通道。商店截图随之重出。
- 应用改名为 **LanSend**（产品名、窗口标题、`CFBundleDisplayName`、托盘、文档与商店文案）；bundle id `com.wangsheng.lansend`、数据目录 `lan-send`、CLI 二进制名与 WiX `UpgradeCode` 保持不变，升级不丢数据。
- `lan-send identity` 多打印一行 `Receive dir`，方便确认文件会收到哪里。

### Added
- iOS 发送时可以直接从相册选图片和视频：发送弹窗在移动端多一个“相册”按钮，走系统照片选择器（`PHPicker`），选中的项目由系统复制到应用临时目录后按普通文件发送，不需要先存进“文件”App。选择类型从布尔的 `folders` 改为 `files / folders / media`；`Info.ios.plist` 补上 `NSPhotoLibraryUsageDescription`。

## [0.3.0] - 2026-09-08

媒体层（里程碑 4）与 macOS 窗口拖动修复。

### Added
- `docs/store-listing.md`：记录 iOS 因 2.1 Information Needed 被拒（新账号例行补充资料）需要的六项内容与备注字段 4000 字符上限。
- `docs/store-listing.md`：记录 macOS 版因 `com.apple.security.network.server` 被自动分析判定"无对应功能"而被拒（2.4.5）的原因与处理流程（备注说明 + 回复审核 + 更新审核后重新提交，无需换构建）。
- 里程碑 4：媒体层（ADR-0015）。`core::media`：按 magic bytes 探测 MIME（`infer`，退回扩展名）并写入协议 `fileType`，接收到通用类型时落盘后重新探测；256 px 缩略图（JPEG 85，透明合成中性灰，EXIF 方向统一用 `kamadak-exif` 处理），解码先走系统解码器——macOS / iOS 用 ImageIO（HEIC / AVIF 原生），Windows 用 WIC（HEIC / AVIF 需系统扩展）——再退回纯 Rust 的 `image`；缩略图缓存在系统缓存目录 `lan-send/thumbs`，上限 200 MB 按最近使用淘汰；音频用 `lofty` 读标题 / 艺术家 / 专辑 / 时长 / 封面。运行时新增 `media_info`、`media_cache_size`、`media_cache_clear` 与事件 `media-ready`（传输完成后后台预热）；Tauri 命令 `cmd_media_*`；历史与传输页显示缩略图与尺寸 / 时长，设置页可查看并清空缩略图缓存。不引入 libheif / libdav1d（LGPL / 系统 C 库）。
- README 与 GitHub Pages 首页加入应用截图（`docs/screenshots/`，由商店截图缩小生成），并更新项目状态与 iOS 下载说明。
- Mac App Store 沙盒版（ADR-0014）：`entitlements/mas.plist`，接收目录的安全作用域书签，`app-mas` 发布任务（证书 secrets 齐全时签名、`productbuild`、上传 App Store Connect）。

### Changed
- 显示名称改为 `Lan-Send`（窗口标题、Bundle 显示名、托盘、侧栏），macOS 应用包名 `Lan-Send.app`。
- 三端统一图标：采用新的 Lan-Send 品牌标志（`docs/brand/logo.jpeg`），iOS / Windows 用满铺方形，macOS 按 Apple 圆角模板生成，应用侧栏的品牌标也换成它；隐私政策与支持页发布在 GitHub Pages（上架必填）；`docs/store-listing.md` 记录上架硬性要求。
- 应用标识符改为 `com.wangsheng.lansend`（与 Apple 开发者账号里已注册的 App ID 一致；0.2.0 的 macOS 应用用的是旧标识符，升级后设置目录不变，但系统权限记录会重新询问）。
- `docs/release.md`：Mac App Store 证书改为用 `openssl` 生成 CSR / 合成 `.p12`（含 WWDR G3 链，`-legacy`），并写明 secrets 命令与到期续期。
- `docs/store-listing.md`：Mac App Store 技术改动标记完成，记录提交审核时的注意事项（“需要登录”默认勾选、出口合规声明、沙盒说明、替换构建）。
- `docs/release.md`：记录 iOS 组播权限申请（2026-09-08，Request ID HTFYV6DZUK）与批准后的启用步骤。

### Fixed
- iOS 真机上接收文件全部失败（`cannot use <容器>/Downloads: Operation not permitted`）。`directories` crate 在 iOS 上套用 macOS 布局，把默认接收目录指到容器根下的 `Downloads`，而 iOS 容器根不可写。默认接收目录在 iOS 上改用 `Documents`（本就可写，且在“文件”App 里可见）。模拟器容器根是普通目录，复现不了，只有真机能测出来。
- 收到传输请求时 `IncomingRequest` 事件先于待决请求登记发出，界面若在收到事件的瞬间就应答会拿到 `NothingPending`（CI 上偶发的 `decline_and_pairing_round_trip` 失败即由此而来）。改为先登记再发事件，与配对请求的做法一致。
- iOS：运行时启动失败时顶部的红色提示条会钻到状态栏和灵动岛下面。安全区上边距从页头移到内容容器，页头或提示条谁在最前面都能避开状态栏。
- macOS 窗口几乎拖不动：窗口没有原生标题栏，拖动区却是用 `-webkit-app-region` 写的，而 WKWebView 根本不支持这个属性，实际只有侧栏顶部 22 px 的空条能拖。改用 Tauri 的 `data-tauri-drag-region="deep"`（tauri ≥ 2.11）标在侧栏与页头上，整个侧栏（含品牌标、身份卡、空白处）和内容区顶部都能拖动窗口；导航按钮、搜索框与卡片按钮由 Tauri 自动排除，仍然可点。
- macOS 应用的 Info.plist 补上出口合规声明（`ITSAppUsesNonExemptEncryption=false`，否则 App Store Connect 标记“缺少出口合规证明”）与本地网络用途说明（macOS 15 起会询问）。
- Mac App Store 沙盒版签名补上 `com.apple.application-identifier` / `team-identifier`（与描述文件一致），否则 altool 警告该构建不能用于 macOS TestFlight。
- 设置页底部的指纹与目录路径过长时把页面撑宽、iOS 上可以横向拖动：长字符串按字符换行，内容区不再横向滚动。

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

[Unreleased]: https://github.com/TgolMsk/lan-send/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/TgolMsk/lan-send/releases/tag/v0.3.0
[0.2.0]: https://github.com/TgolMsk/lan-send/releases/tag/v0.2.0
[0.1.0]: https://github.com/TgolMsk/lan-send/releases/tag/v0.1.0
