# Lan-Send 商店提交前检查清单（iOS App Store / Mac App Store / TestFlight）

适用范围：`com.wangsheng.lansend`，Team `VVB976RN4W`。每次提交（含重新提交）逐条过一遍；带 ★ 的是已经被拒过一次的项。审核备注是**按版本**保存的，新版本要重新粘贴。

## 1. 提交前必查

### 1.1 构建与代码门禁

- **CI 四件套通过**
  - 检查：`source ~/.cargo/env && cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && cargo deny check`
  - 修：任一失败不提交。
- **版本号与变更日志一致**
  - 检查：`grep '"version"' apps/app/src-tauri/tauri.conf.json`，`grep -n '^## \[' CHANGELOG.md | head -3`，`git tag | tail -1`。
  - 修：`chore(release): X.Y.Z` 提交后再打 `vX.Y.Z` 标签触发 `.github/workflows/release.yml`；只改网页或审核备注时**不需要**新构建。
- **上传后等处理完再关联构建**
  - 检查：TestFlight 页该构建出现"准备提交"，且 ASC 邮件里没有 `ITMS-9xxxx` 警告（重点看 `ITMS-91053 Missing API declaration`）。
  - 修：见 1.4 PrivacyInfo。

### 1.2 Mac App Store 沙盒（★ 2.4.5 两次中招）

- **★ 沙盒探针必须 OK**
  - 检查：`sh scripts/mas-sandbox-check.sh`，期望输出 `Receive dir: /Users/<你>/Downloads` 和 `OK`。
  - 修：脚本只比对字符串，把它改成测行为：在打印的接收目录里写探针文件，断言 `realpath` 落在 `$(dscl . -read /Users/$USER NFSHomeDirectory | awk '{print $2}')/Downloads` 下，然后删除。
- **★ 真实沙盒 bundle 里完整收一次文件**
  - 检查：用 CI 产物或本地 `codesign --force --sign - --entitlements apps/app/src-tauri/entitlements/mas.plist`（先用 PlistBuddy 删掉 `com.apple.application-identifier` 和 `com.apple.developer.team-identifier`）签一个 `Lan-Send.app`，从 Finder 双击启动；另一台设备（Lan-Send 或官方 LocalSend）发一个文件；确认文件出现在 `~/Downloads`，设置页"接收目录"占位符显示 `/Users/<你>/Downloads`，历史页"打开"和"在文件夹中显示"都能弹出预览 / Finder。
  - 同时开着 `log stream --style compact --predicate 'eventMessage CONTAINS "Sandbox: lan-send-app"'` 看有没有 `deny`。
  - 修：任何 `deny` 或 `Operation not permitted` 都要在提交前解决；"打开"失败则把 `/usr/bin/open` 子进程换成 `NSWorkspace.openURL`（`apps/app/src-tauri/src/platform/macos.rs`）。
- **★ Downloads 的 TCC 弹窗必须有说明文案、拒绝后有出路**
  - 检查：`tccutil reset SystemPolicyDownloadsFolder com.wangsheng.lansend` 后从 Finder 双击启动沙盒 bundle，接收第一个文件时应弹"想要访问下载文件夹"，且弹窗里有一行用途说明。`plutil -p apps/app/src-tauri/Info.plist | grep NSDownloadsFolderUsageDescription` 必须有值。
  - 修：`apps/app/src-tauri/Info.plist` 加 `NSDownloadsFolderUsageDescription`（如 `Files other devices send you are saved to your Downloads folder.`）；`crates/core/src/runtime/incoming.rs` 里 `Destination::new` 返回 EPERM 时，把原始错误替换成指向"系统设置 › 隐私与安全性 › 文件和文件夹"的提示，并建议改用设置页"选择文件夹"（powerbox 选的目录不会再弹窗）。
  - 注意：在 Terminal 里启动会继承 Terminal 的完全磁盘访问，看不到弹窗，验证无效。
