# 0014. Mac App Store：沙盒与上传

- 状态：已接受
- 日期：2026-09-08

## 背景

用户要求 macOS 应用上架 Mac App Store。商店强制 App Sandbox；直接分发的 `.dmg`（Developer ID + 公证）不需要。两种分发要并存。

## 决策

1. **两套签名，一套代码**。`.dmg` 保持现状（hardened runtime，无沙盒，与命令行版共用 `~/Library/Application Support/lan-send`）。Mac App Store 构建用 `apps/app/src-tauri/entitlements/mas.plist` 开启沙盒：`network.client`、`network.server`（HTTPS 服务与组播）、`files.user-selected.read-write`（选择要发的文件、选择接收目录）、`files.downloads.read-write`（默认接收目录）、`files.bookmarks.app-scope`。沙盒内配置目录自动落到容器（`~/Library/Containers/com.wangsheng.lansend/…`），与命令行版不再共享身份和历史，这是接受的代价。
2. **接收目录的持久授权**。沙盒里用户在文件对话框选的目录只在本次运行有效。macOS 平台代码（`platform/macos.rs`）在用户选定接收目录时创建带安全作用域的书签，存到配置目录的 `receive-dir.bookmark`；启动后解析书签并 `startAccessingSecurityScopedResource`，整个进程生命周期保持；书签失效则删除并回退到默认目录。非沙盒构建下同样执行，无害。
3. **构建与上传**（`release.yml` 的 `app-mas` 任务，secrets 齐全才跑）：Tauri 只出未签名的通用 `.app`；然后 PlistBuddy 写入唯一的 `CFBundleVersion`（GitHub 运行序号），拷入 `embedded.provisionprofile`，用 `Apple Distribution` 证书 + `mas.plist` 签名（不开 hardened runtime），`productbuild` 用 `Mac Installer Distribution` 证书打 `.pkg`，`altool --type macos` 上传到 App Store Connect。云端签名对 Mac 应用不可用（我们不经 Xcode 构建），所以证书与描述文件由用户在 Apple 后台创建并以 secrets 提供。
4. **不做**：iCloud、推送、Sparkle 自动更新（商店版由商店更新；`.dmg` 版的自动更新留给里程碑 7）。

## 备选方案

- 只上架商店、放弃 `.dmg`：GitHub 直接下载的用户会失去与命令行共享配置的能力，且商店审核周期长；两者并存成本可控。
- 用 Xcode 工程构建 Mac 版以复用云端签名：Tauri 的桌面构建不走 Xcode，改造得不偿失。

## 后果

- 需要用户：Apple Distribution 与 Mac Installer Distribution 证书（`.p12`）、Mac App Store 描述文件、在 App Store Connect 给现有 App 记录添加 macOS 平台。
- 全局快捷键（Carbon 热键）、托盘、剪贴板在沙盒内均可用；组播需要 `network.server`。
