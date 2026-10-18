// Renders the brand SVGs into every app icon, in the locally installed Google
// Chrome (ADR-0024). One command does the whole set:
//   node scripts/make-icons.mjs
//
// 1. docs/brand/icon.svg (full bleed) and docs/brand/menubar.svg become:
//   src-tauri/icons/source.png        1024, opaque (iOS and Windows want no transparency)
//   src-tauri/icons/source-macos.png  Apple's template: 824 px squircle with a soft shadow on 1024
//   src-tauri/icons/tray-template.png docs/brand/menubar.svg, black on transparent (macOS menu bar)
//   src/assets/logo.png               256 px squircle for the sidebar
// 2. `tauri icon` turns source.png into the Windows, Linux and iOS sizes (iOS
//    lands in gen/apple and is mirrored to icons/ios), and source-macos.png
//    into icon.icns. The Android set it also writes is deleted.
// 3. App Store Connect rejects iOS icons with an alpha channel, even a fully
//    opaque one, and `tauri icon` writes RGBA: the iOS files are re-encoded
//    without it.
import { chromium } from "playwright-core";
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdtempSync, readFileSync, readdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const ROOT = resolve(import.meta.dirname, "../../..");
const ICONS = resolve(ROOT, "apps/app/src-tauri/icons");
const svgUri = (file) => `data:image/svg+xml;base64,${readFileSync(resolve(ROOT, file)).toString("base64")}`;
const icon = svgUri("docs/brand/icon.svg");
const menubar = svgUri("docs/brand/menubar.svg");

/** Superellipse close to Apple's continuous-corner icon shape. */
function squircle(size, offset = 0, n = 5, points = 240) {
  const a = size / 2;
  const c = offset + a;
  const d = [];
  for (let i = 0; i < points; i += 1) {
    const t = (2 * Math.PI * i) / points;
    const x = c + a * Math.sign(Math.cos(t)) * Math.abs(Math.cos(t)) ** (2 / n);
    const y = c + a * Math.sign(Math.sin(t)) * Math.abs(Math.sin(t)) ** (2 / n);
    d.push(`${x.toFixed(2)},${y.toFixed(2)}`);
  }
  return `M${d.join(" L")} Z`;
}

const mac = squircle(824, 100);
const outputs = [
  { path: resolve(ICONS, "source.png"), size: 1024, opaque: true, body: `<image href="${icon}" width="1024" height="1024"/>` },
  {
    path: resolve(ICONS, "source-macos.png"),
    size: 1024,
    body: `<defs><clipPath id="c"><path d="${mac}"/></clipPath><filter id="s" x="-10%" y="-10%" width="120%" height="125%"><feGaussianBlur stdDeviation="12"/></filter></defs>
      <path d="${mac}" transform="translate(0,10)" fill-opacity="0.3" filter="url(#s)"/>
      <g clip-path="url(#c)"><image href="${icon}" x="100" y="100" width="824" height="824"/></g>
      <path d="${mac}" fill="none" stroke="#000" stroke-opacity="0.08" stroke-width="2"/>`,
  },
  {
    path: resolve(ROOT, "apps/app/src/assets/logo.png"),
    size: 256,
    body: `<defs><clipPath id="c"><path d="${squircle(1024)}"/></clipPath></defs><g clip-path="url(#c)"><image href="${icon}" width="1024" height="1024"/></g>`,
  },
  { path: resolve(ICONS, "tray-template.png"), size: 64, body: `<image href="${menubar}" width="1024" height="1024"/>` },
];

const browser = await chromium.launch({ channel: "chrome", headless: true });
for (const out of outputs) {
  const page = await browser.newPage({ viewport: { width: out.size, height: out.size }, deviceScaleFactor: 1 });
  await page.setContent(
    `<body style="margin:0;background:${out.opaque ? "#000" : "transparent"}"><svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" width="${out.size}" height="${out.size}" style="display:block">${out.body}</svg></body>`,
  );
  await page.waitForTimeout(300);
  await page.screenshot({ path: out.path, omitBackground: !out.opaque });
  console.log(`${out.path} (${out.size}px${out.opaque ? ", opaque" : ""})`);
  await page.close();
}

const tauri = (...args) => execFileSync("pnpm", ["exec", "tauri", "icon", ...args], { cwd: resolve(ROOT, "apps/app"), stdio: "ignore" });
tauri(resolve(ICONS, "source.png"));
const icns = mkdtempSync(join(tmpdir(), "lansend-icns-"));
tauri(resolve(ICONS, "source-macos.png"), "-o", icns);
copyFileSync(join(icns, "icon.icns"), resolve(ICONS, "icon.icns"));
rmSync(icns, { recursive: true });
rmSync(resolve(ICONS, "android"), { recursive: true, force: true });

// Re-encode the iOS set opaque: an image drawn 1:1 on an opaque page and
// screenshotted comes back as RGB.
const appiconset = resolve(ROOT, "apps/app/src-tauri/gen/apple/Assets.xcassets/AppIcon.appiconset");
for (const file of readdirSync(appiconset).filter((name) => name.endsWith(".png"))) {
  const path = join(appiconset, file);
  const png = readFileSync(path);
  const size = png.readUInt32BE(16); // IHDR width; iOS icons are square
  const page = await browser.newPage({ viewport: { width: size, height: size }, deviceScaleFactor: 1 });
  await page.setContent(`<body style="margin:0;background:#000"><img style="display:block" src="data:image/png;base64,${png.toString("base64")}"></body>`);
  await page.screenshot({ path });
  await page.close();
  copyFileSync(path, resolve(ICONS, "ios", file));
}
console.log(`tauri icon done; ${appiconset} re-encoded without alpha and mirrored to icons/ios`);
await browser.close();
