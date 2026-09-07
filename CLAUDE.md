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
```

本机的 Rust 由 rustup 安装在 `~/.cargo/bin`，未写入 PATH；在命令前 `source ~/.cargo/env`。
若 Xcode 已安装但许可证未接受，链接会失败，可临时 `export DEVELOPER_DIR=/Library/Developer/CommandLineTools`。

互操作测试：`python3 -m venv tests/interop/.venv && tests/interop/.venv/bin/pip install pexpect pyte`，
然后 `tests/interop/.venv/bin/python tests/interop/run.py`（自动下载官方 CLI 到 `tests/interop/.cache`）。

## 里程碑状态

1. `core::protocol` + `discovery` + `transport`，CLI 与官方 LocalSend 互传单文件 —— **完成**（2026-09-07，双向互测通过；IPv6、文件夹、断点续传留给里程碑 2）。
2. 多文件、文件夹、校验、断点续传、历史。
3. 剪贴板同步。
4. 媒体层。
5. Tauri 骨架与 IPC。
6. 前端（等 UI 方案，参考 `docs/ui-style-reference.md`）。
7. 可选项。
