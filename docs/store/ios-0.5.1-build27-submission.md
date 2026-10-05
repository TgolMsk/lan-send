# iOS 0.5.1（27）修复构建发布记录

2026-10-05 15:08 PDT（22:08 UTC），构建 27 已重新提交 App Store 审核。详情页显示 **等待审核**，关联的版本为 **0.5.1（27）**。发布方式保留审核通过后自动发布。

| 项目 | 结果 |
| --- | --- |
| App / Bundle ID | LanSend `6809459213` / `com.wangsheng.lansend` |
| 版本 / 构建 | `0.5.1` / `27` |
| 构建 ID | `546cfda7-fa37-45e3-b8a2-42eb53a10fe5` |
| 新提交 ID | `15d0e217-0a58-4f6a-ac23-62d35bcef906` |
| App Store | 等待审核；审核后自动发布 |
| TestFlight | Internal 内部测试组「正在测试」，1 位现有测试员可安装 |
| 真机启动回测 | 用户更新后确认「构建 27 能正常打开」 |
| 修复源代码提交 | `64babb1` |
| IPA SHA-256 | `7f40889339ad86bbd00d66acc314c86e81a2b262ff5dc715befe2a63260f3e97` |

[App Store 审核详情](https://appstoreconnect.apple.com/apps/6809459213/distribution/reviewsubmissions/details/15d0e217-0a58-4f6a-ac23-62d35bcef906) · [TestFlight 测试组](https://appstoreconnect.apple.com/teams/2cd8deef-1942-4bd5-ae9e-378117ec977f/apps/6809459213/testflight/groups/511f2591-e025-48ee-aec0-8f9183116df3/builds)。

修复与验证：[ios-startup-validation.md](../ios-startup-validation.md)。更新说明与审核备注使用 [ios-0.5.1.json](ios-0.5.1.json)，八种语言均包含 iOS 27 启动修复。保留既有描述、关键词、截图、审核联系信息和无登录设置。

构建 26 的原提交 `fed6120d-e591-42ba-82ac-92c1507d543f` 已撤回并显示「已移除」。版本关联先移除 26 并保存，再选择 27、保存、添加以供审核，最后完成提交。该流程没有再次上传 26，也没有修改 macOS 上架版本。

本地凭据：`dist/ios-0.5.1-27/testflight-testing.jpg`、`app-store-submitted.jpg`、`ios27-running.png`、`startup-smoke.json`，均不纳入 Git。发布状态仅证明上传、测试分发及审核提交已经保存；不代表 Apple 已批准或新版本已公开上架。真实 Windows / Mac 与 iPhone 互传仍需另行复测。
