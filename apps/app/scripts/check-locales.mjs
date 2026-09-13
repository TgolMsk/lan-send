// Fails the build when a locale table drifts from the English reference:
// missing or extra keys, or a placeholder ({alias}, {n}, …) that the
// translation lost. Run: node scripts/check-locales.mjs
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

// fileURLToPath, not .pathname: on Windows the latter yields "/D:/…" which join() turns into "D:\D:\…".
const dir = fileURLToPath(new URL("../src/locales/", import.meta.url));
const en = JSON.parse(readFileSync(join(dir, "en.json"), "utf8"));
const placeholders = (text) => [...text.matchAll(/\{[a-zA-Z]+\}/g)].map((m) => m[0]).sort().join(" ");
let failed = false;
for (const file of readdirSync(dir).filter((f) => f.endsWith(".json") && f !== "en.json")) {
  const table = JSON.parse(readFileSync(join(dir, file), "utf8"));
  const missing = Object.keys(en).filter((k) => !(k in table));
  const extra = Object.keys(table).filter((k) => !(k in en));
  const drift = Object.keys(en).filter((k) => k in table && placeholders(en[k]) !== placeholders(table[k]));
  const empty = Object.keys(table).filter((k) => typeof table[k] !== "string" || table[k].trim() === "");
  for (const [label, keys] of [["missing", missing], ["extra", extra], ["placeholder mismatch", drift], ["empty", empty]]) {
    if (keys.length) {
      failed = true;
      console.error(`${file}: ${label}: ${keys.join(", ")}`);
    }
  }
}
if (failed) process.exit(1);
console.log("locales ok");
