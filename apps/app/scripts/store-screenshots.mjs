// Renders App Store screenshots from the mock-backed frontend at the exact
// pixel sizes Apple accepts, driving the locally installed Google Chrome.
//   pnpm screenshots            (writes store/screenshots/<device>/*.png)
import { chromium } from "playwright-core";
import { spawn } from "node:child_process";
import { mkdirSync } from "node:fs";
import { resolve } from "node:path";

const PORT = 1421;
// Light shows the native style (ADR-0024) best; SHOT_THEME=dark for the other.
const THEME = process.env.SHOT_THEME ?? "light";
const OUT = resolve("store/screenshots");
const devices = {
  // iPhone 6.5" slot — 1284×2778 (App Store Connect also uses it for 6.9")
  // `safe` stands in for the status bar and home indicator insets, which
  // Chrome does not report (ADR-0025 lays the iPhone pages out around them).
  "iphone-6.5": { width: 428, height: 926, scale: 3, mobile: true, safe: [54, 34], shots: [
    ["1-devices", "platform=ios&page=devices"],
    ["2-device", "platform=ios&page=devices&sheet=A1B2C3D4E5F60718293A4B5C6D7E8F90A1B2C3D4E5F60718293A4B5C6D7E8F90"],
    ["3-incoming", "platform=ios&page=devices&demo=incoming"],
    ["4-transfer", "platform=ios&page=transfers&demo=transfer&freeze=1&delay=100"],
    ["5-settings", "platform=ios&page=settings"],
  ] },
  // iPad 13" — 2064×2752
  "ipad-13": { width: 1032, height: 1376, scale: 2, mobile: true, safe: [24, 20], shots: [
    ["1-devices", "platform=ios&page=devices"],
    ["2-transfer", "platform=ios&page=transfers&demo=transfer&freeze=1&delay=100"],
    ["3-history", "platform=ios&page=history"],
  ] },
  // Mac — 2880×1800
  mac: { width: 1440, height: 900, scale: 2, mobile: false, shots: [
    ["1-devices", "page=devices"],
    ["2-transfer", "page=transfers&demo=transfer&freeze=1&delay=100"],
    ["3-clipboard", "page=clipboard"],
    ["4-settings", "page=settings"],
  ] },
};

const vite = spawn("pnpm", ["exec", "vite", "--port", String(PORT), "--strictPort"], { stdio: "ignore" });
const waitForServer = async () => {
  for (let i = 0; i < 60; i += 1) {
    try {
      await fetch(`http://localhost:${PORT}/`);
      return;
    } catch {
      await new Promise((r) => setTimeout(r, 500));
    }
  }
  throw new Error("vite did not start");
};

try {
  await waitForServer();
  const browser = await chromium.launch({ channel: "chrome", headless: true });
  for (const [name, spec] of Object.entries(devices)) {
    mkdirSync(resolve(OUT, name), { recursive: true });
    const context = await browser.newContext({
      viewport: { width: spec.width, height: spec.height },
      deviceScaleFactor: spec.scale,
      isMobile: spec.mobile,
      hasTouch: spec.mobile,
      locale: "zh-CN",
      colorScheme: THEME,
    });
    for (const [file, query] of spec.shots) {
      const page = await context.newPage();
      await page.goto(`http://localhost:${PORT}/?${query}&theme=${THEME}`, { waitUntil: "networkidle" });
      if (spec.safe) await page.addStyleTag({ content: `:root{--safe-top:${spec.safe[0]}px;--safe-bottom:${spec.safe[1]}px}` });
      await page.waitForTimeout(query.includes("freeze") ? 3400 : 1800);
      const path = resolve(OUT, name, `${file}.png`);
      await page.screenshot({ path });
      const size = await page.evaluate(() => `${innerWidth}x${innerHeight} @${devicePixelRatio}`);
      console.log(`${name}/${file}.png (${size})`);
      await page.close();
    }
    await context.close();
  }
  await browser.close();
} finally {
  vite.kill();
}
