# LocalSend v2.2 接口清单（里程碑 1 前的确认稿）

> 状态：**待确认，尚未写任何代码。**
> 依据：
> 1. `localsend/protocol` 仓库 `README.md`（v2.2，最后一次协议提交 `62bd340`，2026-08-08）。
> 2. `localsend/localsend` 仓库 `main`（对应 App/CLI 1.18.2，2026-08-21）。自 1.18.0 起官方网络层已全部改为 Rust：`packages/core`（crate 名 `localsend`，**Apache-2.0**），Flutter 只做 UI。文档与实现不一致处一律以 1.18.2 实现为准。
> 3. 官方 Rust 核心的集成测试（`packages/core/tests/*.rs`）用于佐证行为。

---

## 0. 一句话结论

协议本身很小（1 个 UDP 报文 + 6 个 HTTP 路由），但 **官方 1.18+ 的 TLS 模型与 `protocol.md` 描述不同：服务端强制要求客户端证书（mTLS），并以证书指纹作为唯一身份**。不出示客户端证书的连接在 TLS 握手阶段就被拒绝（官方测试 `test_client_without_cert_rejected`）。这是我们实现时最需要对齐的一点。

---

## 1. 常量与默认值

| 项 | 值 | 来源 |
|---|---|---|
| 协议版本字符串 | `"2.2"`（我们也上报 `2.2`） | `model/discovery.rs` `PROTOCOL_VERSION_V2` |
| HTTP 端口 | 53317（IPv4 `0.0.0.0` + IPv6 `[::]` 各绑一个监听，IPv6 设 `IPV6_V6ONLY`） | `http/server/mod.rs` |
| 组播 IPv4 | `224.0.0.167:53317`，TTL=1 | `multicast/mod.rs` |
| 组播 IPv6（官方扩展） | `ff12::fd3a:e420`，hop limit=1，按接口索引加组 | 同上 |
| 指纹 | `SHA-256(证书 DER)`，**大写十六进制**，64 字符 | `crypto/cert.rs` `fingerprint_from_cert_der` |
| 证书 | RSA-2048 自签名，`CN=LocalSend User`，无 SAN，有效期取 rcgen 默认（1975–4096，实际永不过期），序列号由公钥哈希派生 | `crypto/cert.rs` `generate_self_signed` |
| 客户端 ALPN | 强制 `http/1.1`（HTTP/2 流控会限制大文件吞吐） | `http/client/mod.rs` |
| 客户端策略 | 不走代理、不跟随重定向（1.18.2 安全修复） | 同上 |
| 错误响应体 | `{"message": "..."}`，`Content-Type: application/json` | `common/error.rs` |
| `deviceType` 取值 | `mobile / desktop / web / headless / server`，未知值回退 `desktop` | `model/discovery.rs` |
| 官方 CLI 上报 | `deviceType=headless`，`deviceModel="CLI"` | `cli/src/storage/identity.rs` |
| 发现请求超时 | 500 ms | `discovery/mod.rs` |
| 子网扫描并发 | 50 | 同上 |
| 官方发送并发 | 2 个文件同时上传 | `upload_isolate.dart` |
| 接收端 PIN 错误上限 | 每 IP 3 次后 429，答对即清零 | `common/pin.rs` |
| 单文件重传上限 | 3 次（校验和不匹配时同 token 重传） | `server/v2.rs` `MAX_UPLOAD_ATTEMPTS` |

---

## 2. TLS 与身份模型（与文档差异最大的部分）

### 2.1 服务端
- rustls，TLS 1.2/1.3。**要求客户端证书**（`client_auth_mandatory = true`）；只有在对浏览器提供"网页分享/网页接收"页面时才降为可选（浏览器没有客户端证书），但出示了就必须能通过校验。
- 对客户端证书只校验：签名自洽 + 时间有效；不看颁发者。身份 = 客户端证书指纹（大写 hex）。
- `register` 在 HTTPS 下：请求体里的 `fingerprint`（转大写后）必须等于握手证书指纹，否则**静默忽略该注册（仍返回 200）**。
- `prepare-upload` 事件同时给应用层 `info.fingerprint` 与 `cert_fingerprint`，官方 App 优先用后者。

### 2.2 客户端
- 每个请求都携带自己的证书作为客户端证书。
- 服务端证书校验：忽略主机名/SAN；校验签名与有效期；若给了期望指纹则在**握手阶段**比对（大小写不敏感），不匹配则连请求都不发。
- 两种客户端：发现用"未固定指纹"客户端（TOFU，从握手里读对方指纹）；传输/取消一律用"固定到目标指纹"的客户端。

