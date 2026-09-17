#!/usr/bin/env node
// 社交分享图（og:image，1200×630）：品牌色底 + 图标 + 应用截图。
//
//   node lansend_web/tools/og.mjs      # 写出 assets/img/og.png 与 og-en.png
//
// 用本机 Chrome 渲染一段临时 HTML 再截图，和站点共用同一批配图。
// 依赖 apps/app 的 playwright-core。

import { writeFileSync, rmSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const HERE = dirname(fileURLToPath(import.meta.url))
const WEB = resolve(HERE, '..')
const APP = resolve(HERE, '../../apps/app')
const { chromium } = await import(join(APP, 'node_modules/playwright-core/index.mjs'))

const CARDS = {
  'og.png': {
    lang: 'zh',
    lead: '局域网文件与剪贴板互传',
    sub: 'iPhone · Mac · Windows　不经云端，兼容 LocalSend',
    foot: '开源免费 · MIT · github.com/TgolMsk/lan-send',
  },
  'og-en.png': {
    lang: 'en',
    lead: 'Files and clipboard over your own Wi‑Fi',
    sub: 'iPhone · Mac · Windows　No cloud. Speaks LocalSend.',
    foot: 'Free and open source · MIT · github.com/TgolMsk/lan-send',
  },
}

const html = ({ lang, lead, sub, foot }) => `<!doctype html>
<meta charset="utf-8">
<style>
  * { box-sizing: border-box; margin: 0; }
  body {
    width: 1200px; height: 630px; overflow: hidden; position: relative;
    background: linear-gradient(135deg, #050517 0%, #18185c 52%, #4444a3 100%);
    color: #fafafd; font: 16px/1.5 -apple-system, BlinkMacSystemFont, "PingFang SC", "Segoe UI", sans-serif;
  }
  .glow { position: absolute; inset: -20% -10% auto auto; width: 720px; height: 720px;
    background: radial-gradient(circle, rgba(27,238,121,.16), transparent 62%); }
  .pad { position: absolute; inset: 0; padding: 72px 76px; display: flex; flex-direction: column; }
  .top { display: flex; align-items: center; gap: 26px; }
  .top img { width: 118px; height: 118px; border-radius: 28px; box-shadow: 0 24px 60px rgba(5,5,23,.6); }
  .top b { font-size: 88px; font-weight: 800; letter-spacing: -.045em; }
  .lead { margin-top: 42px; font-size: 42px; font-weight: 700; letter-spacing: -.02em; max-width: 720px; }
  .sub { margin-top: 18px; font-size: 25px; color: #b9bce6; max-width: 700px; }
  .foot { margin-top: auto; font-size: 20px; color: #9fa1d4; }
  .shot { position: absolute; right: -170px; bottom: 54px; width: 620px;
    border-radius: 16px; overflow: hidden; border: 1px solid rgba(255,255,255,.16);
    box-shadow: 0 50px 110px rgba(5,5,23,.65); transform: rotate(-7deg); }
  .shot img { display: block; width: 100%; }
</style>
<div class="glow"></div>
<div class="shot"><img src="../assets/img/shots/${lang}/desktop/1-devices.webp"></div>
<div class="pad">
  <div class="top"><img src="../assets/img/icon-512.png"><b>LanSend</b></div>
  <p class="lead">${lead}</p>
  <p class="sub">${sub}</p>
  <p class="foot">${foot}</p>
</div>
`

const browser = await chromium.launch({ channel: 'chrome', headless: true })
const context = await browser.newContext({ viewport: { width: 1200, height: 630 }, deviceScaleFactor: 1 })
const temp = join(WEB, 'tools/.og.html')
try {
  for (const [name, card] of Object.entries(CARDS)) {
    writeFileSync(temp, html(card))
    const page = await context.newPage()
    await page.goto(`file://${temp}`, { waitUntil: 'networkidle' })
    await page.screenshot({ path: join(WEB, 'assets/img', name) })
    await page.close()
    console.log(`  assets/img/${name}`)
  }
} finally {
  rmSync(temp, { force: true })
  await browser.close()
}
