# Changelog

格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，版本号遵循语义化版本。

## [Unreleased]

### Added
- 里程碑 2（第二部分）：断点续传扩展——`x-resume-token` / `x-resume-offsets`、带 `Range` 的上传、`GET /api/ext/v1/resume` 断点查询、会话内与跨会话恢复、上传空闲超时（30 秒）与会话闲置回收（10 分钟）；进程内端到端测试覆盖中断后续传与官方式发送方的兼容路径。
- 里程碑 2（第一部分）：`settings.json` 与 SQLite 持久化（传输历史默认 200 条、已知设备与收藏、断点记录表）；发送文件夹（递归、跳过符号链接与隐藏文件、保留相对路径、发送前汇总）；接收目录按设备 / 日期 / 类型分子目录，同名策略 rename / overwrite / ask；CLI 新增 `history`、`devices`，`send` 与 `receive` 读取设置并写入历史；发现阶段探测收藏与最近 7 天见过的设备地址。
- 里程碑 1：`lan-send-core` 的 `protocol`（v2.2 DTO、`x-lanext` 扩展字段、指纹）、`transport`（RSA-2048 自签名身份、强制/可选客户端证书的 rustls 策略、reqwest 客户端、axum 上传 API 服务端、流式落盘与 SHA-256/字节数校验、文件名净化）、`discovery`（每接口一个组播 socket、公告脉冲、HTTP register 回应、已知地址探测、/24 子网扫描回退）、`store`（应用目录、私钥文件权限）。
- CLI：`lan-send discover / send / receive / identity`，PIN 交互、进度条、`--auto-accept`、`--config-dir`、`--client-certs`。
- 互操作测试 `tests/interop/run.py`：与官方 `localsend-cli` 1.18.2 双向收发（pexpect 驱动官方 TUI），CI 在 Linux 与 macOS 上运行。
- 仓库骨架：Cargo 工作区、`lan-send-core` 与 `lan-send-cli` 空壳、命令行参数定义。
- 文档：开发简报、LocalSend v2.2 接口核对清单、协议差异与扩展、三端平台约束、UI 视觉参考、ADR-0001 仓库布局。
- CI：macOS / Windows / Linux 格式与 clippy 与测试，iOS 交叉检查，Windows 与 macOS CLI 产物，cargo-deny 许可证检查，标签发布。
