# lansend_web —— LanSend 官网

LanSend 的对外官网：中文首页、English 首页，外加隐私政策与技术支持两个内容页。
版式参照 [electerm.org](https://electerm.org/)（白底、窄内容栏、分区交替底色、圆角描边卡片、
实心主按钮、首屏星座背景），配色取应用自己的品牌色（深靛蓝 + 薄荷绿）。

纯静态：没有构建依赖、没有 CDN、没有外部字体、没有统计脚本，断网也能正常显示，
直接双击 `index.html` 就能看（所有路径都是相对的）。

与仓库里原有的 `scripts/build-site.mjs` + `site/` 是两回事：那个是给 App Store 用的
最小页面（隐私政策 / 技术支持，部署在备案域名），这里是完整的产品官网。

## 目录

```
index.html            中文首页（生成物）
en/index.html         English home（生成物）
privacy/ support/     隐私政策与技术支持（由 docs/privacy.md、docs/support.md 生成）
robots.txt sitemap.xml .nojekyll
content.mjs           全部文案与配置（版本号、下载链接、App Store 地址、备案号）
build.mjs             渲染上面那些 HTML
assets/css/site.css   样式表
assets/js/site.js     移动端菜单、标签页、复制按钮、首屏星座动画
assets/img/           图标、社交分享图、各端界面截图
tools/icons.py        从应用图标生成站点图标与 favicon
tools/shots.mjs       用 apps/app 的 mock 前端重出各端界面截图（中英各一套）
tools/og.mjs          生成 1200×630 社交分享图
```

## 常用命令

```bash
node lansend_web/build.mjs                    # 改完 content.mjs 后重新生成 HTML
python3 -m http.server 4174 --directory lansend_web   # 本地预览 http://localhost:4174
```

发版后更新站点：改 `content.mjs` 里的 `config.version` 与 `config.released`，
必要时核对 `download.files` 里的文件名与体积（要和 `.github/workflows/release.yml`
的产物一致），然后重新 `node lansend_web/build.mjs`。

界面改版或加语言后重出配图：

```bash
node lansend_web/tools/shots.mjs     # 需要 apps/app 装好依赖（pnpm install）+ 本机 Chrome + cwebp
python3 lansend_web/tools/icons.py   # 只在应用图标变了以后需要
node lansend_web/tools/og.mjs        # 截图或文案变了以后
```

`tools/shots.mjs` 和商店截图（`apps/app/scripts/store-screenshots.mjs`）同源：同一份 mock
数据、同一套查询参数，只是尺寸按网页排版重新取，并且中英文各出一套（网页上按语言切换）。

## 部署

整个目录就是站点根，扔给任何能发静态文件的地方都行。完整步骤（nginx 配置、缓存与 MIME 的坑、
对象存储 / Pages / Caddy 的替代方案）写在 `docs/release.md` 的“官网：lansend_web 的部署”一节，
这里只留最短路径：

```bash
node lansend_web/build.mjs
rsync -avz --delete --exclude 'tools/' --exclude '*.mjs' --exclude 'README.md' \
  lansend_web/ <user>@<服务器>:/var/www/lansend/
```

- 域名 `https://ls.mixduo.cn` 与网站备案号 `蜀ICP备2026054850号` 记在 `content.mjs` 的 `SITE_URL` / `BEIAN`
  里，默认构建就带上。页脚印的是**网站**备案号，别和 App Store Connect 里填的 **APP** 备案号
  `蜀ICP备2026054850号-2A`（带 A 后缀）弄混。备案号按域名查表——换成别的域名（GitHub Pages 之类）页脚就不印它，
  因为备案号绑在备案域名上。
- `SITE_URL` 写进 canonical、og:url、hreflang 与 sitemap；页面之间全是相对路径，
  放在子目录（GitHub Pages 的 `/lan-send/`）也不会断。
- 三个 `--exclude` 把构建脚本挡在站外。
- 这个站自带 `/privacy/` 与 `/support/`，路径和 `site/` 一致，可以整个顶替掉它，
  App Store Connect 里填的链接不用动。
- CSS / JS 的 URL 带内容指纹，所以**只改样式或脚本也要重新构建一次**，否则 HTML 里还是旧指纹。

## 待办

- App Store 徽章用的是文字按钮，不是苹果官方徽章图；要用官方徽章得从
  Apple Marketing Resources 下载对应语言的 SVG 放进 `assets/img/`。
- 目前只有中文与英文两种语言，应用本身支持八种；要加语言就在 `content.mjs` 的
  `langs` 与 `content` 里各加一项，再用 `tools/shots.mjs` 出一套对应语言的截图。
