# 0001. 仓库布局与目标平台

- 状态：已接受
- 日期：2026-09-06

## 背景

产品要在 macOS、Windows、iOS 三端交付，同时保持 Rust 核心库不依赖任何 GUI。
三端共享一套核心逻辑和一套 UI，但平台差异（剪贴板、组播权限、后台限制）
必须有固定的落点，否则 `cfg` 会散落到业务代码里。

## 决策

单一 Cargo 工作区，目录职责固定如下：

```
lan-send/
├── crates/core/          # lan-send-core：协议、发现、传输、剪贴板、媒体、持久化；无 GUI 依赖
│   └── src/<模块>/platform/{macos,windows,ios}.rs   # 平台实现，按 target_os 编译
├── crates/cli/           # lan-send（二进制名）：macOS/Windows 命令行；不发布到 iOS
├── apps/app/             # Tauri 2 应用，一个工程覆盖三端
│   ├── src/              # 共享 Web 前端（框架待 UI 方案定）
│   └── src-tauri/
│       ├── src/          # IPC 命令 cmd_<模块>_<动作>、事件推送；platform/{macos,windows,ios}.rs
│       ├── tauri.conf.json + tauri.{macos,windows,ios}.conf.json   # 每端只放差异配置
│       └── gen/apple/    # `tauri ios init` 生成的 Xcode 工程，纳入版本控制
├── docs/                 # 简报、接口清单、扩展文档、ADR、平台约束、UI 参考
├── tests/interop/        # 与官方 LocalSend 的互操作测试脚本
└── .github/workflows/    # 三端 CI、Windows/macOS CLI 产物、发布、许可证检查
```

规则：
1. 平台相关代码只允许出现在名为 `platform/` 的子目录中；业务模块通过 trait 使用它。
2. iOS 与桌面共用 `apps/app`，不单独建 `apps/ios`；每端差异用 Tauri 的平台配置文件表达。
3. 新增顶层目录必须先补 ADR。

## 备选方案

- iOS 用原生 SwiftUI + UniFFI 绑定核心库：UI 要写两遍，与"UI 方案统一提供"的前提冲突；暂不采用，若 Tauri iOS 的 WebView 性能或系统集成不达标再重新评估。
- 桌面与 iOS 各建一个 Tauri 工程：配置与前端会分叉，放弃。

## 后果

- 正面：一处前端、一处核心、平台差异位置固定，CI 可以按目录做检查。
- 负面：iOS 构建依赖 Xcode 与签名证书，本机当前只有 Command Line Tools，iOS 打包先由 CI 的 macOS 运行器做 `cargo check --target aarch64-apple-ios`，正式打包待证书就绪。
