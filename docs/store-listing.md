# 应用商店上架：硬性要求与清单

整理日期：2026-09-08。适用于 iOS App Store 与 Mac App Store；命令行版继续走 GitHub Releases。

## 一、全球 App Store（不含中国大陆）需要什么

| 项目 | 要求 | 状态 |
|---|---|---|
| 开发者账号 | Apple Developer Program（已付费，Team ID `VVB976RN4W`） | ✅ |
| 构建 | 2026-04-28 起必须用 Xcode 26 / iOS 26 SDK 构建；CI 用 macos-latest 的 Xcode 26.6 | ✅ |
| 隐私政策 URL | 必填，公开可访问 | ✅ <https://tgolmsk.github.io/lan-send/privacy> |
| 技术支持 URL | 必填 | ✅ <https://tgolmsk.github.io/lan-send/support> |
| App 隐私（数据收集声明） | 在 App Store Connect 填写“不收集数据” | 待填 |
| 年龄分级问卷 | 2026 年新版问卷（含社交媒体问题），本应用无社交功能，预计 4+ | 待填 |
| 出口合规 | 使用标准加密（TLS），`ITSAppUsesNonExemptEncryption = false` 已写入 Info.plist | ✅ |
| 截图 | iPhone 6.9 英寸（1320×2868 或 1290×2796）必传；若支持 iPad 还要 13 英寸 iPad（2064×2752）；Mac 至少 1280×800 | 待做 |
| 图标 | 1024×1024，无透明（iOS）；macOS 圆角模板 | ✅ |
| 元数据 | 名称、副标题、描述、关键词、类别（工具）、版权 | 待填 |
| Mac App Store 额外 | 必须开启 App Sandbox 并声明网络/文件权限；用 Mac App Distribution + Mac Installer Distribution 证书签名；无需公证 | 待做（需要改代码） |
| 审核 | 功能完整、无崩溃、本地网络权限说明清楚（已加 `NSLocalNetworkUsageDescription`） | — |

## 二、中国大陆 App Store 额外需要什么

1. **ICP 备案号（App 备案）**：工信部 2023 年 8 月起要求，苹果已对新 App 做“强校验”——没有备案号的新 App 不能在中国大陆商店上架；个人开发者也一样。备案号在 App Store Connect 的 App 信息里填写。
2. **办理备案的前提**（以阿里云/腾讯云为接入商）：主办者为境内个人（身份证、手机号、人脸核验）或企业（营业执照）；名下要有一个已备案或同时备案的**域名**，以及一台**中国内地的包年包月云服务器**（作为接入服务）；App 上线 30 天内还要做**公安联网备案**。通常 1–4 周。没有后端服务器的应用也必须走这套流程，因为备案挂在接入商名下。
3. **类别许可**：游戏要版号，图书/期刊要网络出版许可，新闻要新闻信息服务许可；lan-send 是工具类，不涉及。
4. **个人信息保护**：隐私政策需覆盖《个人信息保护法》要求（收集范围、目的、用户权利）；lan-send 不收集数据，现有隐私政策已说明。
5. **不上中国大陆商店**则完全不需要备案：在 App Store Connect › 定价与销售范围里把“中国大陆”取消勾选即可，其余 174 个国家/地区正常上架。

**待决定**：是否上架中国大陆。上 → 需要你以个人身份办 App 备案（买域名 + 最便宜的包年包月内地云服务器 + 人脸核验，约 1–4 周）；不上 → 现在就可以提交全球商店。

## 三、Mac App Store 的技术改动（已完成，2026-09-08）

- App Sandbox 已开启（`apps/app/src-tauri/entitlements/mas.plist`，ADR-0014）：`network.client/server`、`files.user-selected.read-write`、`files.downloads.read-write`、`files.bookmarks.app-scope`，外加 `com.apple.application-identifier` / `team-identifier`（缺了 altool 会警告该构建不能用于 macOS TestFlight）。
- 自选接收目录用安全作用域书签保存（`platform/macos.rs`）。
- 证书、描述文件与 `app-mas` 任务见 `docs/release.md`；签名用 `Apple Distribution` + `3rd Party Mac Developer Installer`，`productbuild` 打 `.pkg`，`altool --type macos` 上传。

## 三点五、提交时踩过的坑（iOS 与 macOS 0.2.0 均已于 2026-09-08 提交审核）

- **审核信息里“需要登录”默认勾选**：不取消会因“用户名/密码为必填”而无法“添加以供审核”。应用没有账号，取消勾选即可。
- **出口合规**：Info.plist 里声明 `ITSAppUsesNonExemptEncryption=false`（iOS 在 `Info.ios.plist`，macOS 在 `Info.plist`），App Store Connect 就不再问加密问题。若走手动申报，选“标准加密算法”后会追问“是否在法国分发”，答“是”需上传法国的加密申报文件——直接用 plist 声明可避开。
- **macOS 沙盒说明**（版本页“App 沙盒信息”，可不填）：已为 `network.server`、`network.client`、`files.downloads.read-write`、`files.user-selected.read-write` 各写一句用途，方便审核员理解为何要监听端口。
- **构建版本替换**：版本页里点构建行的“删除”再“添加构建版本”；上传后要等处理完（TestFlight 页出现“准备提交”）才会出现在列表里。

## 四、参考

- [苹果 App Store 已开启 ICP 备案强校验](https://www.baijing.cn/article/48165)
- [关于个人开发者 App 上架 ICP 备案问题](https://blog.csdn.net/weixin_39339407/article/details/135037851)
- [阿里云 App 备案快速入门](https://help.aliyun.com/zh/icp-filing/basic-icp-service/getting-started/quick-sta-rt-for-icp-filing-for-personal-app)
- [Apple: 2026 年提交要求（Xcode 26 SDK）](https://9to5mac.com/2026/02/03/apple-to-update-minimum-sdk-requirements-for-all-app-store-submissions/)
- [App Store 年龄分级问卷 2026 年 9 月起必填](https://finance.sina.com.cn/tech/digi/2026-07-10/doc-inihhzsa3895645.shtml)
- [App Store 截图尺寸 2026](https://www.mobileaction.co/guide/app-screenshot-sizes-and-guidelines-for-the-app-store/)
- [Is app-sandbox entitlement required for App Store?](https://developer.apple.com/forums/thread/739482)