- **权限清单与描述文件一致**
  - 检查：`pkgutil --expand-full Lan-Send.pkg /tmp/pkg && codesign -d --entitlements :- /tmp/pkg/Payload/Lan-Send.app`，逐 key 对比 `apps/app/src-tauri/entitlements/mas.plist`（多出的 `beta-reports-active` 是描述文件注入的，正常）；`TeamIdentifier=VVB976RN4W`。
  - 修：证书续期后 `mas.plist` 里的 `application-identifier` / `team-identifier` 必须和新描述文件一致（`docs/release.md` 证书一节）。
- **每个权限都有"哪里用、怎么验、为什么静态扫描看不见"三句话**
  - 检查：第 2 节表格里的说明已粘贴到 ASC 版本页"App 沙盒信息"和"App 审核信息 › 备注"。
  - 修：缺一条补一条。这是 Rust 应用的固定动作，不要等被拒。
- **默认目录回退不能静默落回容器**
  - 检查：`crates/core/src/store/paths.rs` `download_dir()` 在 `real_home_dir()` 返回 `None` 时是否还会走 `UserDirs`。
  - 修：macOS 上 `tracing::warn!` 并返回 `None`（让 `accept_incoming` 明确报"未配置接收目录"），不要悄悄写容器路径。

### 1.3 审核员十分钟内会碰到的行为（macOS）

- **关窗后点 Dock 图标必须能回来**
  - 检查：关闭主窗口（默认 `close_to_tray = true`），点 Dock 图标；当前什么都不发生。
  - 修：`apps/app/src-tauri/src/lib.rs` 把 `.run(ctx)` 改成 `.build(ctx)?.run(|app, ev| if let tauri::RunEvent::Reopen{..} = ev { platform::desktop::show_main(app) })`（macOS cfg）；考虑首启 `close_to_tray` 默认 `false`。
- **设置页每个开关都要真的生效**
  - 检查：`grep -rn "notifications" apps/app/src-tauri/src crates/core/src/runtime` 无实现即为空开关。
  - 修：隐藏"系统通知"开关，或接 `tauri-plugin-notification` 在传输完成 / 收到剪贴板时发通知。
- **全局快捷键默认值不能劫持系统常用键**
  - 检查：`crates/core/src/store/settings.rs` 默认 `CmdOrCtrl+Shift+V`，会吞掉其他 App 的"粘贴为纯文本"。
  - 修：默认留空，由用户在设置页开启；捕获 `FailedToWatchMediaKeyEvent` 并提示沙盒版拿不到输入监控。
- **未配对时不要读剪贴板（macOS 26 会弹粘贴隐私提示）**
  - 检查：在 macOS 26 上开着 Lan-Send 在 Safari 复制一段文字，看是否弹"Lan-Send 想要粘贴"。
  - 修：`crates/core/src/runtime/clipboard.rs` 只有 `clipboard_peers()` 非空时才启动 watcher / `backend.read()`。
- **拖拽到设备行要能发送**
  - 检查：沙盒 bundle 里从桌面拖文件到设备行，不能出现 `Permission denied`。
  - 修：失败则沙盒版隐藏拖放提示，只保留文件选择器（wry 读的是 `NSFilenamesPboardType`）。
- **历史页"发送"记录重启后按钮别报原始错误**
  - 检查：沙盒 bundle 里发一个文件，重启，历史页点该行"打开"/"在文件夹中显示"。
  - 修：`PlatformInfo` 加 `sandboxed: std::env::var_os("APP_SANDBOX_CONTAINER_ID").is_some()`，`History.tsx` 对 `direction === 'send'` 且 sandboxed 隐藏这两个按钮。

### 1.4 Info.plist / 隐私清单

