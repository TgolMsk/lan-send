# 应用商店上架：硬性要求与清单

整理日期：2026-09-08。适用于 iOS App Store 与 Mac App Store；命令行版继续走 GitHub Releases。

## 一、全球 App Store（不含中国大陆）需要什么

| 项目 | 要求 | 状态 |
|---|---|---|
| 开发者账号 | Apple Developer Program（已付费，Team ID `VVB976RN4W`） | ✅ |
| 构建 | 2026-04-28 起必须用 Xcode 26 / iOS 26 SDK 构建；CI 用 macos-latest 的 Xcode 26.6 | ✅ |
| 隐私政策 URL | 必填，公开可访问 | ✅ <https://tgolmsk.github.io/lan-send/privacy> |
| 技术支持 URL | 必填 | ✅ <https://tgolmsk.github.io/lan-send/support> |
| App 隐私（数据收集声明） | 在 App Store Connect 填写“不收集数据” | 待填 |
| 年龄分级问卷 | 2026 年新版问卷（含社交媒体问题），本应用无社交功能，预计 4+ | 待填 |
| 出口合规 | 使用标准加密（TLS），`ITSAppUsesNonExemptEncryption = false` 已写入 Info.plist | ✅ |
| 截图 | iPhone 6.9 英寸（1320×2868 或 1290×2796）必传；若支持 iPad 还要 13 英寸 iPad（2064×2752）；Mac 至少 1280×800 | 待做 |
| 图标 | 1024×1024，无透明（iOS）；macOS 圆角模板 | ✅ |
| 元数据 | 名称、副标题、描述、关键词、类别（工具）、版权 | 待填 |
| Mac App Store 额外 | 必须开启 App Sandbox 并声明网络/文件权限；用 Mac App Distribution + Mac Installer Distribution 证书签名；无需公证 | 待做（需要改代码） |
| 审核 | 功能完整、无崩溃、本地网络权限说明清楚（已加 `NSLocalNetworkUsageDescription`） | — |

## 二、中国大陆 App Store 额外需要什么

1. **ICP 备案号（App 备案）**：工信部 2023 年 8 月起要求，苹果已对新 App 做“强校验”——没有备案号的新 App 不能在中国大陆商店上架；个人开发者也一样。备案号在 App Store Connect 的 App 信息里填写。
2. **办理备案的前提**（以阿里云/腾讯云为接入商）：主办者为境内个人（身份证、手机号、人脸核验）或企业（营业执照）；名下要有一个已备案或同时备案的**域名**，以及一台**中国内地的包年包月云服务器**（作为接入服务）；App 上线 30 天内还要做**公安联网备案**。通常 1–4 周。没有后端服务器的应用也必须走这套流程，因为备案挂在接入商名下。
3. **类别许可**：游戏要版号，图书/期刊要网络出版许可，新闻要新闻信息服务许可；lan-send 是工具类，不涉及。
4. **个人信息保护**：隐私政策需覆盖《个人信息保护法》要求（收集范围、目的、用户权利）；lan-send 不收集数据，现有隐私政策已说明。
5. **不上中国大陆商店**则完全不需要备案：在 App Store Connect › 定价与销售范围里把“中国大陆”取消勾选即可，其余 174 个国家/地区正常上架。

**待决定**：是否上架中国大陆。上 → 需要你以个人身份办 App 备案（买域名 + 最便宜的包年包月内地云服务器 + 人脸核验，约 1–4 周）；不上 → 现在就可以提交全球商店。

## 三、Mac App Store 的技术改动（已完成，2026-09-08）

- App Sandbox 已开启（`apps/app/src-tauri/entitlements/mas.plist`，ADR-0014）：`network.client/server`、`files.user-selected.read-write`、`files.downloads.read-write`、`files.bookmarks.app-scope`，外加 `com.apple.application-identifier` / `team-identifier`（缺了 altool 会警告该构建不能用于 macOS TestFlight）。
- 自选接收目录用安全作用域书签保存（`platform/macos.rs`）。
- 证书、描述文件与 `app-mas` 任务见 `docs/release.md`；签名用 `Apple Distribution` + `3rd Party Mac Developer Installer`，`productbuild` 打 `.pkg`，`altool --type macos` 上传。