### 2.3 对我们的影响
1. 我们的客户端**必须**出示客户端证书，否则无法与 1.18+ 官方设备通信。
2. 我们的服务端至少要"请求"客户端证书；是否"强制"是产品决策（见 §10）。
3. 组播报文里的 `fingerprint` 必须与自己证书指纹严格一致，否则对方用该指纹固定连接来 `register` 时握手失败，我们永远不会出现在对方设备列表里。

---

## 3. 设备发现

### 3.1 UDP 组播公告
- 报文（JSON，无换行，未知字段被忽略）：

```json
{
  "alias": "Nice Orange",
  "version": "2.2",
  "deviceModel": "macOS",      // 可选
  "deviceType": "desktop",     // 可选
  "fingerprint": "ABCD...64位大写hex",
  "port": 53317,
  "protocol": "https",         // "http" | "https"
  "download": false,           // 可选，默认 false
  "announce": true
}
```

- 官方 1.18：**UDP 只发公告，永不用 UDP 回应**。收到公告后用 HTTP `POST /register` 回到 `来源IP:报文port`，客户端固定到报文里的指纹。
- 公告脉冲：调用一次 announce 会发 3 次，延迟分别为 +100 ms、+500 ms、+2000 ms。App 启动、按"扫描"、恢复前台时触发。
- 官方解析器不看 `announce` 字段（Rust 结构体里没有）。老版（≤1.17）Dart 客户端可能用 `announce:false` 的 UDP 报文回应，官方 1.18 会把它当作一次普通公告并再 `register` 回去，无害。
- Socket 细节（我们需照做，否则多网卡机器收不到/发不出）：
  - 每个接口的 IPv4 地址各建一个 socket；`SO_REUSEADDR` + `SO_REUSEPORT`（非 Windows）；`bind 0.0.0.0:53317`（绑通配地址而非接口地址，否则部分平台收不到组播）；`IP_ADD_MEMBERSHIP(group, iface)`；`IP_MULTICAST_IF=iface`；loopback **开启**（同机多实例互见，自己的报文靠指纹过滤）；TTL=1。
  - IPv6：`IPV6_V6ONLY`，`bind [::]:53317`，按接口索引加组，hops=1。
- 过滤：`fingerprint == 自己` 的报文丢弃。
- 组播失败（端口被占、无网卡）不算致命：官方 `DiscoveryHandle::multicast_error()` 记录原因，其余发现手段继续工作。对应我们提示词 §3 "探测不可用时自动降级"。

### 3.2 `POST /api/localsend/v2/register`
- 请求体 `RegisterDtoV2`：`alias, version, deviceModel?, deviceType?, fingerprint, port, protocol, download?`。
- 响应 200 `RegisterResponseDtoV2`：`alias, version, deviceModel?, deviceType?, fingerprint, download?`（**没有** `port/protocol`）。
- 副作用：服务端把请求方加入自己的设备列表（HTTPS 下须通过 §2.1 的指纹一致性检查）。所以 register 是双向发现。
- 请求方在 HTTPS 下应以**握手证书指纹**作为对方身份，而不是响应体里的 `fingerprint`。

### 3.3 HTTP 扫描回退（官方 1.18 的"分级扫描"）
1. 立即：发组播公告 + 并发探测所有收藏/已配对设备的已知地址（`POST /register`）。
2. 若上述完成后 1 s 内**没有任何一次确认**（新设备或旧设备均算）→ 对每个本机接口的 `/24` 全部 255 个其他地址发 `POST /register`（HTTPS，未固定指纹，超时 500 ms，并发 50）。
3. 官方 App 最多扫 3 个接口。
- 兼容点：≤1.17 官方客户端探测未知设备时用 `GET /api/localsend/v1/info`，官方 1.18 服务端仍提供该路由（返回 v2 的 info 体）。**我们也要同时提供 `v1/info` 与 `v2/info`。**

### 3.4 `GET /api/localsend/v2/info`（及 `v1/info`）
- 响应：`alias, version, deviceModel?, deviceType?, fingerprint, download?`。仅调试/旧客户端探测用。

---

## 4. 上传 API（接收方开 HTTP 服务，发送方是客户端）

### 4.1 `POST /api/localsend/v2/prepare-upload[?pin=123456]`

请求体：

