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

iOS 不在发布流程里：没有证书无法出 `.ipa`，等 App Store Connect 的 API Key 配好后再加 TestFlight 上传（`cargo tauri ios build`）。

版本号取自标签；`0.x` 或带 `-` 的版本自动标记为预发布。发布说明取自 `CHANGELOG.md` 中对应版本的小节。

## 步骤

```bash
# 1. CHANGELOG.md 把 Unreleased 改成版本号和日期；Cargo.toml 的 version 保持一致
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

## Windows 签名（未接入）

`.msi` 目前未签名，SmartScreen 会提示"未知发布者"。需要时可加 Authenticode 证书步骤（`signtool`）。

## 本地打包

```bash
cd apps/app && pnpm install
pnpm tauri build                      # 当前平台的安装包，在 target/release/bundle/
pnpm tauri build --target universal-apple-darwin   # macOS 通用二进制
```