## 三点五、提交时踩过的坑（每次被拒都记在这里；提交前照 `docs/store-checklist.md` 逐项过）

- **审核信息里“需要登录”默认勾选**：不取消会因“用户名/密码为必填”而无法“添加以供审核”。应用没有账号，取消勾选即可。
- **出口合规**：Info.plist 里声明 `ITSAppUsesNonExemptEncryption=false`（iOS 在 `Info.ios.plist`，macOS 在 `Info.plist`），App Store Connect 就不再问加密问题。若走手动申报，选“标准加密算法”后会追问“是否在法国分发”，答“是”需上传法国的加密申报文件——直接用 plist 声明可避开。
- **macOS 沙盒说明**（版本页“App 沙盒信息”，可不填）：已为 `network.server`、`network.client`、`files.downloads.read-write`、`files.user-selected.read-write` 各写一句用途，方便审核员理解为何要监听端口。
- **构建版本替换**：版本页里点构建行的“删除”再“添加构建版本”；上传后要等处理完（TestFlight 页出现“准备提交”）才会出现在列表里。
- **被拒：2.4.5(i) `com.apple.security.files.downloads.read-write` 无对应功能（macOS 0.3.0 构建 19，2026-09-10 中招）**。与 network.server 那次同一个根源：苹果的自动分析只认 `NSFileManager` / `NSOpenPanel` 这类 Apple API 的调用，看不见 Rust 的 `std::fs`。实测沙盒容器里 `Data/Downloads` 是指向真实 `~/Downloads` 的符号链接（每个容器都会建，与权限无关；权限只决定写入是否被允许，没有它写入报 EPERM），所以构建 19 其实一直在往真实下载目录写，权限并非没用。不过 `$HOME`、`NSHomeDirectory()`、`NSHomeDirectoryForUser()`、`FileManager .downloadsDirectory` 在沙盒里**全部**返回容器路径，`directories` crate 算出的默认接收目录字面上是容器路径，设置页显示出来也难看。修法见 `crates/core/src/store/platform/macos.rs`：用 `getpwuid_r` 取真实主目录（唯一不被重定向的查询），默认接收目录显式为 `<真实主目录>/Downloads`，`scripts/mas-sandbox-check.sh` 在临时签名的沙盒 bundle 里验证。**教训**：Rust 应用的每一个沙盒权限都会被静态扫描判成"无对应功能"，所以提交前就把每个权限"在哪用、怎么验证、为什么扫描看不到"写进 App 审核信息的备注和 App 沙盒信息，不要等被拒再解释。**处理记录（2026-09-10）**：构建 20（bfbb4bc，含 `getpwuid_r` 修法、版权串、`NSDownloadsFolderUsageDescription`、iOS `PrivacyInfo.xcprivacy`）上传后在版本页直接删掉构建 19、添加构建 20 并保存；审核备注补齐六个权限的用途 / 验证方法 / 扫描看不见的原因；在同一提交串里"回复 App 审核"贴同样的说明（2626 字符）；版本页"更新审核" → 提交页"重新提交至 App 审核"，状态回到"等待审核"。iOS 0.3.0（构建 19）未受影响，仍在排队。
- **被拒：1.5 Safety 支持网址不合格（macOS 0.3.0，2026-09-10 同一封信）**。支持页只放了 GitHub issues 链接（要账号）和 FAQ，苹果认为用户没有"提问和请求支持"的途径。修法：页面顶部放**电子邮件**（`mailto:` 链接）并写明回复时限，issues 作为补充；页面标题用商店里的名字 `LanSend`。改的是 `docs/support.md`，GitHub Pages 推送后几分钟生效，无需新构建。**教训**：支持页至少要有一个不需要注册就能用的联系方式，隐私页同理要能直接打开。
- **被拒：2.1 Information Needed（iOS，2026-09-08 中招）**。新开发者账号审核记录少，苹果要求补交六项资料，并要求同时写进"App 审核信息 › 备注"。这不是功能缺陷，是例行尽调。备注字段**上限 4000 字符**，写超了保存会报"此栏过长"。六项分别是：
  1. **在真机上录的屏幕录像**（模拟器不算），从启动应用开始，覆盖典型流程；有账号注册 / 登录 / 注销、用户生成内容、付费内容的都要录进去（本应用三者都没有，在备注里写明）。
  2. 应用用途与目标用户，解决什么问题、提供什么价值。
  3. 使用与访问说明，包括登录凭据或示例文件（本应用无账号，写明无需凭据，并给出两台设备互传的验证步骤）。
  4. 交付核心功能所依赖的外部服务、工具、平台（数据源、认证、支付、AI 等，本应用一个都没有）。
  5. 各地区功能 / 内容是否有差异（本应用无差异，只有界面语言跟随系统）。
  6. 是否属于强监管行业、是否含受保护的第三方素材（本应用都不涉及；LocalSend 协议是开放规范，独立实现，未打包其代码）。
  录像里一定要留一手：局域网发现依赖组播，而 iOS 的组播权限尚未获批（见 `docs/release.md`），审核网络若禁用组播就发现不到设备。备注第 3 条里写了改用"按地址发送"输入对方 IP 的备选路径。