```json
{
  "info": { ...RegisterDtoV2，fingerprint 为发送方证书指纹... },
  "files": {
    "<fileId>": {
      "id": "<fileId>",
      "fileName": "photos/2024/a.jpg",   // 文件夹传输时含 '/'，格式 "<文件夹名>/<相对路径>"
      "size": 324242,                      // u64 字节
      "fileType": "image/jpeg",            // MIME 字符串（探测不到时 application/octet-stream）
      "sha256": "…64位小写hex",            // 可选；官方 1.18 默认生成；接收端大小写不敏感比对
      "preview": "…",                      // 可选；"文本消息" = 恰好 1 个 text/* 文件且 preview 即消息正文
      "metadata": {                        // 可选（v2.1 起）
        "modified": "2026-08-01T10:20:30.456Z",   // RFC 3339，官方接收端会写回到文件 mtime
        "accessed": "2026-08-01T10:20:30.456Z"
      }
    }
  }
}
```

官方服务端处理顺序与语义：
1. **先校验 PIN**（在解析 body 之前）：需要 PIN 而未给 → 401 `PIN required`；给错 → 401 `Invalid PIN` 并计数；同 IP 错 3 次 → 429 `Too many requests`（直到答对或服务重启）。
2. body 非法 → 400；`files` 为空 → 400 `No files provided`。
3. **单会话槽**：已有 pending 或 active 会话 → 409 `Blocked by another session`。
4. 挂起请求（长轮询）等待用户决定；发送方可用关闭连接或 `POST /cancel`（此时可不带 sessionId）放弃。
5. 用户拒绝 → 403 `Rejected`；发送方中途取消 → 403 `Cancelled by sender`。
6. 接受子集：响应 200 `{"sessionId": "<uuid v4>", "files": {"<fileId>": "<token uuid v4>"}}`，只含被接受的文件。
7. 接受空集（例如文本消息"已读"）→ **204 无 body**，不创建会话。
8. 官方发送方：收到 401 弹 PIN 框后带 `?pin=` 重发同一请求；收到 204 或空 `files` 视为完成。
9. `fileType` 判定：官方以 MIME 前缀分类（`image/`、`video/`、`application/pdf`、`text/`、apk），其余 other。文本消息就是 `text/plain` + `preview`。

### 4.2 `POST /api/localsend/v2/upload?sessionId=…&fileId=…&token=…`
- body：裸字节。官方客户端用流式 body（`Transfer-Encoding: chunked`，无 `Content-Length`、无 `Content-Type`），服务端不检查这两个头，忽略 trailer。
- 服务端校验（任一不符 → 403 `Invalid token or IP address`）：存在 active 会话；`sessionId` 匹配；**请求来源 IP（含 IPv6 scope）必须等于 prepare-upload 时的 IP**；`fileId` 存在；`token` 匹配；文件状态为 Pending（同一文件不能并行/重复上传）。
- **字节数必须等于声明的 `size`**，多或少都失败 → 500（消息 `Expected N bytes, received M`）。
- 校验和：若发送方给了 `sha256` 且接收端开启校验（1.18 默认开）→ 不匹配 422 `Checksum mismatch`，该文件回到 Pending，**同一 token 可重传，累计最多 3 次**，之后标记失败。
- 成功：200 空 body。会话内所有被接受文件都到终态（成功/失败）后会话结束。
- 允许不同文件并行上传（官方并发 2）。
- 时间戳：`metadata.modified/accessed` 由接收端写到落盘文件。

### 4.3 `POST /api/localsend/v2/cancel[?sessionId=…]`
- 同 IP 发来：`sessionId` 缺省时取消该 IP 的 pending prepare-upload；带 `sessionId` 时取消匹配的 active 会话。任何情况都返回 200 空 body（未知会话也 200）。
- **反向取消**：接收方取消时，会向**发送方的 HTTP 服务**发 `POST /cancel?sessionId=<接收方分配的 sessionId>`。官方服务端收到不属于自己的 sessionId 时抛 `CancelReceived{ip, sessionId}` 事件，应用层核对 ip 与发送目标一致后停止上传。**因此我们的发送流程也必须在本机开着 HTTP 服务并处理这个事件。**

---

## 5. 下载 API（反向传输，浏览器用）—— 里程碑 1 不实现，仅记录
- `POST /api/localsend/v2/prepare-download[?sessionId=&pin=]` → `{info, sessionId, files}`；`GET /api/localsend/v2/download?sessionId=&fileId=` → 字节流。
- 走明文 HTTP（浏览器不接受自签名证书）；启用期间服务端把客户端证书降为可选，并在发现报文里 `download: true`。
- 我们收到 `download: true` 的设备时只需保留该标志，不做处理。

