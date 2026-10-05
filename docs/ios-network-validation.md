# iOS 发现与接收修复验证

日期：2026-10-05。

用户现象：Mac 与 Windows 正常互传；iOS 能发现并发送到 Mac，发现不了 Windows；两个桌面端都无法向 iOS 发送。

## 确认的代码问题

1. iOS 的 `lan-send-app_iOS.entitlements` 实际是空字典，缺少 `com.apple.developer.networking.multicast`。已补入权利文件和 XcodeGen 模板。Apple 要求 iOS 收发 IP 组播具备此权利：[Multicast Networking](https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.developer.networking.multicast)。当前安装在用户手机上的包是否也缺少该权利，尚未读取签名确认。
2. 分级发现只在没有任何确认时扫描子网。一个已知 Mac 的成功注册就能阻止寻找 Windows。移动端现在每次发现都会扫描，无组播的桌面端也不再因一个已知设备在线而跳过扫描；移动端每 30 s 重试发现，同一时间只运行一次。
3. iOS 平台层未处理返回前台事件。现在通过 Tauri `WindowEvent::Resumed` 重建接收服务及组播，运行时的启动/停止串行执行，身份、设置和历史保持持久化。事件转发先终止旧任务，在界面重置旧会话状态后再发送新服务的请求，避免旧弹窗残留或新的接收请求被重置擦除。
4. TLS 握手缺少取消路径，未发送 ClientHello 的连接会阻塞服务停止，进而阻塞重启。回归测试在原实现上复现停止超时；修改后握手可取消，并设 10 s 握手超时。

这些问题有源码和本机回归证据；仍需真机确定它们是否覆盖用户遇到的全部接收失败原因。

## 已完成的本机验证

| 验证 | 结果与范围 |
|---|---|
| `cargo test -p lan-send-core` | 83 通过、1 跳过；跳过项为原有真实 macOS 剪贴板测试 |
| 已知 Mac + 未知 Windows 的发现回归 | 两项通过；使用本机回环上的真实 mTLS 服务模拟协议对端，分别覆盖部分组播可用和无组播。不是实际 Windows 系统的互联结果 |
| 未完成 TLS 握手下停止/重新绑定 | 修复前超时，修复后通过；停止无需等对端完成握手，接收端口能立即重绑 |
| 桌面向移动接收端发送、接收服务重启后接收 | 通过；指纹和端口保持一致，文件字节完全一致。使用本机 Rust runtime，没有真实 iOS 沙盒 |
| `cargo test -p lan-send-app -p lan-send-cli` | 通过，应用层 2 项测试 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 通过 |
| iOS 模拟器目标 `cargo check -p lan-send-app --lib --target aarch64-apple-ios-sim` | 通过，包括 iOS 前台事件处理代码 |
| `pnpm build` | 通过，含 TypeScript 和八语言文案校验 |
| `python3 scripts/check-ios-networking.py` | 源码声明通过；发布任务已加入签名/描述文件检查，尚未运行签名 IPA 检查 |
| `cargo tauri ios build --ci --debug --no-sign --archive-only --target aarch64` | 完整 iOS arm64 归档通过；路径 `apps/app/src-tauri/gen/apple/build/lan-send-app_iOS.xcarchive`。无签名产物不能直接安装，也不证明真机组播权限有效 |

## 0.5.1 发布依赖检查

发布检查更新 rustls 0.23.45、time 0.3.47、plist 1.10.1 / quick-xml 0.42.0，修复 RustSec 已有补丁的告警；这些依赖要求工作区最低 Rust 版本升至 1.88，相关 Clippy 建议同步调整。

`deny.toml` 对无补丁条目作了具体范围评估：RSA crate 仅在 `transport/identity.rs` 生成本地密钥、编码 PKCS#8，网络 TLS 私钥操作走 rustls/ring，不调用 rsa 的易受攻击私钥运算（RUSTSEC-2023-0071）；paste（lofty）和 unic（Tauri/urlpattern）的六项告警为停止维护通知。仅忽略上述具体 ID，仍检查其余漏洞、撤回版本、许可证与来源。

## 真机验收待完成

本次 `xcrun devicectl list devices` 中用户 iPhone 状态为 `unavailable`，无法安装或读取当前 App 签名。Windows 真机未由本任务接入。

1. 用含 Multicast Networking 能力的签名及描述文件构建，运行 `python3 scripts/check-ios-networking.py --ipa <signed.ipa>`，安装到 iPhone。
2. 三端处于同一局域网，iPhone 允许 LanSend 的“本地网络”访问，先保持 LanSend 前台且屏幕亮着。
3. Mac 先在线，Windows 后启动：iPhone 刷新后应同时发现两者；不手动刷新时应在下一次 30 s 发现轮次后出现。
4. 分别执行 Windows → iPhone、Mac → iPhone、iPhone → Windows、iPhone → Mac。确认 iPhone 出现接收弹窗，接收文件在“文件”App 的 LanSend Documents 中存在且 SHA-256 一致。
5. 锁屏或切换后台后回到 LanSend，再执行 Mac/Windows → iPhone，确认监听恢复且没有旧会话弹窗残留。当前修复不保证应用挂起时仍能持续传输。
6. 若仍失败，保留发送方具体错误、iPhone 是否收到接收弹窗、两端 IP/端口，以及 iOS 网络启动日志，继续区分连接失败、授权、会话确认和沙盒写入问题。
