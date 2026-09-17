#!/usr/bin/env node
// 把 docs/ 里的 Markdown 渲染成自包含的静态站点，用于部署到备案域名 ls.mixduo.cn。
//
//   node scripts/build-site.mjs              # 输出到 site/
//   SITE_ICP='沪ICP备2026XXXXXX号' node scripts/build-site.mjs
//
// 与 GitHub Pages（tgolmsk.github.io/lan-send）共用同一份 Markdown 源文件：
// GitHub Pages 面向全球，ls.mixduo.cn 面向中国大陆（github.io 在国内访问不稳定，
// 而 App Store 的隐私政策 / 技术支持链接必须能打开）。
// 页面不引用任何 CDN、外部字体或统计脚本，全部内联，断网也能正常显示。

import { readFileSync, writeFileSync, mkdirSync, cpSync, rmSync, existsSync } from 'node:fs'
import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..')
const DOCS = join(ROOT, 'docs')
const OUT = join(ROOT, 'site')

const SITE_URL = 'https://ls.mixduo.cn'
// 工信部要求网站底部标明备案编号并链接备案系统；拿到号后填进环境变量或直接改这里的默认值。
const ICP = process.env.SITE_ICP ?? ''
// 公安联网备案（上线 30 天内办理），格式如 沪公网安备 31010502000000 号。
const POLICE = process.env.SITE_POLICE ?? ''
const POLICE_URL = process.env.SITE_POLICE_URL ?? 'https://beian.mps.gov.cn/'

const PAGES = [
  { src: 'index.md', out: 'index.html', nav: '首页 / Home' },
  { src: 'privacy.md', out: 'privacy/index.html', nav: '隐私政策 / Privacy' },
  { src: 'support.md', out: 'support/index.html', nav: '支持 / Support' },
]
const NAV = [
  { href: '/', label: '首页 / Home' },
  { href: '/privacy', label: '隐私政策 / Privacy' },
  { href: '/support', label: '支持 / Support' },
]

const escapeHtml = (s) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')