---

## 6. 接收端文件名规则（官方 1.18，供我们对齐）
1. 按路径分隔符拆分 `fileName`；任一段是 `..` 或绝对路径前缀 → **整个文件拒绝**（`Path traversal detected`），不做"猜测式修复"。
2. `.` 与空段丢弃。
3. 每一段做净化：控制字符、平台非法字符（Windows `<>:"/\|?*`；macOS `/ :`；POSIX `/`）、Windows 保留名（`CON` `NUL` `COM1`… 仅比较第一个 `.` 前的主干）、末尾 `.`/空格、255 字节截断（按字符边界）、空结果 → `untitled`。
4. 目录在接收目录下逐级创建；同名冲突 → `name (1).ext`。
5. 落盘前路径必须仍在接收目录内（对应提示词 §9）。

---

## 7. 与 `protocol.md` 不一致或文档未写明、但官方实际如此的行为（将迁入 `docs/protocol-extensions.md`）

| # | 文档 | 官方 1.18.2 实际 |
|---|---|---|
| 1 | 未提及客户端证书 | 服务端强制 mTLS；身份取自客户端证书指纹 |
| 2 | 指纹 = "SHA-256 of the certificate"，未定大小写 | 证书 DER 的 SHA-256，**大写 hex**；比对时不区分大小写 |
| 3 | register 的 fingerprint "在 HTTPS 下忽略" | 不是忽略：必须与证书指纹一致，否则该注册被丢弃 |
| 4 | 可用 UDP `announce:false` 回应 | 官方 Rust 只发公告，不用 UDP 回应 |
| 5 | 公告一次 | 每次 announce 发 3 个报文（+100/+500/+2000 ms） |
| 6 | 仅 IPv4 组播 | 额外 IPv6 组播 `ff12::fd3a:e420`；HTTP 服务同时监听 v4/v6 |
| 7 | v1 已废弃 | 仍服务 `GET /api/localsend/v1/info` 以兼容 ≤1.17 的探测 |
| 8 | upload 错误 403 "Invalid token or IP address" | 确实校验来源 IP（含 IPv6 scope）等于 prepare-upload 的 IP |
| 9 | 未提字节数 | 收到字节数必须严格等于 `size` |
| 10 | 422 校验失败 | 失败后同 token 最多共 3 次尝试 |
| 11 | 429 未说明触发条件 | 同 IP 错 3 次 PIN |
| 12 | 错误只列状态码 | 错误体为 JSON `{"message": ...}` |
| 13 | cancel 只描述发送方→接收方 | 接收方也会反向对发送方的服务端调用 cancel；pending 时可不带 sessionId |
| 14 | 未定义"文本消息" | 单个 `text/*` 文件 + `preview` = 消息正文；接收方可回 204 表示"已读" |
| 15 | `metadata.modified/accessed` 只写"nullable" | 官方发送方默认携带，接收方默认写回文件时间戳 |
| 16 | 未提 HTTP 版本 | 客户端 ALPN 只允许 `http/1.1` |
| 17 | 未提 JSON 未知字段 | 双方 serde/dart_mappable 均忽略未知字段（我们的扩展字段可安全放入 `info`、`files.*`、组播报文与响应体） |
| 18 | `download` 字段 | 缺省即 false；响应体缺 `fingerprint` 时官方按空串处理 |

---

## 8. 对提示词既定设计的修正建议

1. **`x-mtime` 扩展不需要**：协议 v2.1 起已有 `metadata.modified`，官方 1.18 默认发送并应用。改为直接用标准字段。
2. **完整性校验用标准字段**：`sha256` 小写 hex；我们接收端不匹配返回 422 并允许同 token 重传（上限 3），发送端收到 422 自动重传至多 2 次。
3. **断点续传扩展**（`x-resume-token` + `Range`）：只能在对方也是本项目时启用。建议用扩展字段做能力声明，例如在 `info` / 组播报文 / register 响应里加 `"x-lanext": {"v": 1, "features": ["resume", "clipboard"]}`；官方忽略，无副作用。具体字段名等你定（见 §10）。
4. **发送流程也要开服务端**（§4.3 反向取消），CLI 的 `send` 子命令启动时同样要 bind 53317（端口被本机另一实例占用时需回退到随机端口并在 `info.port` 里上报）。
5. **私有扩展端点的 403 校验**天然可用 mTLS 客户端证书指纹，不需要额外签名。
6. 组播 socket 绑定方式按 §3.1 逐条照做，这是多网卡/macOS 上最容易踩坑的地方。

