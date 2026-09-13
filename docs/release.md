# 发布流程

打 `v<版本>` 标签即触发 `.github/workflows/release.yml`：三端构建安装包并发布到 GitHub Releases。`Actions › Release › Run workflow` 可手动跑一次构建（勾选 publish 才发布）。

## 产物

| 平台 | 文件 | 说明 |
|---|---|---|
| macOS 应用 | `lan-send-<ver>-macos-universal.dmg`、`.app.zip` | Tauri 应用，通用二进制；有证书 secrets 时由 Tauri 自动签名并公证 |
| Windows 应用 | `lan-send-<ver>-windows-x86_64-setup.exe`（NSIS，中英文）、`.msi` | Tauri 应用，按用户安装 |
| macOS CLI | `lan-send-cli-<ver>-macos-universal.pkg` / `.tar.gz` | 安装到 `/usr/local/bin` |
| Windows CLI | `lan-send-cli-<ver>-windows-x86_64.msi`、`-{x86_64,arm64}.zip` | 安装到 Program Files 并加入 PATH（WiX，`crates/cli/wix/main.wxs`） |
| Linux CLI | `lan-send-cli-<ver>-linux-{x86_64,aarch64}.tar.gz` | 免安装（Linux 只作测试用途） |
| 全部 | `SHA256SUMS.txt` | 校验和 |

iOS：配置了下面的 App Store Connect API Key secrets 后，`ios-testflight` 任务会用自动签名（Xcode 云端管理的分发证书与描述文件）打出 `.ipa`，用 `altool` 上传到 TestFlight，并把 `.ipa` 附到 Release；没有配置时该任务跳过。

版本号取自标签；`0.x` 或带 `-` 的版本自动标记为预发布。发布说明取自 `CHANGELOG.md` 中对应版本的小节。

publish 任务偶尔被 GitHub 接口 500 打断（2026-09-13 的 v0.4.0 连续两次），构建产物已存为 artifact，可在本机补传，不必重跑构建：

```bash
gh run download <run-id> --repo TgolMsk/lan-send --dir dist && mkdir -p flat && find dist -type f -exec mv {} flat/ \; && cd flat && shasum -a 256 * > SHA256SUMS.txt && gh release upload v<版本> --repo TgolMsk/lan-send --clobber *
```

## 步骤

```bash
# 1. CHANGELOG.md 把 Unreleased 改成版本号和日期；Cargo.toml、apps/app/package.json、apps/app/src-tauri/tauri.conf.json 的 version 保持一致（CI 打包时会按标签覆盖应用版本号）
# 2. 提交后打标签并推送
git tag v0.1.0
git push origin v0.1.0
```

## macOS 签名与公证（可选）

在仓库 `Settings › Secrets and variables › Actions` 配置以下 secrets 后，macOS 的应用与 CLI 产物都会自动签名并公证（应用由 Tauri 读取 `APPLE_*` 环境变量完成，工作流已把这些 secrets 映射过去）；没有时产出未签名包，用户首次打开需要在"系统设置 › 隐私与安全性"里允许。

| Secret | 内容 |
|---|---|
| `MACOS_CERTIFICATE_P12` | Developer ID Application 证书（含私钥）的 `.p12`，base64 编码 |
| `MACOS_CERTIFICATE_PASSWORD` | 上述 `.p12` 的密码 |
| `MACOS_INSTALLER_CERTIFICATE_P12` | Developer ID Installer 证书的 `.p12`，base64 编码（签 `.pkg`） |
| `MACOS_INSTALLER_CERTIFICATE_PASSWORD` | 其密码 |
| `APPLE_ID` | 公证用的 Apple ID |
| `APPLE_TEAM_ID` | 团队 ID |
| `APPLE_APP_PASSWORD` | 该 Apple ID 的 App 专用密码 |

导出证书：钥匙串访问里选中证书 › 导出 › `.p12`，然后 `base64 -i cert.p12 | pbcopy`。

## iOS：TestFlight 需要的准备（只能由账号持有人操作）

