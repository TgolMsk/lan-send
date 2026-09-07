# tests/interop — 与官方 LocalSend 的互操作测试

目标：证明本项目与官方客户端能互相发现、互相收发。CI 至少在 Linux 上跑通。

计划（里程碑 1 实现）：

1. 从官方 GitHub Releases 下载对应平台的 `localsend-cli`（1.18+，Rust 实现，行为与 App 一致）并缓存到 `.cache/`。
2. 官方 → 我们：`localsend-cli send --to <我们的 alias> <file>`（官方 CLI 的无交互模式）对 `lan-send receive --dir <tmp>`；比对 SHA-256。
3. 我们 → 官方：预先把我们的指纹写入官方 CLI 的 `paired-v2.json`（已配对设备自动接受），在 pty 中启动 `localsend-cli`，执行 `lan-send send <alias> <file>`；比对 SHA-256。
4. 发现：两边各自列出设备，确认出现在对方列表且指纹一致。

脚本语言：Python 3（标准库 + `pexpect`），便于在三个 CI 运行器上复用。