- **被拒：2.4.5 Performance: Hardware Compatibility（macOS，2026-09-08 中招）**。苹果的自动分析认为 App 带了 `com.apple.security.network.server` 权限却"没有对应功能"，提交被拒。原因是我们的监听 socket 写在 Rust 里（`std::net` / `tokio::TcpListener`，走 BSD socket 的 bind/listen/accept），不是 Network.framework / NSNetService / CFSocket，静态扫描认不出来。**这个权限必须保留**——LocalSend 协议是对称的，不监听就完全收不到文件。处理办法（苹果消息里给的第二条）：
  1. 在 App 审核信息的"备注"里写清楚为什么需要该权限：TCP 53317 跑 HTTPS 服务器接收 prepare-upload / upload，UDP 53317 加入组播组 224.0.0.167 应答设备发现；并说明扫描认不出来的原因和验证方法（另一台设备装 LanSend 或官方 LocalSend 互传，或 `nc -vz <ip> 53317`）。
  2. 在被拒提交页点"回复 App 审核"，把同样的说明发给审核团队。
  3. 回到版本页点"更新审核"（此时提交项目变成"可供审核"），再回提交页点"重新提交至 App 审核"。不需要重新打包上传，构建版本不变。

## 五、多语言商店文案（2026-09-13 起草，8 种语言）
App Store Connect › App › 左侧“App 信息 / 版本信息”右上角语言下拉里逐个添加本地化，把下面各段粘贴进去；名称统一为 **LanSend**（不翻译）。字数限制：名称 30、副标题 30、推广文本 170、关键词 100（逗号分隔、不带空格、不重复 App 名）、描述 4000。宣传文本可随时改，其余随版本提交审核。
截图带界面文字，每种语言最好用对应语言的界面重出一套（`apps/app/scripts/store-screenshots.mjs`），至少保证英文与简中两套。

### English (U.S.)（ASC 本地化：en-US）
- **名称**：LanSend
- **副标题**：Wi‑Fi file sharing, no cloud
- **推广文本**：Send photos, videos, folders and your clipboard between iPhone, Mac and Windows on the same Wi‑Fi. Nothing leaves your network.
- **关键词**：`file transfer,wifi,share,localsend,clipboard,lan,photos,send,nearby,offline,p2p,airdrop`
- **描述**：

  > LanSend moves files between your devices over the local network. No account, no cloud, no size limits: the data goes straight from one device to the other.
  >
  > WHAT IT DOES
  > • Send photos, videos, documents and whole folders to another phone or computer on the same Wi‑Fi
  > • Receive files into the folder you choose, sorted by device, date or type if you like
  > • Keep the clipboard in sync between paired computers (macOS and Windows)
  > • Resume interrupted transfers instead of starting over
  > • Verify every file with a checksum
  >
  > WORKS WITH LOCALSEND
  > LanSend speaks the open LocalSend protocol, so it also talks to the free LocalSend app on Android, Linux and other platforms.
  >
  > PRIVACY
  > Everything stays on your network. LanSend has no servers, collects no data and needs no sign‑in. Transfers are encrypted end to end with TLS, and you can require a PIN or a one‑time pairing before anyone can send to you.
  >
  > ON YOUR NETWORK
  > Devices find each other automatically. On networks that block discovery you can still send by typing the other device's IP address.

