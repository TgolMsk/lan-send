# apps/app — Tauri 2 应用（macOS / Windows / iOS）

一个工程覆盖三端（ADR-0001、ADR-0013）。

```
apps/app/
├── package.json, vite.config.ts, tsconfig.json, index.html
├── src/                      # 共享 Web 前端（React 19 + TypeScript）
│   ├── ipc.ts                # 类型化命令与事件封装；非 Tauri 环境自动切到 mock.ts
│   ├── types.ts              # Rust 视图与设置的 TypeScript 镜像
│   ├── mock.ts               # 浏览器开发用假数据
│   └── styles/tokens.css     # docs/ui-style-reference.md 的色板与卡片语言
└── src-tauri/
    ├── Cargo.toml            # crate lan-send-app，依赖 crates/core
    ├── src/lib.rs            # 组装 Tauri、注册命令、事件泵；main.rs 是桌面入口
    ├── src/state.rs          # AppState：核心 Runtime + event:<name> 转发
    ├── src/commands/         # cmd_<模块>_<动作>：app / devices / transfer / pair / history / clipboard
    ├── src/platform/         # desktop.rs（托盘、快捷键、关窗到托盘）、macos.rs、windows.rs、ios.rs
    ├── tauri.conf.json       # 公共配置；tauri.{macos,windows,ios}.conf.json 只放差异
    ├── capabilities/         # 主窗口权限
    ├── icons/                # 由 icons/source.svg 经 `cargo tauri icon` 生成
    └── gen/apple/            # `cargo tauri ios init` 生成的 Xcode 工程
```

## 开发

```bash
cd apps/app
pnpm install
cargo tauri dev            # 桌面
cargo tauri ios dev        # iOS 模拟器（需要 Xcode 与 CocoaPods）
pnpm dev                   # 只跑前端，浏览器里用假数据
```

## 约定

- 命令名 `cmd_<模块>_<动作>`，事件名 `event:<kebab-case>`，负载即 Rust 端 `RuntimeEvent` 的变体（带 `type` 字段）。
- 业务逻辑在 `crates/core`；这里只有壳、平台集成和界面。
- 平台差异只允许出现在 `src/platform/` 与三个平台配置文件里。