- **出口合规**：`plutil -p apps/app/src-tauri/Info.plist apps/app/src-tauri/Info.ios.plist | grep ITSAppUsesNonExemptEncryption` 均为 `false`；ASC 就不会再问加密问题。
- **用途说明**：macOS `NSLocalNetworkUsageDescription`、`NSDownloadsFolderUsageDescription`；iOS `NSLocalNetworkUsageDescription`、`NSPhotoLibraryUsageDescription`。缺一个补一个。
- **版权**：`grep -n copyright apps/app/src-tauri/tauri.conf.json` 无结果则在 `bundle` 下加 `NSHumanReadableCopyright` = `© 2026 Wang Sheng`（已加在 `apps/app/src-tauri/Info.plist` 与 `Info.ios.plist`），和 ASC 的版权字段一字不差。
- **类别**：`bundle.category = "Utility"` → `LSApplicationCategoryType = public.app-category.utilities`；ASC 两个平台主类别都选"工具"。
- **PrivacyInfo.xcprivacy**：iOS 已有 `apps/app/src-tauri/gen/apple/PrivacyInfo.xcprivacy`（`project.yml` 以 resources 阶段打进 bundle 根目录；`NSPrivacyTracking=false`、无收集数据、`FileTimestamp` 理由 `C617.1` / `3B52.1`）。检查：模拟器构建后 `ls gen/apple/build/arm64-sim/Lan-Send.app/PrivacyInfo.xcprivacy` 存在；上传邮件里没有 `ITMS-91053`。若日后用到磁盘空间 / 启动时间 API，补 `DiskSpace E174.1`、`SystemBootTime 35F9.1`。macOS 暂不强制。改 `project.yml` 后要 `xcodegen generate`，且 `Sources` / `Externals` 已加 `excludes`（否则本地 `libapp.a` 会被打成资源）。

### 1.5 提交前顺手整理（不阻塞审核，但每次看到就修）

- 书签写入时机：`cmd_app_pick_folder` 里的 `on_receive_dir_chosen` 移到 `cmd_app_settings_update`（取消保存不留书签）。
- `platform::restore_receive_dir()` 放到 `Runtime::start` 之前，否则 `expire_partials()` 删不掉自选目录里的 `.lan-send.part`。
- `Destination::new` 在 macOS 上不要 `canonicalize` 根目录（用户把 `~/Downloads` 软链到别的卷会跑出授权范围）。
- 沙盒版发送侧剪贴板文件用 `readObjectsForClasses:[NSURL]` 读，不用 `stringForType`。

## 2. 沙盒权限逐条对照表

`apps/app/src-tauri/entitlements/mas.plist`；只在 `release.yml` 的 MAS 任务签名时使用，`.dmg` 版 `tauri.macos.conf.json` 里 `entitlements: null`。审核员看不见 Rust 的 BSD socket / POSIX 文件调用，所以每一条都要在"App 沙盒信息"里写一句、在"备注"里写验证方法。