---

## 9. 里程碑 1 的接口范围（拟）

核心库对外暴露（Rust 公共 API，名字可再议）：
- `identity::{generate_self_signed, fingerprint_from_der, load_or_create(dir)}`
- `discovery::{start(config) -> DiscoveryHandle, DiscoveryHandle::{announce, discover(host,port), scan_subnet, devices, events()}}`
- `server::{start(config) -> ServerHandle, ServerEvent::{Register, PrepareUpload{decision_tx}, FileUpload{target_tx}, SessionEnd, CancelReceived, PrepareUploadAborted}}`
- `client::{Client::new(identity, expected_fingerprint), register, prepare_upload, upload(stream, progress), cancel, info}`
- `protocol::{RegisterDto, RegisterResponseDto, PrepareUploadRequest/Response, FileDto, FileMetadata, MulticastMessage, ErrorResponse, DeviceType, ProtocolType}`

CLI（里程碑 1 只需）：`lan-send discover`、`lan-send send <device> <file>`（单文件）、`lan-send receive [--dir DIR] [--pin PIN]`。

---

## 10. 需要你决策的问题

1. **Rust 工具链**：本机没有 `cargo/rustc`。是否允许我用 rustup 安装到用户目录（`~/.cargo`，stable 通道）？这是环境变更，我没有自作主张。
2. **是否复用官方 Rust 核心 crate**（`packages/core`，Apache-2.0，仅在 monorepo 内以路径依赖存在，未发布到 crates.io；crates.io 上的 `localsend` 是第三方实现）？
   - 方案 A（建议）：不依赖，按提示词自研，官方源码只作行为参考；互操作测试用官方 CLI 二进制。
   - 方案 B：以 git 依赖引入官方 core 作为 **dev-dependency**，在测试里直接跑官方服务端/客户端。优点是互操作测试最准；缺点是要拉整个 monorepo、编译 webrtc 等重依赖。
3. **服务端客户端证书策略**：强制（与官方 1.18 一致，不带证书的旧客户端会被拒）还是可选（出示则校验，不出示则退回以请求体指纹为身份）？我倾向"默认强制、设置里可关"，与官方一致。注：≤1.17 官方客户端是否出示客户端证书我没能证实（Dart `HttpClient` 若复用同一 `SecurityContext` 会自动出示），需要真机验证。
4. **能力声明字段**的名字与位置（建议 `x-lanext`，放在 `info` 与组播报文里）。
5. **IPv6** 是否进里程碑 1（官方已支持；实现成本主要在组播 socket 与 IP 比对，建议 M1 先做 IPv4，M2 加 IPv6）。
6. **默认身份信息**：`deviceType`（桌面 App 用 `desktop`，CLI 用 `headless`？）、`deviceModel`（`macOS` / `Windows` / `Linux`？）、默认 `alias` 生成规则（官方是随机"形容词+水果"）。
7. **应用标识**：配置目录名与 CLI 名。提示词给了 CLI 名 `lan-send`，配置目录 `<app>` 未定。
8. **PIN 失败策略**是否照抄官方（3 次 → 429，直到答对或重启）。
9. 接收端**默认是否校验 sha256**（官方默认开；大文件会多一次全量哈希，但仍是流式，不占内存）。

确认以上问题后我再开始写 ADR 与代码。

---

## 11. 决策记录（2026-09-07，用户确认）

| # | 问题 | 决定 |
|---|---|---|
| 1 | Rust 工具链 | 已用 rustup 安装到 `~/.cargo`（stable） |
| 2 | 是否复用官方 core crate | **自研**，官方源码只作行为参考；互操作测试用官方 CLI 二进制 |
| 3 | 服务端客户端证书 | **默认强制、可在设置关闭**，与官方 1.18 一致 |
| 4 | 能力声明字段 | `x-lanext`，放在 `info`、组播报文与 register 响应中 |
| 5 | IPv6 | 里程碑 2 |
| 6 | 默认身份 | 桌面 App `deviceType=desktop`，CLI `headless`；`deviceModel` 为操作系统名（macOS / Windows / iOS）；CLI 默认 alias 为主机名，App 默认 alias 随机生成，均可改 |
| 7 | 应用标识 | 配置目录名 `lan-send` |
| 8 | PIN 与校验和 | 照官方：同 IP 错 3 次 → 429；接收端默认校验 sha256 |
| 9 | iOS 技术路线 | Tauri 2（与桌面同一工程） |
| 10 | Xcode | 安装；用户已有付费开发者账号，可做真机与 TestFlight |
