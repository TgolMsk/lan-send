# 0012. 安装包构建与发布

- 状态：已接受
- 日期：2026-09-07

## 背景

简报要求 Windows 产物通过 GitHub Actions 构建；用户要求在 GitHub 上构建安装包并发布。目前只有 CLI（Tauri 应用在里程碑 5），原 `release.yml` 只上传裸二进制，不能"双击安装"。

## 决策

1. **触发**：推送 `v*` 标签 → 构建并发布到 GitHub Releases；`workflow_dispatch` 只构建（勾选 `publish` 才发布）。版本号取自标签，手动运行时为 `<Cargo 版本>-dev.<sha>`。
2. **macOS**：分别编译 arm64 与 x86_64，`lipo` 合成通用二进制，`pkgbuild` 生成 `.pkg`（identifier `dev.lan-send.cli`，安装到 `/usr/local/bin`），另附 `.tar.gz`。配置了 Developer ID 证书 secrets 时自动 `codesign --options runtime --timestamp`、签名 `.pkg`、`notarytool` 公证并 staple；没有则产出未签名包并在文档说明。
3. **Windows**：`cargo-wix`（WiX 3，runner 预装）生成 `.msi`，安装到 `Program Files\lan-send\bin` 并加入系统 PATH；`UpgradeCode` 固定在 `crates/cli/wix/main.wxs` 以支持升级安装。另附 x86_64 与 arm64 `.zip`。暂不做 Authenticode 签名。
4. **Linux**：只出 `.tar.gz`（x86_64、aarch64），测试用途。
5. **发布**：`SHA256SUMS.txt`；发布说明取 `CHANGELOG.md` 中对应版本小节；`0.x` 或带 `-` 的版本标记为预发布。
6. **Tauri 应用**（里程碑 5）用 `tauri-action` 加入同一工作流：macOS `.dmg`、Windows `.msi`、iOS 走 TestFlight。

## 备选方案

- `cargo-dist`：一次生成多平台安装器与 Homebrew 公式，但自带一整套模板与更新流程；当前只有一个二进制，手写工作流更透明，Tauri 阶段也用不上它。
- macOS 用 `.dmg`：适合 `.app`，命令行工具用 `.pkg` 才能装进 PATH。
- Homebrew tap / winget / scoop：等发布节奏稳定后再加。

## 后果

- 未签名的 macOS `.pkg` 首次打开需在"隐私与安全性"里允许；需要的 secrets 列在 `docs/release.md`。
- WiX 3 依赖 runner 镜像预装；若镜像移除需改为 `choco install wixtoolset`。