- **新功能**：

  > • Interface available in 8 languages
  > • New name and icon
  > • Faster device discovery on iPhone

### 简体中文（ASC 本地化：zh-Hans）
- **名称**：LanSend
- **副标题**：局域网互传文件，不经云端
- **推广文本**：在同一 Wi‑Fi 下的 iPhone、Mac 和 Windows 之间发送照片、视频、文件夹和剪贴板，数据不出本地网络。
- **关键词**：`文件传输,局域网,互传,隔空投送,LocalSend,剪贴板,照片,wifi,发送,离线,附近,电脑`
- **描述**：

  > LanSend 通过局域网在你的设备之间传文件。不用账号，不经云端，没有大小限制：数据直接从一台设备到另一台。
  >
  > 功能
  > • 把照片、视频、文档和整个文件夹发给同一 Wi‑Fi 里的手机或电脑
  > • 接收到你指定的文件夹，可按设备、日期或类型分目录
  > • 已配对的电脑之间同步剪贴板（macOS 与 Windows）
  > • 传输中断后从断点继续，不必重来
  > • 每个文件都做校验和核对
  >
  > 兼容 LocalSend
  > LanSend 使用开放的 LocalSend 协议，因此也能和 Android、Linux 等平台上的免费 LocalSend 应用互传。
  >
  > 隐私
  > 一切都留在你的网络里。LanSend 没有服务器，不收集数据，不需要登录。传输全程 TLS 加密，你还可以要求对方输入 PIN 或先完成一次配对才能向你发送。
  >
  > 网络
  > 设备会自动互相发现。在禁止发现的网络里，也可以直接输入对方的 IP 地址发送。

- **新功能**：

  > • 界面新增 8 种语言
  > • 新名称与新图标
  > • iPhone 上更快地发现设备

### 繁體中文（ASC 本地化：zh-Hant）
- **名称**：LanSend
- **副标题**：區域網路互傳檔案，不經雲端
- **推广文本**：在同一 Wi‑Fi 下的 iPhone、Mac 和 Windows 之間傳送相片、影片、資料夾與剪貼簿內容，資料不離開本地網路。
- **关键词**：`檔案傳輸,區域網路,互傳,隔空投送,LocalSend,剪貼簿,相片,wifi,傳送,離線,附近,電腦`
- **描述**：

  > LanSend 透過區域網路在你的裝置之間傳輸檔案。不需帳號、不經雲端，也沒有檔案大小限制：資料直接從一台裝置傳到另一台。
  >
  > 功能特色
  > • 把相片、影片、文件和整個資料夾傳送給同一 Wi‑Fi 內的手機或電腦
  > • 接收到你指定的資料夾，可依裝置、日期或類型分類存放
  > • 已配對的電腦之間同步剪貼簿內容（macOS 與 Windows）
  > • 傳輸中斷後可從中斷處繼續，不必重新開始
  > • 每個檔案都會核對校驗碼
  >
  > 相容 LocalSend
  > LanSend 採用開放的 LocalSend 通訊協定，因此也能與 Android、Linux 等平台上的免費 LocalSend 應用程式互相傳輸。
  >
  > 隱私
  > 所有資料都留在你的網路裡。LanSend 沒有伺服器、不收集任何資料，也不需要登入。傳輸全程以 TLS 加密，你還可以要求對方輸入 PIN，或先完成一次配對才能傳送檔案給你。
  >
  > 網路環境
  > 裝置會自動互相發現。在封鎖探索功能的網路中，也可以直接輸入對方的 IP 位址來傳送。