| 权限 | 代码里对应功能 | 审核时怎么说明（英文粘贴到 App 沙盒信息 / 备注） |
|---|---|---|
| `com.apple.security.app-sandbox` | MAS 必需；ADR-0014；单一 Mach-O，无嵌套二进制 | 无需说明 |
| `com.apple.security.network.server` ★ | `crates/core/src/transport/server/mod.rs` `TcpListener::bind(0.0.0.0:53317)` + IPv6 socket；`crates/core/src/discovery/multicast.rs` `join_multicast_v4(224.0.0.167)` / v6；`discovery/mod.rs` `recv_from` | `Runs an HTTPS server on TCP 53317 (LocalSend v2 prepare-upload/upload) and joins UDP multicast 224.0.0.167:53317 to answer discovery. Implemented with BSD sockets from Rust (std::net/tokio), not Network.framework, so static analysis does not detect it. Verify: install Lan-Send or LocalSend on a second device and send a file, or run 'nc -vz <mac-ip> 53317'.` |
| `com.apple.security.network.client` | `crates/core/src/transport/client.rs` `reqwest` (rustls, 指纹固定)；`discovery/mod.rs` `send_to` 组播公告与 `register` | `Outbound HTTPS to other devices on the LAN for registration and file upload; UDP announce on 224.0.0.167:53317.` |
| `com.apple.security.files.downloads.read-write` ★ | 默认接收目录 `crates/core/src/store/paths.rs` `download_dir()` → `store/platform/macos.rs` `getpwuid_r` 真实主目录 `/Downloads`；写入 `transport/server/save.rs`（`.lan-send.part` + rename）、`transfer/incoming.rs` `create_dir_all`；设置页占位符与 `lan-send identity` 显示该路径 | `Received files are saved to the user's ~/Downloads by default without an open/save panel, using POSIX file APIs from Rust (invisible to static analysis). The first receive triggers the standard Downloads-folder consent prompt; please allow it. Verify: Settings › Receive folder shows /Users/<user>/Downloads; send a file from another device and it appears there; History › Show in folder opens it in Finder.` 已排除的误判：容器 `Data/Downloads` 本身就是指向真实 `~/Downloads` 的符号链接，旧构建也在写真实目录，被拒是静态扫描而非目录错误，所以**说明 + 录屏**才是关键，代码修复只是让路径显式可读。 |
| `com.apple.security.files.user-selected.read-write` | `apps/app/src-tauri/src/commands/app.rs` `cmd_app_pick_files` / `cmd_app_pick_folder`（tauri-plugin-dialog → rfd → `NSOpenPanel`）；读所选文件 `transport/client.rs`；写所选接收目录 `save.rs` | `NSOpenPanel is used to pick files/folders to send (read) and to choose a custom receive folder (write).` 静态扫描能看到 `NSOpenPanel`，一般不会被质疑。 |
| `com.apple.security.files.bookmarks.app-scope` | `apps/app/src-tauri/src/platform/macos.rs` `bookmarkDataWithOptions(WithSecurityScope)` 存 `<config>/receive-dir.bookmark`，启动时 `URLByResolvingBookmarkData` + `startAccessingSecurityScopedResource` | `An app-scoped security-scoped bookmark keeps access to the receive folder the user chose across launches.` |
| `com.apple.application-identifier` = `VVB976RN4W.com.wangsheng.lansend`，`com.apple.developer.team-identifier` = `VVB976RN4W` | 与 `tauri.conf.json` `identifier` 及描述文件绑定；缺了 altool 会警告不能上 macOS TestFlight | 无需说明；证书续期时同步。 |

**不要申请的权限**（代码里没有对应调用，加了必被 2.4.5 拒）：`device.camera`、`device.microphone`、`personal-information.photos-library`、`automation.apple-events`（打开文件用 `/usr/bin/open` 子进程、Finder 显示用 `NSWorkspace`，都不需要）、`inherit`（无 helper）、`print`、任何 `temporary-exception`。全局快捷键走 Carbon `RegisterEventHotKey`，托盘、`NSPasteboard` 都不需要权限。

**iOS 对应项**：无 App Sandbox 权限文件；组播权限 `com.apple.developer.networking.multicast` 未获批，审核网络禁组播时发现不到设备，备注里必须写"按地址发送"（输入对方 IP）作为备选路径。

## 3. 元数据与公开页面

- **App 名称**：ASC 两个平台"App 名称"字段都必须是 `Lan-Send`，与 `CFBundleDisplayName` 一致（2.3）。仓库里统一：`grep -rn "lan-send" docs/_config.yml docs/index.md docs/privacy.md README.md apps/app/src/pages/Settings.tsx apps/app/src/i18n.ts`，用户可见处全部改为 `Lan-Send`；`lan-send` 只保留给 CLI 二进制、crate 名、仓库 slug。
- **描述**：加一句 `Not affiliated with the LocalSend project; implements the open LocalSend protocol.`，预防 4.1 / 5.2.1 追问。
- **类别**：两个平台主类别"工具"。
- **版权**：ASC 版本页 › 版权 = `© 2026 Wang Sheng`，与 plist 的 `NSHumanReadableCopyright` 一字不差。
- **★ 技术支持 URL** `https://tgolmsk.github.io/lan-send/support`（源 `docs/support.md`）
  - 检查：`curl -sL https://tgolmsk.github.io/lan-send/support | grep -c 'mailto:'` ≥ 1；页面顶部就是不需注册的联系方式（邮箱 + 回复时限），GitHub issues 只作补充；英文段放前面或顶部加 `[English below]` 锚点；`<title>` 用 `Lan-Send`。
  - 修：改 `docs/support.md` 推送，GitHub Pages 几分钟生效，无需新构建；提交前再 `curl -sI` 看 `last-modified` 已更新。
