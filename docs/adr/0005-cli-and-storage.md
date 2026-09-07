# 0005. 命令行工具与本地存储

- 状态：已接受
- 日期：2026-09-07

## 背景

CLI 要在 UI 之前独立可用，并作为互操作测试与自用工具；里程碑 1 只需 `discover / send / receive`。

## 决策

1. 目录用 `directories` 的 `ProjectDirs("", "", "lan-send")`：macOS `~/Library/Application Support/lan-send/`，Windows `%APPDATA%\lan-send\`；`--config-dir` 可整体覆盖（测试用）。身份文件 `identity.pem`。
2. 默认身份：alias = 主机名（去掉 `.local`），`deviceType=headless`，`deviceModel` = 操作系统名。`--alias/--port` 与环境变量 `LAN_SEND_ALIAS/LAN_SEND_PORT` 可覆盖。
3. `send` 与 `receive` 都同时运行服务端与发现：发送时需要服务端接收对方的反向 `cancel` 与 `register`；`discover` 也开服务端，否则别人回应的 `register` 无处可去。
4. 接收目录默认系统下载目录；`--auto-accept` 用于测试脚本，否则终端里 Y/N 确认。
5. 发送目标可以是 alias、指纹前缀或 IP[:port]；IP 会被直接探测，不等组播。
6. 进度条用 `indicatif`；日志走 `tracing`，默认只有 warn，`-v` 递增。剪贴板内容永不进日志。
7. 里程碑 1 的 CLI 不做历史、配对与设置文件；这些随里程碑 2/3 的 SQLite 存储进入。

## 后果

- CLI 与 App 共用同一配置目录，后续要注意两者同时运行时端口与身份的处理（App 端在里程碑 5 处理）。