- **新功能**：

  > • 介面新增 8 種語言
  > • 全新名稱與圖示
  > • iPhone 上裝置探索速度更快

### 日本語（ASC 本地化：ja）
- **名称**：LanSend
- **副标题**：クラウド不要、Wi‑Fiでファイル共有
- **推广文本**：同じWi‑Fi上のiPhone、Mac、Windows間で写真・動画・フォルダ・クリップボードを送信できます。データはネットワークの外に出ません。
- **关键词**：`ファイル転送,wifi,共有,localsend,クリップボード,lan,写真,送信,近くのデバイス,オフライン,p2p,airdrop`
- **描述**：

  > LanSendはローカルネットワーク経由でデバイス間のファイルをやり取りします。アカウント登録もクラウドもサイズ制限もなく、データは一方の端末からもう一方へ直接送られます。
  >
  > できること
  > • 同じWi‑Fi上の他のスマートフォンやパソコンに、写真、動画、書類、フォルダ全体を送信
  > • 好みに応じてデバイス別・日付別・種類別に振り分けて受信フォルダに保存
  > • ペアリングしたパソコン(macOSとWindows)間でクリップボードを同期
  > • 中断した転送を最初からではなく続きから再開
  > • すべてのファイルをチェックサムで検証
  >
  > LocalSendと互換
  > LanSendはオープンなLocalSendプロトコルを使用しているため、Android、Linuxなど他のプラットフォームの無料アプリLocalSendとも通信できます。
  >
  > プライバシー
  > すべての通信はネットワーク内で完結します。LanSendにはサーバーがなく、データを収集せず、サインインも不要です。転送はTLSでエンドツーエンドに暗号化され、送信を受け付ける前にPINやワンタイムのペアリングを必須にすることもできます。
  >
  > ネットワークについて
  > デバイスは自動的に見つかります。検出がブロックされるネットワークでも、相手のIPアドレスを直接入力すれば送信できます。

- **新功能**：

  > • インターフェースが8言語に対応
  > • 新しい名前とアイコン
  > • iPhoneでのデバイス検出を高速化

### 한국어（ASC 本地化：ko）
- **名称**：LanSend
- **副标题**：Wi‑Fi로 파일 공유, 클라우드 없이
- **推广文本**：iPhone, Mac, Windows가 같은 Wi‑Fi에 있으면 사진, 동영상, 폴더, 클립보드를 주고받을 수 있습니다. 데이터는 네트워크 밖으로 나가지 않습니다.
- **关键词**：`파일전송,wifi,공유,localsend,클립보드,lan,사진,전송,근처기기,오프라인,p2p,에어드롭`
- **描述**：

  > LanSend는 로컬 네트워크를 통해 기기 간에 파일을 전송합니다. 계정도, 클라우드도, 용량 제한도 없이 데이터가 한 기기에서 다른 기기로 바로 전달됩니다.
  >
  > 주요 기능
  > • 같은 Wi‑Fi에 있는 다른 휴대폰이나 컴퓨터로 사진, 동영상, 문서, 폴더 전체를 전송
  > • 원하는 대로 기기별, 날짜별, 종류별로 정리해 받는 폴더에 저장
  > • 페어링된 컴퓨터(macOS와 Windows) 간에 클립보드를 동기화
  > • 중단된 전송을 처음부터 다시 하지 않고 이어서 진행
  > • 체크섬으로 모든 파일 검증
  >
  > 로컬센드와 호환
  > LanSend는 공개된 LocalSend 프로토콜을 사용하므로 Android, Linux 등 다른 플랫폼의 무료 LocalSend 앱과도 통신할 수 있습니다.
  >
  > 개인정보 보호
  > 모든 데이터는 사용자의 네트워크 안에만 머무릅니다. LanSend는 서버가 없고 데이터를 수집하지 않으며 로그인도 필요 없습니다. 전송은 TLS로 종단 간 암호화되며, PIN이나 일회성 페어링을 요구해 아무나 파일을 보낼 수 없게 설정할 수 있습니다.
  >
  > 네트워크에서
  > 기기는 자동으로 서로를 찾습니다. 검색이 차단된 네트워크에서도 상대 기기의 IP 주소를 직접 입력해 전송할 수 있습니다.