- **隐私政策 URL** `https://tgolmsk.github.io/lan-send/privacy`（源 `docs/privacy.md`）
  - 检查：`curl -sL … | grep -c mailto:` ≥ 1；内容覆盖：不收集数据、无服务器、TLS、本地存储；局域网发现会向同网段设备广播设备名 / 型号 / 证书指纹；iOS 照片选择器只复制所选项目（PHPicker，无相册权限）；标题 `Lan-Send`；"最后更新"日期为当次修改日。
  - 修：缺项补齐，保持"不收集数据"措辞与 ASC App 隐私一致。
- **App 隐私**：选"不收集数据"。
- **年龄分级**：2026 版问卷，无社交 / UGC / 付费，结果 4+。
- **出口合规**：靠 plist 声明，不走手动申报（手动会追问法国申报文件）。
- **截图**（`apps/app/scripts/store-screenshots.mjs` 从 mock 前端生成，`apps/app/store/screenshots/{iphone-6.5,ipad-13,mac}`）
  - 检查：iPhone 1284×2778（6.5" 槽位，ASC 接受代替 6.9"）、iPad 2064×2752、Mac 2880×1800；内容必须是真机会出现的画面——iPhone 设置页不能出现 `/Users/me/Downloads`（iOS 真实目录是 Documents）；截图语言与 ASC 主要本地化一致。
  - 修：改 mock 数据后重跑脚本；主要本地化是英文就加 `locale: 'en-US'` 生成一套英文；`docs/store-listing.md` 第 16 行把"6.9 英寸必传"改为"6.9 或 6.5"。
- **App 审核信息**
  - "需要登录"取消勾选（否则"添加以供审核"报用户名密码必填）。
  - 联系人电话 / 邮箱填真实可接听的。
  - 备注 ≤ 4000 字符，内容见第 4 节模板；每个新版本重新粘贴。
  - 附件：真机屏幕录像（见第 4 节）。
- **macOS 版本页"App 沙盒信息"**：按第 2 节表格每个权限一句。
- **中国大陆**：无 ICP 备案就在"定价与销售范围"取消勾选中国大陆。
- **文档同步**：提交前把 `docs/store-listing.md` 第一节表格里的"待填 / 待做"改成实际填写内容，三点五补上最新一次被拒记录。

## 4. 新账号首次提交要准备的材料（★ 2.1 Information Needed）

新账号审核记录少，苹果会要六项资料，要求**同时**写进备注和回复线程。提前准备好，首提就附上，省一轮。

1. **真机屏幕录像**（模拟器不算）：从启动 App 开始，覆盖发现设备 → 发送文件 → 接收文件 → 历史页打开文件 → 设置页；macOS 版录到文件出现在 `~/Downloads` 和 Finder 显示；录一段"按地址发送"输入 IP 的路径；无注册 / 登录 / 注销、无 UGC、无付费，在备注里写明。文件 `.mov` / `.mp4`，作为附件放在 App 审核信息。
2. **用途与目标用户**：局域网内设备间直接互传文件与剪贴板，无云端，面向同一 Wi‑Fi 下有多台设备的用户。
3. **使用与访问说明**：无账号无凭据；验证步骤 = 第二台设备装 Lan-Send 或 LocalSend（同协议）→ 两台同网 → 设备列表出现对方 → 选文件发送 → 对方接受；审核网络禁组播时用"按地址发送"。
4. **依赖的外部服务**：无（无数据源、无认证、无支付、无 AI）。
5. **地区差异**：无，仅界面语言跟随系统（中 / 英）。
6. **监管与第三方素材**：不属于强监管行业；LocalSend 协议为开放规范，独立实现，未打包其代码或素材。

