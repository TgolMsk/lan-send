# 协议：与官方实现的差异 与 私有扩展

> 第一部分记录 `protocol.md` 与官方 LocalSend（1.18.2，Rust 核心）实际行为的差异，我们**以实际行为为准**。
> 第二部分是本项目的私有扩展，原则：官方客户端忽略即可正常工作，绝不破坏兼容性。
> 完整核对过程见 `localsend-v2-interface-checklist.md`。

## 1. 文档与官方实现的差异（以实现为准）

| # | 主题 | 官方 1.18.2 实际行为 |
|---|---|---|
| 1 | TLS | 服务端强制客户端证书（mTLS）；身份 = 客户端证书指纹。提供网页分享时才降为可选。 |
| 2 | 指纹 | 证书 DER 的 SHA-256，**大写 hex**；比较时不区分大小写。 |
| 3 | register | HTTPS 下请求体 `fingerprint` 必须等于证书指纹，否则注册被静默丢弃（仍 200）。 |
| 4 | UDP 回应 | 只发公告，不用 UDP 回应；回应一律走 HTTP `POST /register`。 |
| 5 | 公告次数 | 每次 announce 发 3 个报文：+100 ms、+500 ms、+2000 ms。 |
| 6 | IPv6 | 额外的 IPv6 组播组 `ff12::fd3a:e420`；HTTP 服务同时监听 v4/v6。 |
| 7 | v1 路由 | 仍提供 `GET /api/localsend/v1/info`（≤1.17 客户端探测用）。 |
| 8 | upload 校验 | 来源 IP（含 IPv6 scope）必须等于 prepare-upload 的 IP；字节数必须等于 `size`。 |
| 9 | 校验和 | `sha256` 小写 hex；不匹配 422，同 token 最多共 3 次尝试。 |
| 10 | PIN | 同 IP 错 3 次 → 429，答对清零。PIN 校验在解析 body 之前。 |
| 11 | 错误体 | `{"message": "..."}`。 |
| 12 | cancel | 接收方会反向调用发送方服务端的 `/cancel`；pending 阶段可不带 `sessionId`；任何情况都 200。 |
| 13 | 文本消息 | 单个 `text/*` 文件 + `preview` 正文；接收方可回 204 表示已读。 |
| 14 | 时间戳 | `metadata.modified/accessed`（RFC 3339）默认发送并写回文件。 |
| 15 | HTTP 版本 | 客户端 ALPN 仅 `http/1.1`。 |
| 16 | 未知字段 | 双方都忽略未知 JSON 字段。 |
| 17 | 单会话 | 接收端同一时间只允许一个上传会话（pending 或 active），否则 409。 |
| 18 | 代理/重定向 | 客户端不走代理、不跟随重定向。 |
| 19 | 文件夹 | 官方 **App** 按 `fileName` 里的目录组件建子目录；官方 **CLI**（1.18.2）只保留最后一段，目录被压平。我们的接收端保留目录。 |

## 2. 私有扩展（草案，待确认）

所有扩展字段以 `x-` 前缀命名，放在官方会忽略的位置（`info`、`files.*`、组播报文、响应体）。

### 2.1 能力声明（提案）

在组播报文、`register` 请求/响应、`prepare-upload` 的 `info` 中附加：

```json
"x-lanext": { "v": 1, "features": ["resume", "clipboard"] }
```

只有双方都声明了某能力才使用它。字段名待确认。

### 2.2 断点续传（里程碑 2）

- `prepare-upload` 响应附加 `x-resume-token`；`upload` 支持 `Range`/`Content-Range`；对方未声明 `resume` 时整文件重传。
- 详细语义在里程碑 2 的 ADR 中定义。

### 2.3 剪贴板同步（里程碑 3）

- 端点 `POST /api/ext/v1/clipboard`，仅对已配对指纹开放，否则 403。
- 请求体为 `ClipboardItem` JSON；图片 > 512 KB 走 multipart。
- 详细语义在里程碑 3 的 ADR 中定义。
