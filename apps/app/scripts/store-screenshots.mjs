// Renders App Store screenshots from the mock-backed frontend at the exact
// pixel sizes Apple accepts, driving the locally installed Google Chrome.
//   pnpm screenshots            (writes store/screenshots/<device>/*.png)
import { chromium } from "playwright-core";
import { spawn } from "node:child_process";
import { mkdirSync } from "node:fs";
import { resolve } from "node:path";

const PORT = 1421;
const OUT = resolve("store/screenshots");
const devices = {
  // iPhone 6.9" — 1320×2868
  "iphone-6.9": { width: 440, height: 956, scale: 3, mobile: true, shots: [
    ["1-devices", "platform=ios&page=devices"],
    ["2-incoming", "platform=ios&page=devices&demo=incoming"],
    ["3-transfer", "platform=ios&page=transfers&demo=transfer&freeze=1&delay=100"],
    ["4-history", "platform=ios&page=history"],
    ["5-settings", "platform=ios&page=settings"],
  ] },
  // iPad 13" — 2064×2752
  "ipad-13": { width: 1032, height: 1376, scale: 2, mobile: true, shots: [
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
      colorScheme: "dark",
    });
    for (const [file, query] of spec.shots) {
      const page = await context.newPage();
      await page.goto(`http://localhost:${PORT}/?${query}`, { waitUntil: "networkidle" });
      await page.waitForTimeout(1800);
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