备注模板（英文，控制在 4000 字符内，逐段可删）：

```
Purpose: peer-to-peer file and clipboard transfer between devices on the same LAN. No account, no server, no data collection.
How to test: install Lan-Send (or LocalSend, same open protocol) on a second device on the same Wi-Fi; it appears in Devices; pick a file and send; accept on the receiver. If multicast is blocked on your network, use "Send by address" and enter the other device's IP (port 53317).
Entitlements (macOS): <第 2 节 network.server / network.client / files.downloads.read-write / files.user-selected / bookmarks 五段>
Prompts you will see: Local Network (first launch); Downloads folder access (first receive, macOS) — please Allow.
Not affiliated with the LocalSend project. Support: <邮箱>. Screen recording attached.
```

## 5. 被拒后的处理流程

**第一步：分类**（读拒信原文，对号入座）

| 类型 | 特征 | 是否需要新构建 |
|---|---|---|
| A. 权限静态扫描误判 | 2.4.5 / 2.4.5(i)，"entitlement … no matching functionality"，且代码确实在用（★ network.server、★ files.downloads.read-write 均属此类） | 否 |
| B. 元数据 / 公开页面 | 1.5 支持网址、2.3 截图 / 名称、5.1.1 隐私文案 | 否（改网页或 ASC 字段） |
| C. 资料补交 | 2.1 Information Needed | 否 |
| D. 真实缺陷 | 崩溃、功能不可用、权限确实没用到 | 是 |

**第二步：准备内容**

- A：第 2 节该权限那格的英文说明 + 一段录屏（macOS 录到文件落进 `~/Downloads` / 端口被连接）。同时在代码里给扫描器留下可见证据可选（如 macOS 上先用 `NSFileManager URLsForDirectory:NSDownloadsDirectory` 再回退 `getpwuid_r`），但不要指望它替代说明。
- B：改 `docs/support.md` / `docs/privacy.md` / 截图 / ASC 字段，`curl -sL` 验证线上已更新后再回复。
- C：第 4 节六项 + 录屏。
- D：改代码 → 1.1–1.4 全部重跑 → 提交 `CHANGELOG.md` 条目 → 打新 `v*` 标签走 `release.yml` → 等 ASC 处理完成。

**第三步：ASC 精确按钮顺序**（App Store Connect › 我的 App › Lan-Send › 左侧选该平台被拒版本）

1. 版本页顶部"被拒"提示 → 打开 App 审核消息线程，点 **回复 App 审核**，粘贴英文说明；A / C 类在这里附录屏。
2. 同一版本页 → **App 审核信息 › 备注**：把同样的说明合并进去（≤ 4000 字符，超长会报"此栏过长"）；macOS 再补 **App 沙盒信息**。
3. D 类：版本页构建行点 **删除** → 等新构建在 TestFlight 页显示"准备提交" → **添加构建版本** 选新构建。A / B / C 类构建不动。
4. 右上角 **存储**。
5. 右上角 **更新审核** → 提交项目状态变为"可供审核"。
6. 进入提交页（左侧"App 审核" / 提交列表）→ **重新提交至 App 审核**。
7. 不要新建版本号、不要撤回后重建提交；同一版本可以反复重新提交。

**第四步：留档**

- `docs/store-listing.md` 三点五追加一条：日期、平台、构建号、条款号、真实根因、修法、教训。根因要写对（例如 build 19 是静态扫描误判而非目录写错），否则下次会按错的方向修。
- `CHANGELOG.md` 与 `docs/platforms.md` / `docs/release.md` 中相关描述同步更正。
- 把这次新增的"提交前必查"项补进本清单。
