# apps/app — Tauri 2 应用（macOS / Windows / iOS）

里程碑 5 才初始化（`cargo tauri init` + `cargo tauri ios init`）。届时的结构：

```
apps/app/
├── package.json, src/            # 共享 Web 前端（框架随 UI 方案确定）
└── src-tauri/
    ├── Cargo.toml                # 依赖 crates/core
    ├── src/main.rs, lib.rs       # IPC 命令 cmd_<模块>_<动作>；事件 event:*
    ├── src/platform/{macos,windows,ios}.rs
    ├── tauri.conf.json           # 公共配置
    ├── tauri.macos.conf.json / tauri.windows.conf.json / tauri.ios.conf.json
    ├── icons/
    └── gen/apple/                # tauri ios init 生成的 Xcode 工程
```

在此之前这个目录只有本说明，避免目录规范被临时文件打乱。
