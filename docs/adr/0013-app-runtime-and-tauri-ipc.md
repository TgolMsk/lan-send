# 0013. 应用运行时、Tauri 壳与 IPC

- 状态：已接受
- 日期：2026-09-07

## 背景

里程碑 5 要求 Tauri 2 骨架、托盘、全局快捷键，IPC 命令"一一对应 core 的公共 API"（简报 §13）。到目前为止，把服务端、发现、传输、配对、剪贴板串起来的编排逻辑（接受请求、落盘、写历史、重试与续传、配对确认）都在 CLI 里，夹着终端提示与进度条，Tauri 无法复用。用户已给出 UI 方案：三端都是客户端，视觉参考 `docs/ui-style-reference.md`。

## 决策

1. **编排下沉到核心库**：新增 `lan_send_core::runtime`。`Runtime::start(RuntimeConfig)` 打开设置、数据库、身份，启动 HTTPS 服务、发现、（可选）剪贴板同步，并把所有需要用户决定的事情变成"事件 + 应答命令"：
   - 事件 `RuntimeEvent`（可序列化）：`DeviceFound / DeviceUpdated / DeviceLost`、`IncomingRequest / IncomingWithdrawn / IncomingConflict`、`TransferProgress / TransferFileDone / TransferCompleted / TransferNeedsPin`、`PairRequest / PairResponse / PairResult`、`ClipboardReceived / ClipboardSync`、`Error`。
   - 应答 `respond_incoming(session, accept, files)`、`respond_conflict(session, file, overwrite)`、`provide_pin(transfer, pin)`、`respond_pair_request(fingerprint, accept)`、`pair_confirm(fingerprint, matches)`；未应答的决定有超时并取保守默认（拒绝 / 重命名）。
   - 查询与操作：身份、设置（保存并热应用别名等，端口变更返回"需重启"）、设备列表（发现结果与数据库合并：在线、收藏、已配对、自定义别名）、刷新发现、发送（返回 `transfer_id`，进度走事件，可取消）、配对 / 解除、历史与剪贴板历史、剪贴板推送与同步开关。
   - 在线状态：每 30 s 对在线设备 `GET /info` 探活，连续两次失败发 `DeviceLost`（LocalSend 不做周期公告，只能探活）。
   - 进度事件每个文件最多每 100 ms 一条，完成时必发。
   - 运行时不打印、不依赖终端；剪贴板文本永不进日志。CLI 暂保留自己的编排，后续迁到 `runtime`（跟踪项）。
2. **Tauri 壳** `apps/app/src-tauri`（crate `lan-send-app`，加入工作区）只做三件事：持有 `Runtime`、把 `RuntimeEvent` 转成 Tauri 事件 `event:<kebab-case>`（简报要求的六个名字保留：`event:transfer-progress`、`event:device-found`、`event:device-lost`、`event:clipboard-received`、`event:transfer-completed`、`event:error`，其余同样前缀）、注册 `cmd_<模块>_<动作>` 命令（`cmd_devices_list`、`cmd_transfer_send`、`cmd_pair_start`……）。平台代码只在 `src/platform/{macos,windows,ios}.rs`：桌面端托盘与全局快捷键（默认 `CmdOrCtrl+Shift+V` = 推送剪贴板，可在设置改），关窗默认隐藏到托盘；iOS 无托盘、无快捷键、无剪贴板同步。
3. **前端** `apps/app/src`：Vite + React 19 + TypeScript，不用 UI 框架；样式用 CSS 变量实现参考图的色板与卡片语言，一套代码三端，iOS 走底部标签栏。非 Tauri 环境（浏览器开发）用内存假数据层，方便截图与迭代。
4. **依赖与许可证**：tauri / wry / tao / 插件均为 MIT 或 Apache-2.0。Linux 上 Tauri 会链接 GTK / WebKitGTK（LGPL），Linux 不是目标平台，`deny.toml` 把检查范围限定到 macOS / Windows / iOS 三端的目标三元组；Linux CI 对工作区检查时排除 `lan-send-app`。
5. **打包**：里程碑 6 结束后把 `tauri-action` 加进 `release.yml`：macOS `.dmg`（通用二进制），Windows `.msi` + NSIS `.exe`，iOS 走 Xcode 归档与 TestFlight（需要证书 secrets）。

## 备选方案

- 每端一个原生壳（SwiftUI / WinUI）：UI 写三遍，与"UI 方案统一"相悖。
- 编排留在各应用层：Tauri 与 CLI 各写一份接受 / 落盘 / 续传 / 配对逻辑，必然分叉。
- Tailwind / 组件库：多一层构建与许可证核对，参考图的风格靠几十个 CSS 变量就能表达。

## 后果

- `core` 多了一个有状态的模块，但仍无 GUI 依赖，CLI 可独立运行。
- 需要用户决定：关窗隐藏到托盘、剪贴板同步默认开启（仅已配对设备）、默认深色主题；先按这些默认实现，里程碑汇报时确认。
