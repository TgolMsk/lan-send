# 0004. HTTPS 传输：服务端与客户端

- 状态：已接受
- 日期：2026-09-07

## 背景

上传 API 是接收方开 HTTPS 服务、发送方做客户端；官方 1.18 的服务端强制客户端证书并按来源 IP、token、字节数、校验和逐项校验。

## 决策

1. 服务端：tokio `TcpListener` + `tokio-rustls` 手工做 TLS accept（需要拿到对端证书），每个连接交给 hyper 服务 axum `Router`；对端信息以 `Extension` 注入。客户端证书**默认强制**，可配置为可选；出示的证书必须签名自洽且在有效期内。
2. 路由：`register`、`v1/info`、`v2/info`、`prepare-upload`、`upload`、`cancel`，错误体统一 `{"message"}`。PIN 在解析 body 之前校验，同 IP 错 3 次返回 429。
3. 会话：单槽（pending 或 active），冲突 409；`prepare-upload` 是长轮询，应用层通过 oneshot 回答"接受哪些/拒绝"；发送方断开或 `cancel` 会释放槽位并通知应用层。
4. `upload`：校验 sessionId、fileId、token、来源 IP、文件状态；流式写入 `<目标>.lan-send.part`，字节数必须等于 `size`；开启校验时比对 sha256，不匹配返回 422 并保留 `.part` 供重传（同 token 最多 3 次）；成功后写回时间戳并重命名为最终文件名。
5. 落盘路径由应用层决定，但核心库负责：按官方规则净化 `fileName` 的每一段、拒绝 `..` 与绝对路径、同名追加 ` (n)`、最终路径必须位于接收目录内。
6. 客户端：reqwest + 预配置的 rustls（自定义校验器：忽略主机名，校验签名与有效期，可固定指纹）、出示客户端证书、ALPN 只允许 `http/1.1`、不走代理、不跟随重定向。发现用未固定指纹的客户端，传输一律固定到目标指纹。
7. 事件用 `tokio::sync::mpsc` 推给应用层；进度事件按阈值节流。

## 备选方案

- `axum-server`/`axum::serve` 的 TLS 集成：拿不到对端证书或不支持 mTLS 定制，放弃。
- 直接写最终文件名：校验失败时会留下坏文件，且无法重传覆盖，放弃。

## 后果

- 服务端每个连接都要克隆一次 `Router`（加 `Extension` 层），开销可忽略。
- 断点续传（里程碑 2）在此基础上加 `Range` 支持，不改会话模型。
