#!/usr/bin/env node
// 把 content.mjs 的文案渲染成 lansend_web/ 下的静态页面。
//
//   node lansend_web/build.mjs
//   SITE_URL=https://lansend.app SITE_ICP='沪ICP备2026XXXXXX号' node lansend_web/build.mjs
//
// 产出（全部是纯静态文件，相对路径，直接双击 index.html 也能看）：
//   index.html            中文首页
//   en/index.html         English home
//   privacy/index.html    隐私政策（从 docs/privacy.md 转换）
//   support/index.html    技术支持（从 docs/support.md 转换）
//   robots.txt · sitemap.xml · .nojekyll
//
// 配图在 assets/img/：图标由 tools/icons.py 生成，界面截图由 tools/shots.mjs 生成。
// 页面不引用任何 CDN、外部字体或统计脚本。

import { createHash } from 'node:crypto'
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

import { config, content, langs } from './content.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DOCS = join(HERE, '../docs')
const YEAR = 2026

// 样式表和脚本的文件名是固定的，加个内容指纹，服务器才敢给 assets/ 配长缓存。
// 改了 CSS / JS 也要重新构建一次，指纹写在 HTML 里。
const fingerprint = (rel) => createHash('sha256').update(readFileSync(join(HERE, rel))).digest('hex').slice(0, 8)
const V = { css: fingerprint('assets/css/site.css'), js: fingerprint('assets/js/site.js') }

