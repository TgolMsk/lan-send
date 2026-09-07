# 0011. 剪贴板同步

- 状态：已接受
- 日期：2026-09-07

## 背景

剪贴板同步是区别于 LocalSend 的核心能力（简报 §5）。用户决定：只对已配对设备开启、双向同步、敏感内容按启发式不入历史、iOS 不做。平台差异大：macOS 只能轮询 `changeCount`，Windows 有 `AddClipboardFormatListener` 事件。

## 决策

1. **数据模型**：`ClipboardItem { id, origin_device(指纹), created_at, payload, content_hash }`，`payload` 为 `Text { plain, html?, rtf? }`、`Image { format, bytes, width, height }`、`Files { paths }`。`content_hash` = SHA-256（文本取 `plain`，图片取字节，文件取路径列表），用于回环防止与去重。
2. **平台抽象**：`ClipboardBackend` trait（`read` / `write` / `wait_for_change(timeout)`），实现放在 `clipboard/backend/{macos,windows}.rs`；其他平台返回"不支持"。macOS 用 `objc2-app-kit` 的 `NSPasteboard`，每 300 ms（可配）比较 `changeCount`；读图片优先 PNG，其次 TIFF 经 `NSBitmapImageRep` 转 PNG。Windows 用 `windows` crate：仅消息窗口 + `AddClipboardFormatListener` 事件驱动；图片优先注册格式 `"PNG"`，其次 `CF_DIBV5` 转 PNG。两端读写遇到"剪贴板被占用"的瞬时错误时按 50/100/200 ms 退避重试 3 次后放弃并记录，不崩溃。
3. **同步引擎**（平台无关）：监听线程发现变化 → 读取 → 算哈希 → 若哈希在"最近写入"的 10 项环形缓冲中则忽略（回环防止）→ 超限（文本 1 MB、图片 10 MB，可配）则只发事件提示手动发送 → 并行推送给全部已配对且在线的设备。收到远端条目时先把哈希记入环形缓冲再写入本地剪贴板。
4. **传输通道**：`POST /api/ext/v1/clipboard`，要求请求方证书指纹已配对（否则 403）。图片 ≤ 512 KB 时 JSON + base64，否则 `multipart/form-data`（`item` JSON 部分 + `image` 二进制部分）。服务端按上限拒绝 413。文件类型剪贴板不走此端点：转为一次普通文件传输，`info.x-lanext.intent = "clipboard"` 标记意图，对方收完后把文件列表写入其剪贴板。
5. **历史**：`clipboard_items` 表默认保留 50 条（可配），图片字节存缓存目录、表中只记路径；可查看、重新复制、删除、清空。文本满足"无空白、长度 20–128、字符熵 ≥ 3.5 位/字符、不是 URL/路径/邮箱"视为密钥类，只同步不入历史。
6. **日志**：剪贴板内容永不进日志，只记类型与大小。
7. iOS 编译时不含剪贴板后端。

## 备选方案

- `arboard` 等通用剪贴板库：没有变更通知、图片只有 RGBA 位图、拿不到 HTML/RTF，放弃。
- 内容轮询比较（不看 `changeCount`）：大图片每 300 ms 读一次代价太高，放弃。

## 后果

- Windows 后端只能通过 CI 编译与用户的 Windows 机器实测。
- 引擎与端点可在无剪贴板的环境（CI、Linux）用内存后端测试。
