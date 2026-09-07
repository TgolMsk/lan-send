# tests/interop — 与官方 LocalSend 的互操作测试

目标：证明本项目与官方客户端能互相发现、互相收发。`run.py` 在一台机器上跑两个方向：

| 方向 | 官方侧 | 本项目侧 |
|---|---|---|
| A | `localsend-cli -f <file>`：TUI 列出发现的设备，按 Enter 发送 | `lan-send receive --auto-accept` |
| B | `localsend-cli`：预先把我们的指纹写入 `paired-v2.json`，已配对设备自动接受 | `lan-send send official-b <file>` |

两侧各用独立的配置目录（`--config-dir` / `XDG_CONFIG_HOME`）与 HTTP 端口（53400 / 53401），组播端口 53317 共用。
官方 1.18.2 发布版的 CLI 只有 TUI（无 `send --to` 子命令），所以两个方向都用 `pexpect` 驱动其终端。

## 本地运行

```bash
cargo build -p lan-send-cli
python3 -m venv tests/interop/.venv
tests/interop/.venv/bin/pip install pexpect
tests/interop/.venv/bin/python tests/interop/run.py        # 自动下载官方 CLI 到 .cache/
tests/interop/.venv/bin/python tests/interop/run.py --only a --keep   # 只跑一个方向并保留临时目录
```

CI 的 `interop` 任务在 Linux 与 macOS 运行器上执行同样的脚本。