const esc = (s) => String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;')
// 文案里的 `code`、**粗体**、[链接](url) —— 只支持这三种，够用。
const rich = (s) =>
  esc(s)
    .replace(/\[([^\]]+)\]\(([^)\s]+)\)/g, '<a href="$2">$1</a>')
    .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
    .replace(/`([^`]+)`/g, '<code>$1</code>')

/* ---------------------------------------------------------------- 图标 */

const PATHS = {
  link: 'M9.5 14.5l5-5M10.6 7.4l1.7-1.7a3.9 3.9 0 115.5 5.5l-1.7 1.7M13.4 16.6l-1.7 1.7a3.9 3.9 0 11-5.5-5.5l1.7-1.7',
  'cloud-off': 'M7.5 18.5h9.5a3.4 3.4 0 000-6.8 5.2 5.2 0 00-9.8-1.4 3.8 3.8 0 00.3 8.2zM3.5 3.5l17 17',
  devices: 'M3 6.5h12v8.5H3zM2 18.5h14M18.5 9.5h3v9h-3zM19.4 16.6h1.2',
  shield: 'M12 3.2l7.3 2.9v5c0 4.3-2.9 8-7.3 9.5-4.4-1.5-7.3-5.2-7.3-9.5v-5zM9 11.8l2.1 2.1 4-4',
  clipboard: 'M9.2 3.8h5.6v3.1H9.2zM9.2 5.4H7.4A1.6 1.6 0 005.8 7v12.2a1.6 1.6 0 001.6 1.6h9.2a1.6 1.6 0 001.6-1.6V7a1.6 1.6 0 00-1.6-1.6h-1.8',
  resume: 'M3.6 12a8.4 8.4 0 103-6.4M3.4 4v5h5',
  folder: 'M3.4 7.6A1.6 1.6 0 015 6h3.7l2 2.5h7.3a1.6 1.6 0 011.6 1.6v8a1.6 1.6 0 01-1.6 1.6H5a1.6 1.6 0 01-1.6-1.6z',
  image: 'M4 5.2h16v13.6H4zM4 15.6l4.6-4.6 4 4 3-3L20 15.6M9.6 9.4a1.3 1.3 0 11-2.6 0 1.3 1.3 0 012.6 0z',
  history: 'M20.4 12a8.4 8.4 0 11-16.8 0 8.4 8.4 0 0116.8 0zM12 7.2V12l3.2 1.9',
  terminal: 'M4.5 6.5l5 5.2-5 5.2M12.5 17h7',
  globe: 'M20.4 12a8.4 8.4 0 11-16.8 0 8.4 8.4 0 0116.8 0zM3.8 12h16.4M12 3.6c2.6 3.2 2.6 13.6 0 16.8M12 3.6c-2.6 3.2-2.6 13.6 0 16.8',
  bolt: 'M13.2 3L5.4 13.2h5.6L10 21l8.2-10.4h-5.6z',
  laptop: 'M5 6.4h14v9.2H5zM2.8 18.4h18.4',
  window: 'M4 5.4h16v13.2H4zM4 9.6h16M9.2 9.6v9',
  download: 'M12 4v11M7.4 10.8L12 15.4l4.6-4.6M5 19.4h14',
  github:
    'M9 19.2c-4.3 1.3-4.3-2.2-6-2.7m12 4.7v-3.3c0-.9.1-1.3-.5-1.9 2.7-.3 5.4-1.3 5.4-5.8a4.5 4.5 0 00-1.3-3.1 4.1 4.1 0 00-.1-3.1s-1-.3-3.4 1.2a11.7 11.7 0 00-6.2 0C6.5 3.7 5.5 4 5.5 4a4.1 4.1 0 00-.1 3.1 4.5 4.5 0 00-1.3 3.1c0 4.5 2.7 5.5 5.4 5.8-.6.6-.6 1.2-.5 1.9v3.3',
  menu: 'M4 7h16M4 12h16M4 17h16',
  external: 'M7.5 16.5l9-9M9 7.5h7.5V15',
  check: 'M5.5 12.5l4.5 4.5 8.5-9',
  apple: 'M16.2 12.6c0-2.4 2-3.5 2.1-3.6-1.1-1.7-2.9-1.9-3.5-1.9-1.5-.2-2.9.9-3.7.9-.8 0-1.9-.9-3.1-.8-1.6 0-3.1.9-3.9 2.4-1.7 2.9-.4 7.2 1.2 9.5.8 1.2 1.7 2.4 3 2.4 1.2 0 1.7-.8 3.1-.8s1.9.8 3.1.8c1.3 0 2.1-1.2 2.9-2.3.9-1.3 1.3-2.6 1.3-2.7-.1 0-2.5-1-2.5-3.9zM14 5.6c.7-.8 1.1-1.9 1-3-1 0-2.2.7-2.9 1.5-.6.7-1.2 1.9-1 3 1.1.1 2.2-.6 2.9-1.5z',
}

const icon = (name, cls = '') => {
  const d = PATHS[name]
  if (!d) throw new Error(`没有这个图标：${name}`)
  const fill = name === 'apple'
  return `<svg${cls ? ` class="${cls}"` : ''} viewBox="0 0 24 24" aria-hidden="true" fill="${fill ? 'currentColor' : 'none'}"${
    fill ? '' : ' stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"'
  }><path d="${d}"/></svg>`
}

const kindIcon = { dmg: 'laptop', zip: 'laptop', pkg: 'laptop', exe: 'window', msi: 'window', tar: 'terminal' }

/* ------------------------------------------------------------ 页面骨架 */

// base：从当前页面回到站点根的相对前缀（首页 ''，/en/ 与 /privacy/ 是 '../'）。
const shell = ({ lang, base, href, title, description, body, bodyClass = '' }) => {
  const t = content[lang]
  const L = langs[lang]
  const other = langs[L.other]
  const url = `${config.siteUrl}${href}`
  const asset = (p) => `${base}assets/${p}`
  return `<!doctype html>
