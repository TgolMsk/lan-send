// Visual review: every main screen in light and dark, from the mock-backed
// frontend in the locally installed Chrome (store shots: store-screenshots.mjs).
//   node scripts/ui-shots.mjs <outDir> [dark,light] [shot,shot]
import { chromium } from "playwright-core";
import { spawn } from "node:child_process";
import { mkdirSync } from "node:fs";
import { resolve } from "node:path";

const PORT = 1431;
const [outDir, themeArg = "dark,light", shotArg] = process.argv.slice(2);
if (!outDir) throw new Error("usage: ui-shots.mjs <outDir> [themes] [shots]");
const themes = themeArg.split(",");

const desk = { width: 1120, height: 740, scale: 2, mobile: false };
const phone = { width: 390, height: 844, scale: 3, mobile: true };
const tray = { width: 300, height: 380, scale: 2, mobile: false };
const allShots = {
  "desk-devices": [desk, "page=devices"],
  "desk-transfers": [desk, "page=transfers&demo=transfer&freeze=1&delay=100"],
  "desk-incoming": [desk, "page=devices&demo=incoming&delay=300"],
  "desk-settings": [desk, "page=settings"],
  "desk-clipboard": [desk, "page=clipboard"],
  "desk-history": [desk, "page=history"],
  "desk-pair": [desk, "page=devices&demo=pair&delay=300"],
  "ios-devices": [phone, "platform=ios&page=devices"],
  "ios-transfers": [phone, "platform=ios&page=transfers&demo=transfer&freeze=1&delay=100"],
  "ios-incoming": [phone, "platform=ios&page=devices&demo=incoming&delay=300"],
  tray: [tray, "panel=tray-drop&hover=0"],
};
const shots = shotArg ? shotArg.split(",") : Object.keys(allShots);

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
  {
    for (const theme of themes) {
      const dir = resolve(outDir, theme);
      mkdirSync(dir, { recursive: true });
      for (const name of shots) {
        const [spec, query] = allShots[name];
        const context = await browser.newContext({
          viewport: { width: spec.width, height: spec.height },
          deviceScaleFactor: spec.scale,
          isMobile: spec.mobile,
          hasTouch: spec.mobile,
          locale: process.env.SHOT_LOCALE ?? "zh-CN",
          colorScheme: theme,
        });
        const page = await context.newPage();
        await page.goto(`http://localhost:${PORT}/?${query}&theme=${theme}`, { waitUntil: "networkidle" });
        await page.waitForTimeout(query.includes("freeze") ? 3400 : 1500);
        await page.screenshot({ path: resolve(dir, `${name}.png`) });
        console.log(`${theme}/${name}.png`);
        await context.close();
      }
    }
  }
  await browser.close();
} finally {
  vite.kill();
}
