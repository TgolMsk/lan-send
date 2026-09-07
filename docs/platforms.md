# 平台约束（macOS / Windows / iOS）

本文记录三端各自的硬约束，功能设计与里程碑排期以此为准。发现新的约束就补充到这里。

## macOS

- 组播：非沙盒分发不需要额外权限；沙盒/App Store 分发需要 `com.apple.developer.networking.multicast` 权利。macOS 15+ 首次访问局域网会弹"本地网络"授权，需在 `Info.plist` 写 `NSLocalNetworkUsageDescription`。
- 组播 socket：每个接口地址各绑一个 socket，`SO_REUSEADDR + SO_REUSEPORT`，绑定 `0.0.0.0`，`IP_MULTICAST_IF` 指定接口，loopback 开启（同机多实例互见）。
- 剪贴板：`NSPasteboard` 无变更通知，只能轮询 `changeCount`（200–500 ms）。图片优先取 PNG，其次 TIFF 转 PNG。
- 托盘、全局快捷键、Dock 图标：桌面专属，放在 `apps/app/src-tauri/src/platform/macos.rs`。

## Windows

- 首次绑定 53317 会触发防火墙提示；安装器需要预置入站规则或在首次运行引导用户允许。
- 无 `SO_REUSEPORT`，只用 `SO_REUSEADDR`。
- 剪贴板：`AddClipboardFormatListener` 事件驱动，不轮询；图片优先注册格式 `"PNG"`，其次 `CF_DIBV5` 转 PNG。剪贴板被其他进程占用时 `OpenClipboard` 会瞬时失败，指数退避重试 3 次。
- 文件名：NTFS 非法字符、保留设备名（`CON`、`NUL`、`COM1`…）、末尾点/空格都要净化。
- 构建：由 GitHub Actions 的 `windows-latest` 运行器构建 CLI 与后续的 Tauri 安装包。

## iOS

- 组播需要 `com.apple.developer.networking.multicast` 权利，且该权利必须向 Apple 申请。未获批前 iOS 端**只能**依赖 HTTP 扫描、收藏/配对设备直连，以及"被别人发现"（别的设备公告时，iOS 端能否收到组播同样受限）。发现模块必须把"无组播"当作正常路径。
- 本地网络隐私：`NSLocalNetworkUsageDescription` 必填；`NSBonjourServices` 仅在用 Bonjour 时需要（当前不用）。
- 后台：App 挂起时系统会回收监听 socket，回到前台必须重建 HTTP 服务与组播（官方 LocalSend 的 `ListenerFailed` / `SocketsFailed` 事件就是为此设计）。长时间传输需要 `beginBackgroundTask` 争取几分钟，超时即中断——断点续传扩展在 iOS 上价值最大。
- 剪贴板：`UIPasteboard` 只能在前台轮询 `changeCount`；iOS 16+ 读取他人写入的剪贴板会弹"允许粘贴"提示。"后台持续同步"在 iOS 上不可行，产品上应定义为"前台同步 + 手动推送"。
- 文件：接收目录是 App 沙盒的 `Documents/`（可通过"文件"App 访问），没有系统"下载目录"。
- 构建：Tauri 2 iOS 目标，`apps/app/src-tauri/gen/apple`。需要 Xcode（本机目前仅 Command Line Tools）与开发者证书；CI 先做核心库交叉 `cargo check`，真机/TestFlight 打包待证书就绪。
- CLI 不发布到 iOS。

## 三端共同

- 设备身份：自签名 RSA-2048 证书，指纹 = DER 的 SHA-256 大写 hex，首次启动生成并持久化。
- 官方 LocalSend 1.18+ 强制客户端证书（mTLS），我们的客户端必须出示证书；服务端策略见接口清单第 10 节的决策项。
- 任何写文件前必须确认最终路径位于接收目录内。