<html lang="${L.htmlLang}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${esc(title)}</title>
<meta name="description" content="${esc(description)}">
<meta name="theme-color" content="#14144a">
<link rel="canonical" href="${url}">
<link rel="alternate" hreflang="${L.htmlLang}" href="${config.siteUrl}${href}">
<link rel="alternate" hreflang="${other.htmlLang}" href="${config.siteUrl}/${other.dir ? `${other.dir}/` : ''}">
<link rel="icon" href="${asset('img/favicon.png')}" type="image/png">
<link rel="apple-touch-icon" href="${asset('img/apple-touch-icon.png')}">
<meta property="og:type" content="website">
<meta property="og:site_name" content="LanSend">
<meta property="og:title" content="${esc(title)}">
<meta property="og:description" content="${esc(description)}">
<meta property="og:url" content="${url}">
<meta property="og:image" content="${config.siteUrl}/assets/img/${lang === 'zh' ? 'og.png' : 'og-en.png'}">
<meta property="og:image:width" content="1200">
<meta property="og:image:height" content="630">
<meta property="og:image:alt" content="${esc(t.meta.ogAlt)}">
<meta name="twitter:card" content="summary_large_image">
<link rel="stylesheet" href="${asset('css/site.css')}?v=${V.css}">
</head>
<body${bodyClass ? ` class="${bodyClass}"` : ''}>
${body}
<script src="${asset('js/site.js')}?v=${V.js}" defer></script>
</body>
</html>
`
}

const header = ({ lang, base, nav, showLang = true }) => {
  const t = content[lang]
  const L = langs[lang]
  const other = langs[L.other]
  const otherHref = other.dir ? `${base}${other.dir}/` : base || './'
  const home = base || './'
  return `<a class="skip" href="#main">${lang === 'zh' ? '跳到正文' : 'Skip to content'}</a>
<header class="site-header">
  <div class="wrap header-inner">
    <a class="brand" href="${home}"><img src="${base}assets/img/icon-128.png" width="28" height="28" alt=""><span>LanSend</span></a>
    <nav class="nav" id="site-nav" hidden>
      ${nav.map(([href, label]) => `<a href="${href}">${esc(label)}</a>`).join('\n      ')}
    </nav>
    <div class="header-side">
      ${showLang ? `<a class="lang-switch" href="${otherHref}" hreflang="${other.htmlLang}" rel="alternate">${esc(t.header.langSwitch)}</a>` : ''}
      <a class="header-ghost" href="${config.repo}" rel="noopener">${icon('github')}<span>${esc(t.header.github)}</span></a>
      <button class="nav-toggle" type="button" aria-expanded="false" aria-controls="site-nav" aria-label="${esc(t.header.menu)}">${icon('menu')}</button>
    </div>
  </div>
</header>`
}

const footer = ({ lang, base }) => {
  const t = content[lang]
  const beian = []
  if (config.icp)
    beian.push(`<p><a href="https://beian.miit.gov.cn/" rel="nofollow noopener" target="_blank">${esc(config.icp)}</a></p>`)
  if (config.police)
    beian.push(`<p><a href="${config.policeUrl}" rel="nofollow noopener" target="_blank">${esc(config.police)}</a></p>`)
  return `<footer class="site-footer">
  <div class="wrap">
    <div class="footer-top">
      <div>
        <span class="footer-brand"><img src="${base}assets/img/icon-128.png" width="26" height="26" alt="">LanSend</span>
        <p>${esc(t.footer.tagline)}</p>
      </div>
      <div class="footer-links">
        <a href="${config.repo}" rel="noopener">GitHub</a>
        <a href="${config.releases}" rel="noopener">${lang === 'zh' ? '下载' : 'Downloads'}</a>
        <a href="${base}support/">${esc(t.footer.support)}</a>
        <a href="${base}privacy/">${esc(t.footer.privacy)}</a>
        <a href="mailto:${config.email}">${esc(config.email)}</a>
      </div>
    </div>
    <div class="footer-bottom">
      <p>${esc(t.footer.rights(YEAR, config.author))} · <a href="${config.repo}/blob/main/LICENSE" rel="noopener">${esc(t.footer.license)}</a></p>
      <p>${t.footer.credit}</p>
      ${beian.join('\n      ')}
    </div>
  </div>