1. **App Store Connect 里建应用**：Apps › 新建 App，平台 iOS，名称 `lan-send`（或你想要的名字），Bundle ID 选 `com.wangsheng.lansend`（先在 [Certificates, Identifiers & Profiles › Identifiers](https://developer.apple.com/account/resources/identifiers/list) 注册这个 App ID；想换成自己的域名前缀也可以，同时改 `apps/app/src-tauri/tauri.conf.json` 的 `identifier` 和 `gen/apple/project.yml`）。
2. **生成 API Key**：App Store Connect › Users and Access › Integrations › App Store Connect API › Team Keys › 生成，角色 **App Manager**（自动签名需要它能创建描述文件）。记下 **Issuer ID**、**Key ID**，下载 `AuthKey_XXXX.p8`（只能下载一次）。
3. **Team ID**：developer.apple.com › Membership details。
4. 在仓库 `Settings › Secrets and variables › Actions` 添加：

| Secret | 内容 |
|---|---|
| `APPSTORE_ISSUER_ID` | Issuer ID |
| `APPSTORE_KEY_ID` | Key ID |
| `APPSTORE_PRIVATE_KEY` | `.p8` 文件的完整文本（含 BEGIN/END 行） |
| `APPLE_TEAM_ID` | Team ID（与 macOS 公证共用） |

5. 之后推送 `v*` 标签或手动运行 Release（勾选 publish 与否都会上传 TestFlight）。上传后在 App Store Connect › TestFlight 里等处理完成（通常几分钟），加内部测试员即可安装。构建号取自 GitHub 的运行序号，每次自动递增。

局域网发现依赖 UDP 组播；iOS 14 起组播需要向 Apple 申请 `com.apple.developer.networking.multicast` 权限（[申请入口](https://developer.apple.com/contact/request/networking-multicast)，需登录开发者账号，填 App 名称、App Store Connect 的 Apple ID、类别、应用用途与为什么需要组播）。2026-09-08 提交申请（Request ID `HTFYV6DZUK`），**2026-09-13 获批**。状态在 developer.apple.com › Identifiers › `com.wangsheng.lansend` › **Capability Requests** 标签查看（Multicast Networking 一行）。没有这个权限时 iOS 端和沙盒版 macOS 都只能靠子网扫描和已知地址发现设备（仍可用，只是慢一些）。

获批后的启用步骤（**三步均已于 2026-09-13 完成**：App ID 已勾选 Multicast Networking，`Lan-Send Mac App Store` 描述文件已重建并更新 `MAS_PROVISIONING_PROFILE`，本地副本在 `~/Downloads/lan-send-mas/LanSend_Mac_App_Store.provisionprofile`；entitlements 已于 2026-09-13 加上 `com.apple.developer.networking.multicast`：iOS 在 `apps/app/src-tauri/gen/apple/lan-send-app_iOS/lan-send-app_iOS.entitlements`，沙盒版 macOS 在 `apps/app/src-tauri/entitlements/mas.plist`）：

1. developer.apple.com › Identifiers › `com.wangsheng.lansend` › Capabilities 勾选 **Multicast Networking** › Save（获批后该项才会出现）。App ID 的能力变了，旧的描述文件全部失效。
2. iOS：不用做别的，Release 的云端自动签名会重新生成带该能力的描述文件。
3. 沙盒版 macOS：描述文件是手动建的，必须重建——developer.apple.com › Profiles › `LanSend Mac App Store` › Edit › Save（或按下文 Mac App Store 一节第 4 步重新生成）› 下载，再更新 secret：

   ```bash
   gh secret set MAS_PROVISIONING_PROFILE --repo TgolMsk/lan-send < <(base64 -i ~/Downloads/LanSend_Mac_App_Store.provisionprofile)
   ```

   不更新的话 `app-mas` 任务在 `codesign --entitlements` 时会因描述文件缺少该权限而失败。
4. 之后跑 Release。

## Mac App Store（沙盒版，ADR-0014）

`app-mas` 任务在下列 secrets 齐全时构建沙盒版、签名、打 `.pkg` 并上传 App Store Connect；缺任何一个就跳过。Mac 应用不经 Xcode 构建，无法用 API Key 云端签名，所以证书要在 Apple 后台创建：

1. 生成两份私钥与 CSR（不必经过钥匙串访问；下面全部放在 `~/Downloads/lan-send-mas/`）：

   ```bash
   mkdir -p ~/Downloads/lan-send-mas && cd ~/Downloads/lan-send-mas
   openssl rand -base64 24 > p12-password.txt && chmod 600 p12-password.txt
   for n in distribution installer; do
     openssl req -new -newkey rsa:2048 -nodes -keyout $n.key -out $n.csr \
       -subj "/emailAddress=你的AppleID邮箱/CN=LanSend $n/C=CN"
   done
   ```

2. developer.apple.com › Certificates › ＋ › **Apple Distribution**，上传 `distribution.csr`，下载得到 `distribution.cer`；再 ＋ › **Mac Installer Distribution**，上传 `installer.csr`，下载得到 `mac_installer.cer`（改名 `installer.cer`）。
3. 下载 Apple 的中间证书 [WWDR G3](https://www.apple.com/certificateauthority/AppleWWDRCAG3.cer)，把 `.cer` 和私钥合成 `.p12`（`-legacy` 是为了让 GitHub runner 上的 `security import` 能读）：

   ```bash
   curl -sO https://www.apple.com/certificateauthority/AppleWWDRCAG3.cer
   openssl x509 -inform der -in AppleWWDRCAG3.cer -out wwdr-g3.pem
   for n in distribution installer; do
     openssl x509 -inform der -in $n.cer -out $n.pem
     openssl pkcs12 -export -legacy -inkey $n.key -in $n.pem -certfile wwdr-g3.pem \
       -passout file:p12-password.txt -out $n.p12 && chmod 600 $n.p12
   done
   ```

   本机验证：`security import distribution.p12 -P "$(cat p12-password.txt)"`（installer 同理）后，`security find-identity -v` 应列出 `Apple Distribution: …` 与 `3rd Party Mac Developer Installer: …`。
4. developer.apple.com › Profiles › ＋ › Distribution › **Mac App Store Connect** › App ID `com.wangsheng.lansend` › 选上面的 Apple Distribution 证书 › 名称 `LanSend Mac App Store` › 下载 `.provisionprofile`。
5. App Store Connect › LanSend › 左上角 App 名称旁的“添加平台”› macOS。
6. 写入 secrets（前四个含私钥或密码，由账号持有人自己运行）：

```bash
cd ~/Downloads/lan-send-mas
gh secret set MAS_CERTIFICATE_P12 --repo TgolMsk/lan-send < <(base64 -i distribution.p12)
gh secret set MAS_CERTIFICATE_PASSWORD --repo TgolMsk/lan-send < p12-password.txt
gh secret set MAS_INSTALLER_CERTIFICATE_P12 --repo TgolMsk/lan-send < <(base64 -i installer.p12)
gh secret set MAS_INSTALLER_CERTIFICATE_PASSWORD --repo TgolMsk/lan-send < p12-password.txt
gh secret set MAS_PROVISIONING_PROFILE --repo TgolMsk/lan-send < <(base64 -i LanSend_Mac_App_Store.provisionprofile)
```

证书一年后过期（到期日见 developer.apple.com › Certificates），到期前按上面步骤重做并更新 secrets；描述文件随证书一起重建。

沙盒版与 `.dmg` 版的区别：配置目录在容器里（与命令行版不共享身份和历史）；自选接收目录通过安全作用域书签保持授权（`apps/app/src-tauri/src/platform/macos.rs`）。

### 提交 Mac App Store 前必跑

```bash
scripts/mas-sandbox-check.sh   # 在临时签名的沙盒 bundle 里跑 CLI，确认接收目录是真实 ~/Downloads
```

沙盒会把 `$HOME` 重定向进容器，`files.downloads.read-write` 权限只有在代码真的写真实 `~/Downloads` 时才算"有对应功能"，否则 App Review 按 2.4.5 拒（2026-09-10 中过一次）。脚本输出 `FAIL` 就不要提交。

## Windows 签名（未接入）

`.msi` 目前未签名，SmartScreen 会提示"未知发布者"。需要时可加 Authenticode 证书步骤（`signtool`）。

## 本地打包

```bash
cd apps/app && pnpm install
pnpm tauri build                      # 当前平台的安装包，在 target/release/bundle/
pnpm tauri build --target universal-apple-darwin   # macOS 通用二进制
```