// 相对链接（privacy、support、screenshots/...）改成站点绝对路径，
// 否则 /support/ 下的 "privacy" 会解析成 /support/privacy。
const resolveHref = (href) =>
  /^(https?:|mailto:|#|\/)/.test(href) ? href : '/' + href.replace(/^\.\//, '')

function inline(text) {
  let s = escapeHtml(text)
  s = s.replace(/!\[([^\]]*)\]\(([^)\s]+)\)/g, (_, alt, src) => `<img src="${resolveHref(src)}" alt="${alt}">`)
  s = s.replace(/\[([^\]]+)\]\(([^)\s]+)\)/g, (_, label, href) => `<a href="${resolveHref(href)}">${label}</a>`)
  s = s.replace(/&lt;(https?:\/\/[^&\s]+)&gt;/g, '<a href="$1">$1</a>')
  s = s.replace(/&lt;([^@\s]+@[^&\s]+)&gt;/g, '<a href="mailto:$1">$1</a>')
  s = s.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
  s = s.replace(/`([^`]+)`/g, '<code>$1</code>')
  return s
}

function render(markdown) {
  const lines = markdown.split('\n')
  let title = 'LanSend'
  let i = 0
  if (lines[0] === '---') {
    i = 1
    for (; i < lines.length && lines[i] !== '---'; i++) {
      const m = /^title:\s*(.+)$/.exec(lines[i])
      if (m) title = m[1].trim()
    }
    i++
  }

  const html = []
  let list = null
  let para = []
  const flushPara = () => {
    if (para.length) html.push(`<p>${inline(para.join(' '))}</p>`)
    para = []
  }
  const flushList = () => {
    if (list) html.push(`<ul>\n${list.map((li) => `  <li>${inline(li)}</li>`).join('\n')}\n</ul>`)
    list = null
  }
  const flush = () => {
    flushPara()
    flushList()
  }

  for (; i < lines.length; i++) {
    const line = lines[i]
    if (!line.trim()) {
      flush()
      continue
    }
    const heading = /^(#{1,4})\s+(.*)$/.exec(line)
    if (heading) {
      flush()
      const level = heading[1].length
      html.push(`<h${level}>${inline(heading[2])}</h${level}>`)
      continue
    }
    if (/^---+$/.test(line.trim())) {
      flush()
      html.push('<hr>')
      continue
    }
    const item = /^[-*]\s+(.*)$/.exec(line)
    if (item) {
      flushPara()
      ;(list ??= []).push(item[1])
      continue
    }
    if (line.startsWith('<')) {
      // 原始 HTML 块（index.md 里的截图排版），原样输出到下一个空行为止。
      flush()
      const block = []
      for (; i < lines.length && lines[i].trim(); i++) block.push(lines[i].replace(/(src|href)="([^"]+)"/g, (_, a, v) => `${a}="${resolveHref(v)}"`))
      html.push(block.join('\n'))
      continue
    }
    flushList()
    para.push(line.trim())
  }
  flush()
  return { title, body: html.join('\n') }
}

const STYLE = `
:root {
  color-scheme: light dark;
  --bg: #ffffff; --fg: #1d1d1f; --muted: #6e6e73;
  --accent: #0b6bcb; --line: #e3e3e6; --card: #f6f6f8;
}
@media (prefers-color-scheme: dark) {
  :root { --bg: #16161a; --fg: #ececf1; --muted: #9a9aa2; --accent: #6aa9f0; --line: #2c2c33; --card: #1e1e24; }
}
* { box-sizing: border-box; }
body {
  margin: 0; background: var(--bg); color: var(--fg);
  font: 16px/1.7 -apple-system, BlinkMacSystemFont, "PingFang SC", "Microsoft YaHei", "Helvetica Neue", Arial, sans-serif;
  -webkit-text-size-adjust: 100%;
}
.wrap { max-width: 720px; margin: 0 auto; padding: 0 20px; }
header { border-bottom: 1px solid var(--line); }
header .wrap { display: flex; flex-wrap: wrap; align-items: center; gap: 8px 20px; padding-block: 16px; }
header .brand { font-weight: 600; font-size: 17px; margin-right: auto; color: var(--fg); text-decoration: none; }
header nav { display: flex; flex-wrap: wrap; gap: 16px; }
header nav a { color: var(--muted); text-decoration: none; font-size: 14px; }
header nav a:hover, header nav a[aria-current] { color: var(--accent); }
main { padding-block: 8px 40px; }
h1 { font-size: 26px; line-height: 1.35; margin: 32px 0 12px; }
h2 { font-size: 20px; margin: 28px 0 10px; }
h3 { font-size: 17px; margin: 22px 0 8px; }
p, ul { margin: 12px 0; }
ul { padding-left: 22px; }
li { margin: 6px 0; }
a { color: var(--accent); }
code { background: var(--card); padding: 1px 5px; border-radius: 4px; font-size: 90%; }
hr { border: 0; border-top: 1px solid var(--line); margin: 40px 0; }
img { max-width: 100%; height: auto; border-radius: 8px; }
footer { border-top: 1px solid var(--line); color: var(--muted); font-size: 13px; }
footer .wrap { padding-block: 20px 32px; }
footer a { color: var(--muted); }
footer p { margin: 4px 0; }
`.trim()

function beianFooter() {
  const rows = []
  if (ICP) rows.push(`<p><a href="https://beian.miit.gov.cn/" rel="nofollow noopener" target="_blank">${escapeHtml(ICP)}</a></p>`)
  if (POLICE) rows.push(`<p><a href="${POLICE_URL}" rel="nofollow noopener" target="_blank">${escapeHtml(POLICE)}</a></p>`)
  return rows.join('\n    ')
}

function page({ title, body, href }) {
  const nav = NAV.map(
    (n) => `<a href="${n.href}"${n.href === href ? ' aria-current="page"' : ''}>${n.label}</a>`,
  ).join('\n        ')
  return `<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${escapeHtml(title)}</title>
<link rel="canonical" href="${SITE_URL}${href}">
<meta name="description" content="LanSend —— 局域网文件与剪贴板互传工具，不经云端，兼容 LocalSend 协议。">
<style>
${STYLE}
</style>
</head>
<body>
<header>
  <div class="wrap">
    <a class="brand" href="/">LanSend</a>
    <nav>
        ${nav}
    </nav>
  </div>
</header>
<main class="wrap">
${body}
</main>
<footer>
  <div class="wrap">
    <p>© 2026 Wang Sheng · <a href="mailto:511297735@qq.com">511297735@qq.com</a></p>
    ${beianFooter()}
  </div>
</footer>
</body>
</html>
`
}

rmSync(OUT, { recursive: true, force: true })
mkdirSync(OUT, { recursive: true })

for (const p of PAGES) {
  const md = readFileSync(join(DOCS, p.src), 'utf8')
  const { title, body } = render(md)
  const href = p.out === 'index.html' ? '/' : '/' + p.out.replace(/\/index\.html$/, '')
  const dest = join(OUT, p.out)
  mkdirSync(dirname(dest), { recursive: true })
  writeFileSync(dest, page({ title, body, href }))
  console.log(`  ${p.src} -> site/${p.out}`)
}

if (existsSync(join(DOCS, 'screenshots'))) {
  cpSync(join(DOCS, 'screenshots'), join(OUT, 'screenshots'), { recursive: true })
  console.log('  docs/screenshots -> site/screenshots')
}

writeFileSync(join(OUT, 'robots.txt'), `User-agent: *\nAllow: /\nSitemap: ${SITE_URL}/sitemap.xml\n`)
writeFileSync(
  join(OUT, 'sitemap.xml'),
  `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n` +
    PAGES.map((p) => `  <url><loc>${SITE_URL}${p.out === 'index.html' ? '/' : '/' + p.out.replace(/\/index\.html$/, '')}</loc></url>`).join('\n') +
    `\n</urlset>\n`,
)

if (!ICP) console.log('\n注意：SITE_ICP 为空，页脚没有备案号。拿到 APP/网站备案号后用 SITE_ICP=... 重新构建。')
console.log(`\n构建完成：${OUT}`)