</footer>`
}

/* -------------------------------------------------------------- 首页 */

const heroSection = (lang, base) => {
  const t = content[lang].hero
  const shots = `${base}assets/img/shots/${lang}`
  return `<section class="hero" id="top">
  <canvas class="hero-net" aria-hidden="true"></canvas>
  <div class="wrap hero-inner">
    <a class="chip" href="${config.releaseTag(config.version)}" rel="noopener"><span class="dot"></span>${esc(
      t.chip(config.version, config.released),
    )}<span class="sep">·</span>${esc(t.chipHint)}</a>
    <h1 class="wordmark"><img src="${base}assets/img/icon-256.png" width="84" height="84" alt="LanSend"><span>LanSend</span></h1>
    <p class="hero-lead">${esc(t.lead)}</p>
    <p class="hero-sub">${esc(t.sub)}</p>
    <div class="hero-cta">
      <a class="btn btn-primary" href="#download">${icon('download')}${esc(t.download(config.version))}</a>
      <a class="btn" href="${config.appStore}" rel="noopener">${icon('apple')}${esc(t.appStore)}</a>
      <a class="btn" href="${config.repo}" rel="noopener">${icon('github')}${esc(t.source)}</a>
    </div>
    <p class="hero-meta">${esc(t.meta)}</p>
  </div>
  <div class="wrap hero-shot">
    <figure class="frame frame-desktop">
      <div class="frame-bar"><i></i><i></i><i></i><span>${esc(t.windowTitle)}</span></div>
      <img src="${shots}/desktop/1-devices.webp" width="1360" height="780" alt="${esc(t.shotAlt)}" fetchpriority="high">
    </figure>
    <figure class="frame frame-phone">
      <img src="${shots}/iphone/2-incoming.webp" width="390" height="844" alt="${esc(t.phoneAlt)}" loading="lazy">
    </figure>
  </div>
</section>`
}

const sectionHead = (t) =>
  `<div class="section-head">
    <span class="eyebrow">${esc(t.eyebrow)}</span>
    <h2>${esc(t.title)}</h2>
    ${t.sub ? `<p>${esc(t.sub)}</p>` : ''}
  </div>`

const featuresSection = (lang) => {
  const t = content[lang].features
  return `<section id="features">
  <div class="wrap">
    ${sectionHead(t)}
    <div class="card-grid">
      ${t.items
        .map(
          ([ico, title, body]) => `<article class="feature">
        <span class="ico">${icon(ico)}</span>
        <h3>${esc(title)}</h3>
        <p>${rich(body)}</p>
      </article>`,
        )
        .join('\n      ')}
    </div>
  </div>
</section>`
}

const screensSection = (lang, base) => {
  const t = content[lang].screens
  const shots = `${base}assets/img/shots/${lang}`
  const frameFor = { desktop: 'frame-desktop', iphone: 'frame-phone', ipad: 'frame-tablet' }
  const sizeFor = { desktop: [1360, 780], iphone: [390, 844], ipad: [1032, 1376] }
  const panel = (device, i) => {
    const [w, h] = sizeFor[device]
    const figures = t.shots[device]
      .map((name) => {
        const [title, caption] = t.captions[device][name]
        const bar =
          device === 'desktop'
            ? `<div class="frame-bar"><i></i><i></i><i></i><span>LanSend — ${esc(title)}</span></div>`
            : ''
        return `<figure class="shot">
          <div class="frame ${frameFor[device]}">${bar}<img src="${shots}/${device}/${name}.webp" width="${w}" height="${h}" alt="${esc(
            `${title} — ${caption}`,
          )}" loading="lazy" decoding="async"></div>
          <figcaption><b>${esc(title)}</b>${esc(caption)}</figcaption>
        </figure>`
      })
      .join('\n        ')
    return `<div class="panel panel-${device}" id="screens-${device}" role="tabpanel" aria-labelledby="tab-screens-${device}"${
      i ? ' hidden' : ''
    }>
      <div class="strip">
        ${figures}
      </div>
    </div>`
  }
  return `<section id="screens" class="alt">
  <div class="wrap">
    ${sectionHead(t)}
    <div class="tabs" data-tabs role="tablist" aria-label="${esc(t.title)}">
      ${t.tabs
        .map(
          ([id, label], i) =>
            `<button type="button" role="tab" id="tab-screens-${id}" aria-controls="screens-${id}" aria-selected="${
              i === 0
            }" tabindex="${i === 0 ? 0 : -1}">${esc(label)}</button>`,
        )
        .join('\n      ')}
    </div>
    ${t.tabs.map(([id], i) => panel(id, i)).join('\n    ')}
  </div>