- **新功能**：

  > • 8개 언어로 인터페이스 제공
  > • 새로운 이름과 아이콘
  > • iPhone에서 더 빨라진 기기 검색

### Deutsch（ASC 本地化：de-DE）
- **名称**：LanSend
- **副标题**：Dateien per WLAN, ohne Cloud
- **推广文本**：Sende Fotos, Videos, Ordner und deine Zwischenablage zwischen iPhone, Mac und Windows im selben WLAN. Nichts verlässt dein Netzwerk.
- **关键词**：`dateiübertragung,wlan,teilen,localsend,zwischenablage,lan,fotos,senden,nähe,offline,p2p,airdrop`
- **描述**：

  > LanSend überträgt Dateien zwischen deinen Geräten über das lokale Netzwerk. Kein Konto, keine Cloud, keine Größenbeschränkung: Die Daten gehen direkt von einem Gerät zum anderen.
  >
  > WAS ES KANN
  > • Fotos, Videos, Dokumente und ganze Ordner an ein anderes Telefon oder einen anderen Computer im selben WLAN senden
  > • Dateien in den Ordner deiner Wahl empfangen, auf Wunsch sortiert nach Gerät, Datum oder Typ
  > • Die Zwischenablage zwischen gekoppelten Computern synchron halten (macOS und Windows)
  > • Unterbrochene Übertragungen fortsetzen, statt von vorn zu beginnen
  > • Jede Datei mit einer Prüfsumme verifizieren
  >
  > FUNKTIONIERT MIT LOCALSEND
  > LanSend spricht das offene LocalSend-Protokoll und kann daher auch mit der kostenlosen LocalSend-App unter Android, Linux und anderen Plattformen kommunizieren.
  >
  > DATENSCHUTZ
  > Alles bleibt in deinem Netzwerk. LanSend hat keine Server, sammelt keine Daten und benötigt keine Anmeldung. Übertragungen sind Ende-zu-Ende mit TLS verschlüsselt, und du kannst eine PIN oder eine einmalige Kopplung verlangen, bevor dir jemand etwas senden kann.
  >
  > IN DEINEM NETZWERK
  > Geräte finden sich automatisch. In Netzwerken, die die Erkennung blockieren, kannst du trotzdem senden, indem du die IP-Adresse des anderen Geräts eingibst.

- **新功能**：

  > • Oberfläche in 8 Sprachen verfügbar
  > • Neuer Name und neues Icon
  > • Schnellere Geräteerkennung auf dem iPhone

### Français（ASC 本地化：fr-FR）
- **名称**：LanSend
- **副标题**：Fichiers par Wi-Fi, sans cloud
- **推广文本**：Envoyez photos, vidéos, dossiers et le presse-papiers entre iPhone, Mac et Windows sur le même Wi-Fi. Rien ne quitte votre réseau.
- **关键词**：`transfert,wifi,partage,localsend,presse-papiers,lan,photos,envoyer,proximite,hors-ligne,p2p,airdrop`
- **描述**：

  > LanSend transfère des fichiers entre vos appareils sur le réseau local. Pas de compte, pas de cloud, aucune limite de taille : les données passent directement d'un appareil à l'autre.
  >
  > CE QU'IL FAIT
  > • Envoyez photos, vidéos, documents et dossiers entiers vers un autre téléphone ou ordinateur sur le même Wi-Fi
  > • Recevez les fichiers dans le dossier de votre choix, triés par appareil, date ou type si vous le souhaitez
  > • Gardez le presse-papiers synchronisé entre ordinateurs jumelés (macOS et Windows)
  > • Reprenez les transferts interrompus au lieu de tout recommencer
  > • Vérifiez chaque fichier grâce à une somme de contrôle
  >
  > COMPATIBLE AVEC LOCALSEND
  > LanSend utilise le protocole ouvert LocalSend, et communique donc aussi avec l'application gratuite LocalSend sur Android, Linux et d'autres plateformes.
  >
  > CONFIDENTIALITÉ
  > Tout reste sur votre réseau. LanSend n'a pas de serveur, ne collecte aucune donnée et ne nécessite aucune connexion. Les transferts sont chiffrés de bout en bout avec TLS, et vous pouvez exiger un code PIN ou un jumelage ponctuel avant que quiconque puisse vous envoyer quelque chose.
  >
  > SUR VOTRE RÉSEAU
  > Les appareils se détectent automatiquement. Sur les réseaux qui bloquent la découverte, vous pouvez toujours envoyer en saisissant l'adresse IP de l'autre appareil.

