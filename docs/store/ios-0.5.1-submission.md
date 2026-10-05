# iOS 0.5.1（26）App Store 提交记录

2026-10-05 13:51 PDT（20:51 UTC），通过用户已登录的 Chrome 在 App Store Connect 创建 iOS 0.5.1、关联构建 26，并完成「提交以供审核」。提交详情页已显示 **等待审核**。

| 项目 | 已核对值 |
| --- | --- |
| App | LanSend / `6809459213` |
| Bundle ID / Team | `com.wangsheng.lansend` / `VVB976RN4W` |
| iOS 版本 / 构建 | `0.5.1` / `26` |
| 构建 ID | `a00e8dfb-e037-4fa9-bc87-5b26a2e6fa3f` |
| 提交 ID | `fed6120d-e591-42ba-82ac-92c1507d543f` |
| 审核状态 | 等待审核（`WAITING_FOR_REVIEW`） |
| 发布方式 | 审核通过后自动发布；立即向所有用户发布更新 |
| IPA SHA-256 | `5c02ef05061561e7dfaf91a1d9584f998d2d0072fc0eb572b052e3aba8fbdbcd` |

审核详情：[App Store Connect](https://appstoreconnect.apple.com/apps/6809459213/distribution/reviewsubmissions/details/fed6120d-e591-42ba-82ac-92c1507d543f)。

## 上传与元数据

- 本地 Xcode 完成优化的 Release archive、自动分发签名及 IPA 导出。成品签名和内嵌分发描述文件均通过 `scripts/check-ios-networking.py --ipa` 的组播权限检查。
- 13:39:34 PDT，Xcode 上传日志出现 `Upload succeeded` 和 `EXPORT SUCCEEDED`。Apple 处理完成后，网页构建选择器出现 `0.5.1 (26)`，已关联并保存。
- 八种既有语言（简中、繁中、英、德、法、西、日、韩）的更新说明和审核备注使用 [ios-0.5.1.json](ios-0.5.1.json)。沿用已有描述、关键词、截图、审核联系信息及发布方式。「需要登录」保持关闭。
- GitHub Actions 受托管 runner 分配延迟影响。本地上传后，所有排队的准备/提交工作流均已取消，包括 `37371534748`，避免网页操作与 CI 重复提交。当前发布没有依赖这些排队任务。
- 本地提交截图：`dist/ios-0.5.1-26/app-store-submitted.jpg`；签名产物：`dist/ios-0.5.1-26/LanSend.ipa`（二者均不纳入 Git）。

## 验证边界

源代码修复、回归测试、iOS 原生构建及签名核验见 [ios-network-validation.md](../ios-network-validation.md)。此次没有连通用户的实体 iPhone，Windows / Mac 与真实 iPhone 的发现和互传仍需安装新构建后复测。

「等待审核」证明提交已经持久保存，不代表 Apple 已批准或 0.5.1 已经公开上架。审核通过后将按已保存设置自动发布。

## TestFlight 分发核对

同日按用户要求检查 TestFlight，既有 **Internal** 内部测试组已包含 iOS `0.5.1 (26)`，页面状态为 **正在测试**，显示 90 天后过期。该组有 1 位现有测试员，可通过 TestFlight 安装或更新；此次无需再次上传或添加构建。

- 测试组 ID：`511f2591-e025-48ee-aec0-8f9183116df3`。
- [测试组构建列表](https://appstoreconnect.apple.com/teams/2cd8deef-1942-4bd5-ae9e-378117ec977f/apps/6809459213/testflight/groups/511f2591-e025-48ee-aec0-8f9183116df3/builds)。
- 状态截图：`dist/ios-0.5.1-26/testflight-testing.jpg`（不纳入 Git）。