</section>`
}

// 手画的示意图：两台设备连同一个 Wi‑Fi，文件沿着它们之间的直连飞过去，云被划掉。
const diagram = (lang) => {
  const d = content[lang].how.diagram
  return `<div class="diagram">
  <svg viewBox="0 0 880 320" role="img" aria-label="${esc(
    `${d.phone} ↔ ${d.router} ↔ ${d.laptop} · ${d.direct} · ${d.cloud}`,
  )}">
    <defs>
      <linearGradient id="lansend-screen" x1="0" y1="0" x2="1" y2="1">
        <stop offset="0" stop-color="#14144a"/><stop offset="1" stop-color="#4444a3"/>
      </linearGradient>
    </defs>

    <path class="d-cloud" d="M78 62h44a17 17 0 001-34 25 25 0 00-46-7 19 19 0 001 41z"/>
    <path class="d-slash" d="M60 68l64-56"/>
    <text class="d-label d-start" x="152" y="52">${esc(d.cloud)}</text>

    <rect class="d-body" x="50" y="118" width="86" height="168" rx="17"/>
    <rect class="d-screen" x="58" y="126" width="70" height="152" rx="11"/>
    <path class="d-screen-line" d="M70 152h46M70 168h30M70 184h46"/>
    <text class="d-name" x="93" y="308">${esc(d.phone)}</text>

    <path class="d-body" d="M648 268h204l10 15H638z"/>
    <rect class="d-body" x="668" y="160" width="164" height="108" rx="9"/>
    <rect class="d-screen" x="676" y="168" width="148" height="92" rx="5"/>
    <path class="d-screen-line" d="M692 194h60M692 210h96M692 226h44"/>
    <text class="d-name" x="750" y="308">${esc(d.laptop)}</text>

    <path class="d-wifi" d="M412 192a40 40 0 0156 0M424 202a24 24 0 0132 0"/>
    <path class="d-wire" d="M414 216l-6-18M466 216l6-18"/>
    <rect class="d-body" x="388" y="216" width="104" height="40" rx="11"/>
    <circle class="d-led" cx="408" cy="236" r="4"/>
    <path class="d-wire" d="M424 236h52"/>
    <text class="d-label" x="440" y="280">${esc(d.router)}</text>

    <path class="d-hop" d="M138 212h248M494 234h172"/>
    <path class="d-flow" d="M136 160Q440 60 668 172"/>
    <g class="d-pill">
      <rect x="348" y="98" width="184" height="30" rx="15"/>
      <text x="440" y="118">${esc(d.direct)}</text>
    </g>
  </svg>
</div>`
}

const howSection = (lang) => {
  const t = content[lang].how
  return `<section id="how">
  <div class="wrap">
    ${sectionHead(t)}
    ${diagram(lang)}
    <div class="steps">
      ${t.steps
        .map(
          ([title, body], i) => `<article class="step">
        <span class="n">${i + 1}</span>
        <h3>${esc(title)}</h3>
        <p>${esc(body)}</p>
      </article>`,
        )
        .join('\n      ')}
    </div>
    <p class="note">${rich(t.note)}</p>
  </div>