- **新功能**：

  > • Interface disponible en 8 langues
  > • Nouveau nom et nouvelle icône
  > • Découverte des appareils plus rapide sur iPhone

### Español（ASC 本地化：es-ES / es-MX）
- **名称**：LanSend
- **副标题**：Comparte archivos por Wi‑Fi
- **推广文本**：Envía fotos, vídeos, carpetas y tu portapapeles entre iPhone, Mac y Windows en la misma red Wi‑Fi. Nada sale de tu red.
- **关键词**：`transferencia,wifi,compartir,localsend,portapapeles,lan,fotos,enviar,cercano,sinconexion,p2p,airdrop`
- **描述**：

  > LanSend mueve archivos entre tus dispositivos a través de la red local. Sin cuenta, sin nube, sin límites de tamaño: los datos van directamente de un dispositivo a otro.
  >
  > QUÉ HACE
  > • Envía fotos, vídeos, documentos y carpetas enteras a otro teléfono u ordenador en la misma red Wi‑Fi
  > • Recibe archivos en la carpeta que elijas, organizados por dispositivo, fecha o tipo si lo prefieres
  > • Mantén el portapapeles sincronizado entre ordenadores emparejados (macOS y Windows)
  > • Reanuda las transferencias interrumpidas en lugar de empezar de nuevo
  > • Verifica cada archivo con una suma de comprobación
  >
  > COMPATIBLE CON LOCALSEND
  > LanSend habla el protocolo abierto de LocalSend, así que también se comunica con la app gratuita LocalSend en Android, Linux y otras plataformas.
  >
  > PRIVACIDAD
  > Todo se queda en tu red. LanSend no tiene servidores, no recopila datos y no requiere iniciar sesión. Las transferencias se cifran de extremo a extremo con TLS, y puedes exigir un PIN o un emparejamiento único antes de que alguien pueda enviarte algo.
  >
  > EN TU RED
  > Los dispositivos se encuentran automáticamente. En redes que bloquean el descubrimiento, igualmente puedes enviar escribiendo la dirección IP del otro dispositivo.

- **新功能**：

  > • Interfaz disponible en 8 idiomas
  > • Nuevo nombre e icono
  > • Descubrimiento de dispositivos más rápido en iPhone

## 四、参考

- [苹果 App Store 已开启 ICP 备案强校验](https://www.baijing.cn/article/48165)
- [关于个人开发者 App 上架 ICP 备案问题](https://blog.csdn.net/weixin_39339407/article/details/135037851)
- [阿里云 App 备案快速入门](https://help.aliyun.com/zh/icp-filing/basic-icp-service/getting-started/quick-sta-rt-for-icp-filing-for-personal-app)
- [Apple: 2026 年提交要求（Xcode 26 SDK）](https://9to5mac.com/2026/02/03/apple-to-update-minimum-sdk-requirements-for-all-app-store-submissions/)
- [App Store 年龄分级问卷 2026 年 9 月起必填](https://finance.sina.com.cn/tech/digi/2026-07-10/doc-inihhzsa3895645.shtml)
- [App Store 截图尺寸 2026](https://www.mobileaction.co/guide/app-screenshot-sizes-and-guidelines-for-the-app-store/)
- [Is app-sandbox entitlement required for App Store?](https://developer.apple.com/forums/thread/739482)
