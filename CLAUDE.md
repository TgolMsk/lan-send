# CLAUDE.md

本仓库的完整需求与硬性规则在 `docs/dev-brief.md`（原始简报，逐字保留，顶部有修订记录）。先读它，再读本文件。

## 目标与平台

- LocalSend v2.2 兼容的局域网文件 + 剪贴板互传工具，MIT 许可证，公开仓库。
- 目标平台：**macOS、Windows、iOS**。Linux 只在 CI 上跑核心库测试与互操作测试。
- 官方 LocalSend 1.18+ 的真实行为（尤其是强制 mTLS、指纹大写 hex）见 `docs/localsend-v2-interface-checklist.md`；文档与实现冲突时以实现为准，差异记录在 `docs/protocol-extensions.md`。

## 目录规范（ADR-0001）

- `crates/core`：纯 Rust 核心，禁止 GUI 依赖；平台代码只能放在各模块的 `platform/{macos,windows,ios}.rs`。
- `crates/cli`：二进制名 `lan-send`，macOS/Windows。
- `apps/app`：Tauri 2，一个工程覆盖三端，平台差异用 `tauri.{macos,windows,ios}.conf.json` 与 `src-tauri/src/platform/`。
- `docs/adr`：每个模块先写 ADR 再写代码，不超过一页，用 `0000-template.md`。
- `docs/release.md`：打 `v*` 标签发布安装包（macOS `.pkg`、Windows `.msi`）的流程与签名 secrets；WiX 定义在 `crates/cli/wix/main.wxs`。
- 新增顶层目录先补 ADR。

## 硬性规则（摘要，全文见简报）

- tokio + axum + reqwest(rustls)；库内 `thiserror`，应用层 `anyhow`。
- 非测试代码禁止 `unwrap()`（clippy 强制），`expect()` 需注释不变量。
- 依赖必须 MIT 兼容，`cargo deny check` 在 CI 强制；引入 GPL 类依赖前必须停下来问。
- 不确定的产品决策（默认值、限制、交互）先问，不自行拍板。
- 每个里程碑结束时停下汇报：做了什么、没做什么、下一步、需要决策的问题。
- 每个 PR/提交带 `CHANGELOG.md` 条目。

## 常用命令

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check            # 需要 cargo-deny
cargo run -p lan-send-cli -- --help
cd apps/app && pnpm install && pnpm build     # 前端（Vite + React）
cargo tauri dev                                 # 在 apps/app 下：桌面应用开发模式
cargo tauri build                               # 打包 .app/.dmg 或 .msi/.exe
```

Linux 上 `--workspace` 要加 `--exclude lan-send-app`（Tauri 需要 GTK）。`Runtime` 在 `crates/core/src/runtime/`，Tauri 命令在 `apps/app/src-tauri/src/commands/`，前端类型镜像在 `apps/app/src/types.ts`。

本机的 Rust 由 rustup 安装在 `~/.cargo/bin`，未写入 PATH；在命令前 `source ~/.cargo/env`。
若 Xcode 已安装但许可证未接受，链接会失败，可临时 `export DEVELOPER_DIR=/Library/Developer/CommandLineTools`。

互操作测试：`python3 -m venv tests/interop/.venv && tests/interop/.venv/bin/pip install pexpect pyte`，
然后 `tests/interop/.venv/bin/python tests/interop/run.py`（自动下载官方 CLI 到 `tests/interop/.cache`）。

## 里程碑状态

1. `core::protocol` + `discovery` + `transport`，CLI 与官方 LocalSend 互传单文件 —— **完成**（2026-09-07，双向互测通过；IPv6、文件夹、断点续传留给里程碑 2）。
2. 多文件、文件夹、校验、断点续传、历史 —— **完成**（2026-09-07：持久化、文件夹、分目录、历史与设备命令、断点续传、IPv6）。
3. 剪贴板同步 —— **完成**（2026-09-07：配对、文本 / 图片 / 文件列表同步，macOS 与 Windows 后端，历史；iOS 不做）。
4. 媒体层 —— **完成**（2026-09-08：`core::media` MIME 探测、系统解码器 + `image` 回退的缩略图、EXIF 方向、200 MB LRU 缓存、`lofty` 音频元数据；历史 / 传输页缩略图；ADR-0015。波形与 HEIC → JPEG 转换留在里程碑 7）。
5. Tauri 骨架与 IPC —— **完成**（2026-09-07：`core::runtime` 事件式运行时、`apps/app` Tauri 壳、托盘与快捷键、类型化 IPC；ADR-0013）。
6. 前端 —— **完成**（2026-09-07：五个页面与全部弹窗，桌面侧栏 / 移动端底部标签栏，中英文，深浅色；参考 `docs/ui-style-reference.md`）。
7. 可选项。