</section>`
}

const downloadSection = (lang, base) => {
  const t = content[lang].download
  const v = config.version
  const fileCard = ([kind, template, label, size, recommended]) => {
    const name = template.replace('{v}', v)
    return `<a class="file" href="${config.asset(v, name)}" rel="noopener">
        <span class="ico">${icon(kindIcon[kind])}</span>
        <span><b>${esc(label)}</b><small>${esc(name)} · ${esc(size)}</small></span>
        ${recommended ? `<span class="tag">${esc(t.recommended)}</span>` : ''}
      </a>`
  }
  const panel = (id, i, inner) =>
    `<div class="panel" id="download-${id}" role="tabpanel" aria-labelledby="tab-download-${id}"${i ? ' hidden' : ''}>
      ${inner}
      <p class="dl-note">${esc(t.notes[id])}</p>
    </div>`
  const filePanel = (id, i) => panel(id, i, `<div class="files">\n      ${t.files[id].map(fileCard).join('\n      ')}\n      </div>`)
  const iosPanel = (i) =>
    panel(
      'ios',
      i,
      `<div class="appstore">
        <img src="${base}assets/img/icon-256.png" width="96" height="96" alt="">
        <div class="body">
          <h3>${esc(t.ios.title)}</h3>
          <p>${esc(t.ios.body)}</p>
          <a class="btn btn-primary" href="${config.appStore}" rel="noopener">${icon('apple')}${esc(t.ios.badge)}</a>
          <p class="fine"><a href="${config.asset(v, `lan-send-${v}-ios.ipa`)}" rel="noopener">${esc(t.ios.ipa)}</a></p>
        </div>
      </div>`,
    )
  return `<section id="download" class="alt">
  <div class="wrap">
    <div class="section-head">
      <span class="eyebrow">${esc(t.eyebrow)}</span>
      <h2>${esc(t.title(v))}</h2>
      <p>${esc(t.sub)}</p>
    </div>
    <div class="tabs" data-tabs role="tablist" aria-label="${esc(t.eyebrow)}">
      ${t.tabs
        .map(
          ([id, label], i) =>
            `<button type="button" role="tab" id="tab-download-${id}" aria-controls="download-${id}" aria-selected="${
              i === 0
            }" tabindex="${i === 0 ? 0 : -1}">${esc(label)}</button>`,
        )
        .join('\n      ')}
    </div>
    ${filePanel('macos', 0)}
    ${filePanel('windows', 1)}
    ${iosPanel(2)}
    ${filePanel('cli', 3)}
    <p class="dl-links">
      <a href="${config.releases}" rel="noopener">${esc(t.all)}${icon('external')}</a>
      <a href="${config.asset(v, 'SHA256SUMS.txt')}" rel="noopener">${esc(t.checksums)}</a>
    </p>
  </div>
</section>`
}

const cliSection = (lang) => {
  const t = content[lang].cli
  return `<section id="cli">
  <div class="wrap">
    ${sectionHead(t)}
    <div class="terminal">
      <div class="frame-bar"><i></i><i></i><i></i><span>lan-send</span></div>
      <ol>
        ${t.lines
          .map(
            ([cmd, what]) => `<li>
          <span class="cmd">${esc(cmd)}<span class="what">${esc(what)}</span></span>
          <button class="copy" type="button" data-copy="${esc(cmd)}" data-done="${esc(t.copied)}">${esc(t.copy)}</button>
        </li>`,
          )
          .join('\n        ')}
      </ol>
    </div>
    <p class="cli-more">${rich(t.more)}</p>
  </div>
</section>`
}

const faqSection = (lang) => {
  const t = content[lang].faq
  return `<section id="faq" class="alt">
  <div class="wrap">
    ${sectionHead(t)}
    <div class="faq">
      ${t.items
        .map(
          ([q, a]) => `<details>
        <summary>${esc(q)}</summary>
        <p>${rich(a)}</p>
      </details>`,
        )
        .join('\n      ')}
    </div>
  </div>
</section>`
}

const linksSection = (lang) => {
  const t = content[lang].links
  return `<section id="links">
  <div class="wrap">
    ${sectionHead(t)}
    <div class="link-cols">
      ${t.groups
        .map(
          ([title, items]) => `<div>
        <h3>${esc(title)}</h3>
        <ul>
          ${items.map(([label, href]) => `<li><a href="${href}">${esc(label)}</a></li>`).join('\n          ')}
        </ul>
      </div>`,
        )
        .join('\n      ')}
    </div>
  </div>
</section>`
}

const homePage = (lang) => {
  const base = langs[lang].dir ? '../' : ''
  const t = content[lang]
  const href = langs[lang].dir ? `/${langs[lang].dir}/` : '/'
  const body = `${header({ lang, base, nav: t.nav })}
