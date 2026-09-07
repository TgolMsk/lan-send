# 0002. 协议类型与设备身份

- 状态：已接受
- 日期：2026-09-07

## 背景

里程碑 1 要与官方 LocalSend 1.18 互通。接口清单证实：身份 = 证书 DER 的 SHA-256 大写 hex；官方服务端强制 mTLS；JSON 未知字段双方都忽略。

## 决策

1. `core::protocol` 只放纯数据：v2.2 的 DTO（serde，camelCase）、常量、`Fingerprint` 新类型。`DeviceType` 未知值回退 `desktop`；`Fingerprint` 反序列化时统一转大写，比较不区分大小写。
2. 私有扩展只通过一个字段 `x-lanext: {v, features[]}` 声明，挂在 `info`、组播报文、register 响应上；官方忽略，我们只在双方都声明时启用对应能力。
3. 身份用 **RSA-2048 自签名证书**（`rsa` 生成密钥，`rcgen` 签发，CN=`lan-send`，有效期取 rcgen 默认即永不过期），与官方一致，避免老客户端对 ECDSA 的兼容风险。持久化为 `identity.pem`（证书 + PKCS#8 私钥），Unix 上权限 0600。
4. 证书校验只检查签名自洽与时间有效，不看颁发者；对端身份一律取握手证书指纹，请求体里的指纹只在 HTTP 明文模式下使用。
5. 时间戳走标准字段 `metadata.modified/accessed`（RFC 3339），不做 `x-mtime`。

## 备选方案

- ECDSA P-256：更快更小，但 ≤1.17 的 Dart 客户端未验证过，放弃。
- 复用官方 crate：已由用户否决（自研）。

## 后果

- RSA 密钥生成在 debug 构建下要几秒，工作区已把 `rsa`/`num-bigint-dig` 的 dev 优化等级设为 2。
- 指纹一旦生成就是设备身份，删除 `identity.pem` 等于换设备，配对关系失效。
