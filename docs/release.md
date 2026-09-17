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

## 网站：ls.mixduo.cn（中国大陆可访问的隐私政策 / 支持页）

App Store 的隐私政策与技术支持链接必须能打开，而 `tgolmsk.github.io` 在国内访问不稳定；中国大陆上架又要求备案，备案域名 `mixduo.cn` 的解析必须指向接入商（阿里云）名下的内地服务器，**不能 CNAME 到 GitHub Pages**，否则备案会被注销。所以同一份 Markdown 出两份站点：

| 站点 | 面向 | 来源 | 部署 |
|---|---|---|---|
| <https://tgolmsk.github.io/lan-send/> | 全球 | `docs/*.md`（Jekyll） | push 到 main 自动 |
| <https://ls.mixduo.cn/> | 中国大陆 | 同样的 `docs/*.md` | `scripts/build-site.mjs` + rsync，手动 |

改文案只改 `docs/index.md` / `docs/privacy.md` / `docs/support.md`，GitHub Pages 自动更新，`ls.mixduo.cn` 要重新构建并上传。

### 构建

```bash
node scripts/build-site.mjs                       # 输出 site/（已 gitignore）
SITE_ICP='蜀ICP备2026054850号' node scripts/build-site.mjs   # 带备案号
```

零依赖（只用 Node 标准库），渲染 `docs/` 里那三页 Markdown 成自包含 HTML：CSS 内联，不引用任何 CDN、外部字体或统计脚本，深浅色自适应，手机端单栏。输出 `index.html`、`privacy/index.html`、`support/index.html`、`screenshots/`、`robots.txt`、`sitemap.xml`，目录式路径让任何静态服务器都能直接用 `/privacy`、`/support`。

环境变量：

- `SITE_ICP` —— 工信部备案编号，渲染在页脚并链接 <https://beian.miit.gov.cn/>（备案要求网站底部标明并可查询）。留空则不渲染该行，构建时会提示。
- `SITE_POLICE` / `SITE_POLICE_URL` —— 公安联网备案编号（上线 30 天内办理），同样渲染在页脚。

拿到备案号后把它写进部署命令或 CI 的环境变量，别只存在某个人的 shell 历史里。

### 部署

```bash
node scripts/build-site.mjs
rsync -avz --delete site/ <user>@<内地服务器>:/var/www/ls.mixduo.cn/
```

nginx：

```nginx
server {
    listen 80;
    server_name ls.mixduo.cn;
    return 301 https://$host$request_uri;
}

server {
    listen 443 ssl;
    http2 on;
    server_name ls.mixduo.cn;

    ssl_certificate     /etc/nginx/ssl/ls.mixduo.cn.pem;
    ssl_certificate_key /etc/nginx/ssl/ls.mixduo.cn.key;

    root /var/www/ls.mixduo.cn;
    index index.html;
    charset utf-8;

    location / {
        try_files $uri $uri/ $uri.html =404;
    }

    location /screenshots/ {
        expires 7d;
    }
}
```

证书用阿里云的免费 DV 证书或 certbot 都行；App Store Connect 里的链接必须是 https。

### 上架中国大陆时要同步改的

1. 二级域名 `ls.mixduo.cn` 解析到内地服务器（备案在 `mixduo.cn` 主域名下，子域名无需单独备案）。
2. App Store Connect › App 信息 › 隐私政策 URL，简体中文本地化填 <https://ls.mixduo.cn/privacy>；版本页的技术支持 URL 填 <https://ls.mixduo.cn/support>（其他语言可继续用 GitHub Pages，或一并换成新域名）。
3. 阿里云备案表里"具体使用的域名"填 `mixduo.cn`，这两页就是该域名提供的服务内容，前后自洽。
4. 备案通过后用 `SITE_ICP=...` 重新构建上传，页脚出现备案号再去 ASC 填写备案号字段。

## 官网：lansend_web 的部署

`lansend_web/` 是完整的产品官网（中英首页 + 隐私政策 + 技术支持），纯静态、路径全相对，
放到任何能发静态文件的地方都能跑，不需要 Node、不需要数据库。

它已经**包含** `/privacy/` 与 `/support/` 两页（从同一份 `docs/*.md` 生成），路径和
`scripts/build-site.mjs` 出的 `site/` 完全一致，所以可以整个顶替掉 `site/`，
App Store Connect 里填的 <https://ls.mixduo.cn/privacy> 与 `/support` 不用改。

### 1. 构建

```bash
node lansend_web/build.mjs
```

- 域名 `https://ls.mixduo.cn` 与网站备案号 `蜀ICP备2026054850号` 已经记在 `lansend_web/content.mjs`
  里，默认构建就带上，不用再传环境变量。页脚印的是**网站**备案号；App Store Connect 里
  填的是 **APP** 备案号 `蜀ICP备2026054850号-2A`（带 A 后缀的那条），两者不是一回事。
- `SITE_URL` 写进 canonical、`og:url`、`hreflang` 与 `sitemap.xml`；页面之间全是相对路径，
  所以放在子目录（如 GitHub Pages 的 `/lan-send/`）也不会断。
