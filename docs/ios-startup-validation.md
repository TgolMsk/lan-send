# iOS 27 启动崩溃修复（0.5.1 构建 27）

日期：2026-10-05（PDT）。

## 真机证据与根因

用户通过 TestFlight 安装 0.5.1（26），打开立即闪退。已从 App Store Connect 下载对应 iPhone 14 Pro Max / iOS 27.0 的崩溃反馈；启动到崩溃间隔约 0.09 秒，主线程在 UIKit 的 `___UIApplicationEvaluateRuntimeIssueForNoSceneLifecycleAdoption_block_invoke` 中触发 `EXC_BREAKPOINT (SIGTRAP)`，下层为 Tao / Tauri 的 `UIApplicationMain`，尚未进入应用网络服务。

构建 26 的签名成品缺少 `UIApplicationSceneManifest`。Apple 要求使用 iOS 27 SDK 构建的应用采用 Scene 生命周期：[Apple 生命周期迁移文档](https://developer.apple.com/documentation/uikit/transitioning-to-the-uikit-scene-based-life-cycle)。此外，原 Tao 0.35.3 在 Scene 配置回调中返回已释放对象；上游 [修复 1245](https://github.com/tauri-apps/tao/pull/1245) 已将返回值改为 `Retained::autorelease_ptr(config)`。只增加空 Scene 配置仍不满足启动验证，参见 [Tauri 问题 15719](https://github.com/tauri-apps/tauri/issues/15719)。

原始反馈 ZIP / crashlog 和构建 26 归档仅保存在被 Git 忽略的 `dist/ios-crash-26/`，不提交测试员联系方式或设备私有路径。

## 修复

- Tauri / 前端 API / CLI 升至正式版 2.12.1，锁定 Tao 0.37.1；该版本包含 Scene 配置对象生命周期修复、单窗口 Scene 支持和 Scene 的 `Resumed` 事件，因此保留现有前台网络恢复代码。
- iOS 合并配置、生成的 Info.plist 与 XcodeGen 模板均声明静态 `TaoSceneDelegate` 配置，`UIApplicationSupportsMultipleScenes=false`，保持单窗口使用方式。
- 最低 Rust 版本随 Tauri 升至 1.90。Tauri 移除了 unic，删除对应五项不再需要的 RustSec 忽略项。
- `check-ios-networking.py` 同时检查源码、生成项目和签名成品中的 Scene 配置；旧构建 26 被此检查明确拒绝。增加缺失配置、空配置、错误原生 delegate、单窗口约束的回归测试，并纳入 CI。

## 验证

| 检查 | 结果 / 范围 |
| --- | --- |
| workspace fmt / Clippy / 测试 | 通过；85 项 Rust 测试通过、1 项原有真实剪贴板测试跳过 |
| cargo-deny | 通过，含删除过时例外后的复查 |
| 前端构建与八语言校验 | 通过 |
| Python 发布 / 启动检查测试 | 5 项通过 |
| 旧构建 26 IPA | 新检查因缺少 Scene 配置失败，匹配真机报告 |
| iOS 27.0 优化模拟器构建 | 通过；界面正常显示并发现现有 Windows 与 Mac 对端 |
| 模拟器三次冷启动 | 三次均保持运行，mTLS HTTPS 接收端响应，身份不变 |
| 模拟器切后台再返回 | 进程 PID 不变、身份不变，接收端恢复响应 |
| 真机架构优化归档 / 分发签名 / IPA 导出 | 通过；成品 Scene 配置、签名与描述文件的组播权限检查通过 |

模拟器与 Mac 共享网络端口，测试时只将模拟器自身配置端口设为 53327；Mac 的 53317 端口保持原状。移除 Scene 配置的模拟器对照包没有复现真机的系统断言，因此不将模拟器启动结果作为真机问题完全消失的证据。根因证据来自真实 TestFlight 崩溃报告与构建 26 的签名包配置。

构建 27 IPA：`dist/ios-0.5.1-27/LanSend.ipa`；SHA-256 `7f40889339ad86bbd00d66acc314c86e81a2b262ff5dc715befe2a63260f3e97`。2026-10-05 14:53:09 PDT，本地 Xcode 上传完成，日志确认 `Upload succeeded`。

本机实体 iPhone 仍显示 unavailable。构建 27 在用户 iPhone 上的冷启动及真实 Windows / Mac 互传，需要更新 TestFlight 后复测。