<main id="main">
${heroSection(lang, base)}
${featuresSection(lang)}
${screensSection(lang, base)}
${howSection(lang)}
${downloadSection(lang, base)}
${cliSection(lang)}
${faqSection(lang)}
${linksSection(lang)}
</main>
${footer({ lang, base })}`
  return shell({ lang, base, href, title: t.meta.title, description: t.meta.description, body })
}

/* ------------------------------------------- Markdown（隐私政策 / 支持） */

// docs/*.md 里的相对链接是按站点根写的（privacy、support）；内容页在 /privacy/ 下，
// 原样输出会变成 /support/privacy，所以补成上一级的目录链接。
const docHref = (href) => {
  if (/^(https?:|mailto:|#|\/|\.\.\/)/.test(href)) return href
  const clean = href.replace(/^\.\//, '')
  return `../${clean}${/\.\w+$/.test(clean) ? '' : '/'}`
}

const inline = (text) =>
  esc(text)
    .replace(/!\[([^\]]*)\]\(([^)\s]+)\)/g, (_, alt, src) => `<img src="${docHref(src)}" alt="${alt}">`)
    .replace(/\[([^\]]+)\]\(([^)\s]+)\)/g, (_, label, href) => `<a href="${docHref(href)}">${label}</a>`)
    .replace(/&lt;(https?:\/\/[^&\s]+)&gt;/g, '<a href="$1">$1</a>')
    .replace(/&lt;([^@\s]+@[^&\s]+)&gt;/g, '<a href="mailto:$1">$1</a>')
    .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
    .replace(/`([^`]+)`/g, '<code>$1</code>')

const markdown = (source) => {
  const lines = source.split('\n')
  let title = 'LanSend'
  let i = 0
  if (lines[0] === '---') {
    for (i = 1; i < lines.length && lines[i] !== '---'; i += 1) {
      const m = /^title:\s*(.+)$/.exec(lines[i])
      if (m) title = m[1].trim()
    }
    i += 1
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
  for (; i < lines.length; i += 1) {
    const line = lines[i]
    if (!line.trim()) {
      flush()
      continue
    }
    const heading = /^(#{1,4})\s+(.*)$/.exec(line)
    if (heading) {
      flush()
      html.push(`<h${heading[1].length}>${inline(heading[2])}</h${heading[1].length}>`)
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
    flushList()
    para.push(line.trim())
  }
  flush()
  return { title, body: html.join('\n') }
}

const docPage = (slug, lang = 'zh') => {
  const { title, body } = markdown(readFileSync(join(DOCS, `${slug}.md`), 'utf8'))
  const base = '../'
  const t = content[lang]
  const nav = [
    [base || './', t.page.home],
    [`${base}privacy/`, t.footer.privacy],
    [`${base}support/`, t.footer.support],
  ]
  const html = `${header({ lang, base, nav, showLang: false })}
<main id="main" class="wrap doc">
${body}
</main>
${footer({ lang, base })}`
  return shell({
    lang,
    base,
    href: `/${slug}/`,
    title: `${title} · LanSend`,
    description: t.meta.description,
    body: html,
  })
}

/* ------------------------------------------------------------- 写文件 */

const write = (rel, text) => {
  const dest = join(HERE, rel)
  mkdirSync(dirname(dest), { recursive: true })
  writeFileSync(dest, text)
  console.log(`  ${rel}`)
}

for (const lang of Object.keys(langs)) {
  const dir = langs[lang].dir
  write(dir ? `${dir}/index.html` : 'index.html', homePage(lang))
}
write('privacy/index.html', docPage('privacy'))
write('support/index.html', docPage('support'))

const pages = ['/', '/en/', '/privacy/', '/support/']
write('robots.txt', `User-agent: *\nAllow: /\nSitemap: ${config.siteUrl}/sitemap.xml\n`)
write(
  'sitemap.xml',
  `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${pages
    .map((p) => `  <url><loc>${config.siteUrl}${p}</loc></url>`)
    .join('\n')}\n</urlset>\n`,
)
write('.nojekyll', '')

if (!config.icp) console.log('\n注意：SITE_ICP 为空，页脚没有备案号。拿到号后用 SITE_ICP=... 重新构建。')
console.log(`\n构建完成：${HERE}`)