- **备案号按域名查表**（`content.mjs` 里的 `BEIAN`）：备案号绑在备案域名上，`SITE_URL`
  一旦换成别的域名，页脚就不会再印它。公安联网备案（上线 30 天内办）拿到号后填进同一张表的
  `police` 字段。`SITE_ICP` / `SITE_POLICE` 仍可临时覆盖。
- CSS 与 JS 的 URL 带内容指纹（`site.css?v=43ca6a47`），所以改了样式或脚本也要重新构建，
  否则 HTML 里还是旧指纹。

### 2. 上传

```bash
rsync -avz --delete \
  --exclude 'tools/' --exclude '*.mjs' --exclude 'README.md' \
  lansend_web/ <user>@<服务器>:/var/www/lansend/
```

三个 `--exclude` 把构建脚本挡在站外（没有密钥，但没必要公开）。`--delete` 会清掉目标目录里
多余的文件，第一次跑之前确认路径没写错。

### 3. nginx

```nginx
server {
    listen 80;
    listen [::]:80;
    server_name ls.mixduo.cn;
    return 301 https://$host$request_uri;
}

server {
    listen 443 ssl;
    listen [::]:443 ssl;
    http2 on;
    server_name ls.mixduo.cn;

    ssl_certificate     /etc/letsencrypt/live/ls.mixduo.cn/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/ls.mixduo.cn/privkey.pem;

    root /var/www/lansend;
    index index.html;
    charset utf-8;

    # 目录式路径：/privacy -> /privacy/index.html，/en -> /en/index.html
    location / {
        try_files $uri $uri/ =404;
    }

    # HTML 每次回源校验：版本号、下载链接都写在页面里
    location ~* \.html$ { expires -1; }
    # 带指纹的样式表与脚本可以长缓存
    location ~* \.(css|js)$ { expires 30d; }
    # 图片文件名固定（重出配图不改名），给一周
    location ^~ /assets/img/ { expires 7d; }

    gzip on;
    gzip_min_length 1024;
    gzip_types text/css application/javascript application/xml image/svg+xml text/plain;

    add_header X-Content-Type-Options nosniff always;
}
```

几个容易踩的点：

- **别在 `location` 里写 `add_header`**。nginx 的 `add_header` 一旦出现在子层级，会**丢掉**父层级
  所有的 `add_header`，`nosniff` 就没了。缓存用 `expires` 指令设置，不受这个规则影响。
- **确认 `image/webp` 在 MIME 表里**：界面截图全是 WebP，配了 `nosniff` 之后类型发错浏览器就不显示了。
  `grep webp /etc/nginx/mime.types` 有输出就行（nginx 1.11.6 起自带）；没有就在 server 块里补
  `types { image/webp webp; }`。
- 证书用 certbot 最省事：`sudo certbot --nginx -d ls.mixduo.cn`，它会自己改上面的 ssl 两行并配好续期。
  App Store Connect 里的链接必须是 https。
- 内容变了记得让 CDN 刷新（阿里云 CDN / Cloudflare 都要手动刷 `/index.html`、`/en/index.html`）。

### 其它落地方式

| 方式 | 适合 | 做法 |
|---|---|---|
| Caddy | 想省掉证书配置 | Caddyfile 两行：`ls.mixduo.cn { root * /var/www/lansend<br>file_server }`，证书自动申请续期 |
| 对象存储 + CDN（阿里云 OSS / 腾讯 COS） | 不想维护服务器 | 上传整个目录，开静态网站托管，默认首页 `index.html`、默认 404 页留空；注意子目录索引要开，否则 `/privacy` 404 |
| GitHub Pages | 全球访问、免备案 | Pages 的源目录只能是仓库根或 `/docs`，所以要加一个 workflow 把 `lansend_web/` 当 artifact 上传（`actions/upload-pages-artifact` + `actions/deploy-pages`）；会和现在 `docs/` 的 Jekyll 站冲突，二选一 |
| Cloudflare Pages / Vercel | 想要自动部署 | 连仓库，构建命令 `node lansend_web/build.mjs`，输出目录 `lansend_web` |

备案域名（`mixduo.cn` 及其子域名）**必须**解析到接入商名下的内地服务器，不能指向 GitHub Pages /
Cloudflare，否则备案会被注销——这条见上一节。

### 4. 发版后更新站点

```bash
# 1. 改版本号与下载文件信息
$EDITOR lansend_web/content.mjs          # config.version / config.released / download.files
# 2. 重新构建并上传
node lansend_web/build.mjs
rsync -avz --delete --exclude 'tools/' --exclude '*.mjs' --exclude 'README.md' \
  lansend_web/ <user>@<服务器>:/var/www/lansend/
```

界面改版之后还要重出配图（`node lansend_web/tools/shots.mjs`，需要 `apps/app` 的依赖 + 本机 Chrome + cwebp），
详见 `lansend_web/README.md`。
