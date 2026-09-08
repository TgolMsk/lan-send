# 0015. 媒体层：MIME 探测、缩略图与音频元数据

- 状态：已接受
- 日期：2026-09-08

## 背景

简报 §6–8 要求：按 magic bytes 探测 MIME 并写入协议的 `fileType`；接收完成后异步生成 256 px 缩略图供历史展示；读取图片尺寸与 EXIF 方向并在预览时正确旋转，但**不**改动传输的字节；音频读取标题 / 艺术家 / 专辑 / 时长 / 封面。简报点名 `libheif-rs` 解 HEIC、`image` 的 avif 特性解 AVIF。核对后两者都有问题：libheif 是 LGPL，且依赖 libde265（LGPL）或 x265（GPL），与"依赖必须 MIT 兼容"冲突，还要在三端编译 C++；`image` 的 avif 解码需要系统 libdav1d。用户于 2026-09-08 批准了下面的替代方案。

## 决策

1. **MIME**：`infer` 按 magic bytes 探测（读文件头 8 KB），探测不到时用扩展名（`mime_guess`），都没有则 `application/octet-stream`。发送方在收集文件时探测；接收方若发送方声明的是通用类型，落盘后重新探测写入历史。
2. **解码策略**：`media::decode` 先试**系统解码器**——macOS / iOS 用 ImageIO（`CGImageSourceCreateThumbnailAtIndex`，原生支持 HEIC / AVIF / WebP 等），Windows 用 WIC（`IWICImagingFactory`，HEIC / AVIF 依赖用户安装的 HEIF / AV1 扩展）；失败再用纯 Rust 的 `image`（PNG、JPEG、GIF、WebP、BMP、TIFF）。两者都失败则该文件"无预览"，传输不受影响。SVG 只识别不栅格化。平台代码只在 `media/platform/{apple,windows}.rs`。
3. **EXIF 方向**：统一用 `kamadak-exif` 读取（JPEG / TIFF / HEIF / PNG / WebP 容器），在解码后的像素上旋转；不让系统解码器自行旋转，避免双重旋转。原文件字节永不修改。
4. **缩略图缓存**：`<系统缓存目录>/lan-send/thumbs/<sha256(路径, 大小, 修改时间)前 32 位>.jpg`，长边 256 px，JPEG 质量 85，透明像素合成到中性灰（#2A2D3A）。总量上限 200 MB，超出时按最近访问时间淘汰（命中即 touch）；设置页可查看大小并一键清空。发送方也为已发文件生成，历史与传输卡片都能显示。
5. **音频**：`lofty` 读标签（标题、艺术家、专辑）、时长与封面；封面按同样规则生成缩略图。`symphonia`（波形）留到里程碑 7。
6. **运行时接口**：`Runtime::media_info(path)` 返回 `MediaView { kind, thumbnail, width, height, title, artist, album, duration_ms }`，命中缓存则立即返回，否则在阻塞线程池里生成；传输完成后后台预热所有已完成文件。Tauri 命令 `cmd_media_info / cmd_media_cache_size / cmd_media_cache_clear`，前端通过 asset 协议显示缩略图（作用域 `$CACHE/lan-send/**`）。
7. **不做**：接收请求弹窗里的对方文件预览（需要私有扩展携带预览图，记为可选项）；HEIC → JPEG 发送前转换（里程碑 7）。

## 备选方案

- `libheif-rs` + `libde265`：许可证不兼容，构建复杂，且 iOS 上静态链接 LGPL 库有合规风险。
- `image` 的 avif 特性（libdav1d）：Windows / iOS 需要自行交叉编译 C 库；系统解码器已经覆盖。
- 让 ImageIO 直接按 EXIF 旋转：与 `image` 回退路径行为不一致，改为统一在像素层处理。

## 后果

- 正面：零 GPL/LGPL 依赖；Apple 端 HEIC / AVIF 开箱即用；纯 Rust 回退保证 Linux CI 也能测主流程。
- 负面：Windows 上 HEIC / AVIF 预览取决于系统扩展是否安装；缩略图质量在 Windows 与 Apple 之间可能略有差异；WIC 代码只能在 CI 的 Windows 任务里验证。
