#!/usr/bin/env node
// 官网配图：用 apps/app 的 mock 前端渲染各端界面截图，再压成 WebP。
//
//   node lansend_web/tools/shots.mjs            # 全部语言 / 全部设备
//   node lansend_web/tools/shots.mjs zh desktop     # 只出简中的桌面端（语言 zh/en，设备 desktop/iphone/ipad）
//
// 与商店截图（apps/app/scripts/store-screenshots.mjs）同源：同一个 mock 数据、
// 同一套查询参数，只是尺寸按网页排版重新取，并且每种语言各出一套。
// 依赖 apps/app 的 playwright-core 与本机 Chrome，外加 sips / cwebp（macOS 自带 + brew install webp）。

import { spawn, spawnSync } from 'node:child_process'
import { mkdirSync, rmSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const HERE = dirname(fileURLToPath(import.meta.url))
const ROOT = resolve(HERE, '../..')
const APP = join(ROOT, 'apps/app')
const OUT = join(HERE, '../assets/img/shots')
const TMP = join(ROOT, 'target/site-shots')
const PORT = 1422

const { chromium } = await import(join(APP, 'node_modules/playwright-core/index.mjs'))

// 界面语言 -> 目录名 / 浏览器 locale。应用把语言存在 localStorage 的 lan-send.language。
const LANGS = {
  zh: { locale: 'zh-Hans', browser: 'zh-CN' },
  en: { locale: 'en', browser: 'en-US' },
}

// 桌面端窗口比商店截图矮一些：网页里横幅太高会把下面的内容挤出首屏。
const DEVICES = {
  desktop: {
    width: 1360, height: 780, scale: 2, mobile: false, webpWidth: 2176,
    shots: [
      ['1-devices', 'page=devices'],
      ['2-transfer', 'page=transfers&demo=transfer&freeze=1&delay=100'],
      ['3-clipboard', 'page=clipboard'],
      ['4-history', 'page=history'],
      ['5-settings', 'page=settings'],
    ],
  },
  iphone: {
    width: 390, height: 844, scale: 3, mobile: true, webpWidth: 936,
    shots: [
      ['1-devices', 'platform=ios&page=devices'],
      ['2-incoming', 'platform=ios&page=devices&demo=incoming'],
      ['3-transfer', 'platform=ios&page=transfers&demo=transfer&freeze=1&delay=100'],
      ['4-history', 'platform=ios&page=history'],
      ['5-settings', 'platform=ios&page=settings'],
    ],
  },
  ipad: {
    width: 1032, height: 1376, scale: 2, mobile: true, webpWidth: 1548,
    shots: [
      ['1-devices', 'platform=ios&page=devices'],
      ['2-transfer', 'platform=ios&page=transfers&demo=transfer&freeze=1&delay=100'],
      ['3-history', 'platform=ios&page=history'],
    ],
  },
}

// 参数可以任意组合语言与设备；某一维没提到就全出。
const args = process.argv.slice(2)
const pick = (all, chosen) => (chosen.length ? chosen : all)
const langs = pick(Object.keys(LANGS), args.filter((a) => a in LANGS))
const devices = pick(Object.keys(DEVICES), args.filter((a) => a in DEVICES))
const unknown = args.filter((a) => !(a in LANGS) && !(a in DEVICES))
if (unknown.length) throw new Error(`不认识的参数：${unknown.join(' ')}（语言 ${Object.keys(LANGS)}，设备 ${Object.keys(DEVICES)}）`)

const run = (cmd, argv) => {
  const r = spawnSync(cmd, argv, { stdio: ['ignore', 'ignore', 'inherit'] })
  if (r.status !== 0) throw new Error(`${cmd} ${argv.join(' ')} -> ${r.status}`)
}

const vite = spawn('pnpm', ['exec', 'vite', '--port', String(PORT), '--strictPort'], { cwd: APP, stdio: 'ignore' })
const waitForServer = async () => {
  for (let i = 0; i < 90; i += 1) {
    try {
      await fetch(`http://localhost:${PORT}/`)
      return
    } catch {
      await new Promise((r) => setTimeout(r, 500))
    }
  }
  throw new Error('vite did not start')
}

try {
  await waitForServer()
  const browser = await chromium.launch({ channel: 'chrome', headless: true })
  for (const lang of langs) {
    for (const device of devices) {
      const spec = DEVICES[device]
      const raw = join(TMP, lang, device)
      const out = join(OUT, lang, device)
      rmSync(raw, { recursive: true, force: true })
      mkdirSync(raw, { recursive: true })
      mkdirSync(out, { recursive: true })
      const context = await browser.newContext({
        viewport: { width: spec.width, height: spec.height },
        deviceScaleFactor: spec.scale,
        isMobile: spec.mobile,
        hasTouch: spec.mobile,
        locale: LANGS[lang].browser,
        colorScheme: 'dark',
        reducedMotion: 'reduce',
      })
      await context.addInitScript(`try { localStorage.setItem('lan-send.language', '${LANGS[lang].locale}') } catch {}`)
      for (const [file, query] of spec.shots) {
        const page = await context.newPage()
        await page.goto(`http://localhost:${PORT}/?${query}`, { waitUntil: 'networkidle' })
        await page.waitForTimeout(1800)
        const png = join(raw, `${file}.png`)
        await page.screenshot({ path: png })
        await page.close()
        // 缩到排版用的宽度再转 WebP：2 倍图给 Retina，体积比 PNG 小一个数量级。
        run('sips', ['--resampleWidth', String(spec.webpWidth), png, '--out', png])
        run('cwebp', ['-q', '86', '-metadata', 'none', '-quiet', png, '-o', join(out, `${file}.webp`)])
        console.log(`  ${lang}/${device}/${file}.webp`)
      }
      await context.close()
    }
  }
  await browser.close()
} finally {
  vite.kill()
}
console.log(`\n配图输出：${OUT}`)
