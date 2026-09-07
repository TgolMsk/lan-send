# 发布流程

打 `v<版本>` 标签即触发 `.github/workflows/release.yml`：三端构建安装包并发布到 GitHub Releases。`Actions › Release › Run workflow` 可手动跑一次构建（勾选 publish 才发布）。

## 产物

| 平台 | 文件 | 说明 |
|---|---|---|
| macOS | `lan-send-<ver>-macos-universal.pkg` | arm64 + x86_64 通用二进制，安装到 `/usr/local/bin`；有证书时自动签名并公证 |
| macOS | `lan-send-<ver>-macos-universal.tar.gz` | 免安装 |
| Windows | `lan-send-<ver>-windows-x86_64.msi` | 安装到 Program Files 并加入 PATH（WiX，`crates/cli/wix/main.wxs`） |
| Windows | `lan-send-<ver>-windows-{x86_64,arm64}.zip` | 免安装 |
| Linux | `lan-send-<ver>-linux-{x86_64,aarch64}.tar.gz` | 免安装（Linux 只作测试用途） |
| 全部 | `SHA256SUMS.txt` | 校验和 |

版本号取自标签；`0.x` 或带 `-` 的版本自动标记为预发布。发布说明取自 `CHANGELOG.md` 中对应版本的小节。

## 步骤

```bash
# 1. CHANGELOG.md 把 Unreleased 改成版本号和日期；Cargo.toml 的 version 保持一致
# 2. 提交后打标签并推送
git tag v0.1.0
git push origin v0.1.0
```

## macOS 签名与公证（可选）

在仓库 `Settings › Secrets and variables › Actions` 配置以下 secrets 后，macOS 产物会自动签名并公证；没有时产出未签名包，用户首次打开需要在"系统设置 › 隐私与安全性"里允许。

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

## Tauri 应用

里程碑 5 之后由 `tauri-action` 产出 `.dmg` / `.msi` / iOS 包，会加入同一工作流。
