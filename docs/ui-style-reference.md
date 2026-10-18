# UI 视觉规范：原生

决策见 ADR-0024（2026-10-18 起替换原来的“深靛蓝渐变 + 薄荷绿”金融风参考）。目标是看起来像 macOS 26 / iOS 26 自带的工具。颜色和尺寸都在 `apps/app/src/styles/tokens.css`，组件样式在 `app.css`（iOS 专用的在 `ios.css`），只引用变量。

## 色彩

| 变量 | 浅色 | 深色 | 用途 |
|---|---|---|---|
| `--window` | `#F5F5F7` | `#141416` | 窗口底色 |
| `--surface` | `#FFFFFF` | `#222225` | 卡片 |
| `--raised` | `#FFFFFF` | `#2A2A2D` | 弹窗 |
| `--glass` | 浅灰 72% | 深灰 72% | 侧栏、标签栏、提示、菜单栏面板（配 `backdrop-filter`） |
| `--text` / `--text-muted` / `--text-faint` | `#1D1D1F` / `#6E6E73` / `#AEAEB2` | `#F5F5F7` / `#98989D` / `#636366` | 正文 / 次要 / 离线圆点等 |
| `--accent` | `#0A72FF` | `#0A84FF` | 主按钮、选中、进度、链接 |
| `--success` | `#28C840` | `#30D158` | 在线、开关打开、成功 |
| `--danger` | `#FF3B30` | `#FF453A` | 取消、删除、失败 |
| `--favorite` | `#FFB800` | `#FFD60A` | 收藏星标 |
| `--fill` / `--fill-strong` | 灰 12% / 20% | 灰 20% / 32% | 次级按钮、输入框、选中的侧栏项 |

设备图标按类型着色：手机蓝、电脑石墨灰、其它（服务器、网页）青色；离线时一律变灰。

## 字体

只用系统字体：`-apple-system`（SF Pro + 苹方），Windows 上 Segoe UI Variable + 微软雅黑。标题、数字用 Display 字形（`--display`）。字号：正文 13 px（iOS 15 px），页面标题 28 px（手机 34 px），传输进度大数字 64 px，标签 11 px。

## 形状与层次

- 卡片：16 px 圆角，不描边，只用 `--lift` 极淡阴影。弹窗 22 px 圆角。
- 按钮、搜索框、标签栏、徽章：胶囊形。主按钮蓝底白字；次级按钮灰色填充；危险按钮淡红底红字。
- 桌面侧栏是悬浮的圆角玻璃面板，离窗口边 8 px，红绿灯落在面板里。iPhone 底部标签栏是悬浮胶囊。玻璃只用 CSS 模拟：真正的窗口透明需要 Tauri 的 macOS 私有 API，上不了 Mac App Store。
- 设备页：桌面是居中的大图标卡片。iPhone 按 ADR-0025 用“磁贴”布局：两列方磁贴，点按展开设备卡片（两块发送大按钮加一排圆形开关）；传输页是进度环大磁贴；收件请求是灵动岛位置的黑色横幅；设置是分组列表、改动立即保存；弹窗贴底浮起。iOS 底色用系统分组列表的 `#F2F2F7` / 纯黑，表面不加阴影。
- 分隔线 0.5 px（`--sep`）。触控目标在 iOS 上不小于 44 pt。

## 图标

源文件 `docs/brand/icon.svg`（满版）与 `docs/brand/menubar.svg`（菜单栏单色模板），`node apps/app/scripts/make-icons.mjs` 一条命令生成全部尺寸（含 iOS 去透明通道、macOS 模板、侧栏小图）。

## 截图自查

`node apps/app/scripts/ui-shots.mjs <输出目录>` 用演示数据拍出所有主要页面的深浅两色截图（桌面、iPhone、菜单栏面板）；商店截图用 `pnpm screenshots`。
