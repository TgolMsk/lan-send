// Interface strings: one JSON table per locale under ./locales, English
// being the reference (its keys define the type) and the fallback.
// The chosen language lives in settings (`app.language`, "system" by
// default) and is mirrored in localStorage so the first paint before the
// settings arrive already uses it.

import de from "./locales/de.json";
import en from "./locales/en.json";
import es from "./locales/es.json";
import fr from "./locales/fr.json";
import ja from "./locales/ja.json";
import ko from "./locales/ko.json";
import zhHans from "./locales/zh-Hans.json";
import zhHant from "./locales/zh-Hant.json";

export const LOCALES = ["en", "zh-Hans", "zh-Hant", "ja", "ko", "de", "fr", "es"] as const;
export type Locale = (typeof LOCALES)[number];
/** What the user chooses: a locale or "system". */
export type LanguageSetting = Locale | "system";
export type Key = keyof typeof en;

/** Native names, shown untranslated in the language picker. */
export const LOCALE_NAMES: Record<Locale, string> = {
  en: "English",
  "zh-Hans": "简体中文",
  "zh-Hant": "繁體中文",
  ja: "日本語",
  ko: "한국어",
  de: "Deutsch",
  fr: "Français",
  es: "Español",
};

const tables: Record<Locale, Record<Key, string>> = {
  en,
  "zh-Hans": zhHans,
  "zh-Hant": zhHant,
  ja,
  ko,
  de,
  fr,
  es,
};

const STORAGE_KEY = "lan-send.language";

/** Maps a BCP 47 tag from the OS to a locale we ship. */
export function localeForTag(tag: string): Locale | null {
  const lower = tag.toLowerCase();
  if (lower.startsWith("zh")) {
    const traditional = /hant|tw|hk|mo/.test(lower.slice(2));
    return traditional ? "zh-Hant" : "zh-Hans";
  }
  const base = lower.split(/[-_]/)[0];
  return (LOCALES as readonly string[]).includes(base) ? (base as Locale) : null;
}

export function systemLocale(): Locale {
  const tags = typeof navigator === "undefined" ? [] : [...(navigator.languages ?? []), navigator.language];
  for (const tag of tags) {
    const locale = localeForTag(tag ?? "");
    if (locale) return locale;
  }
  return "en";
}

export function isLanguageSetting(value: unknown): value is LanguageSetting {
  return value === "system" || (LOCALES as readonly string[]).includes(String(value));
}

/** The locale a setting resolves to; unknown values follow the system. */
export function resolveLocale(setting: string | null | undefined): Locale {
  if (setting === "zh") return "zh-Hans"; // pre-0.4 stored value
  if (setting && setting !== "system" && (LOCALES as readonly string[]).includes(setting)) {
    return setting as Locale;
  }
  return systemLocale();
}

function storedLanguage(): LanguageSetting {
  try {
    const stored = localStorage.getItem(STORAGE_KEY) ?? localStorage.getItem("lan-send.locale");
    if (stored === "zh") return "zh-Hans";
    if (isLanguageSetting(stored)) return stored;
  } catch {
    // no storage
  }
  return "system";
}

let language: LanguageSetting = storedLanguage();
let current: Locale = resolveLocale(language);
applyDocumentLanguage();

function applyDocumentLanguage() {
  if (typeof document !== "undefined") document.documentElement.lang = current;
}

/** Applies a language choice for this session and remembers it locally. */
export function setLanguage(setting: LanguageSetting) {
  language = setting;
  current = resolveLocale(setting);
  applyDocumentLanguage();
  try {
    localStorage.setItem(STORAGE_KEY, setting);
  } catch {
    // no storage
  }
}

export function getLanguage(): LanguageSetting {
  return language;
}

export function getLocale(): Locale {
  return current;
}

export function t(key: Key, vars?: Record<string, string | number>): string {
  let text: string = tables[current][key] ?? tables.en[key] ?? key;
  if (vars) {
    for (const [name, value] of Object.entries(vars)) {
      text = text.replace(`{${name}}`, String(value));
    }
  }
  return text;
}

/** True when the reference table has this key (for dynamic keys). */
export function hasKey(key: string): key is Key {
  return Object.prototype.hasOwnProperty.call(en, key);
}

export type { Key as I18nKey };
