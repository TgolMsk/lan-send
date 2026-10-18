// iOS settings (ADR-0025): grouped rows like the Settings app. Every change
// is saved at once; choices open the system picker through a transparent
// <select> over the row, numbers and text are edited in place.

import { useEffect, useState, type ReactNode } from "react";
import { PageHead } from "../components/Layout";
import { Icon, type IconName } from "../components/icons";
import { DeviceIcon } from "../components/ui";
import { formatBytes, shortFingerprint } from "../format";
import { LOCALES, LOCALE_NAMES, t, type LanguageSetting } from "../i18n";
import { mediaCacheClear, mediaCacheSize, saveSettings, switchLanguage, useStore } from "../store";
import type { Settings } from "../types";

type Tint = "blue" | "indigo" | "orange" | "green" | "red" | "gray" | "teal" | "purple";

export function SettingsList() {
  const stored = useStore((s) => s.settings);
  const identity = useStore((s) => s.identity);
  const platform = useStore((s) => s.platform);
  const [draft, setDraft] = useState<Settings | null>(stored);
  useEffect(() => setDraft(stored), [stored]);
  const [cacheSize, setCacheSize] = useState<number | null>(null);
  useEffect(() => {
    void mediaCacheSize().then(setCacheSize);
  }, []);
  if (!draft) return <PageHead title={t("settings.title")} subtitle={t("app.loading")} />;

  const commit = (fn: (s: Settings) => Settings) => {
    const next = fn(draft);
    if (JSON.stringify(next) === JSON.stringify(draft)) return;
    setDraft(next);
    // A refused change (say, a port in use) snaps the row back.
    void saveSettings(next, { quiet: true }).then((saved) => saved || setDraft(draft));
  };
  const count = (value: string, fallback: number, min = 0) => {
    const parsed = Number(value);
    return Number.isFinite(parsed) && parsed >= min ? Math.round(parsed) : fallback;
  };

  return (
    <>
      <PageHead title={t("settings.title")} />
      <div className="m-group m-me">
        <DeviceIcon type="mobile" model={identity?.deviceModel ?? null} />
        <div className="m-me-text">
          <TextCell
            className="m-me-name"
            ariaLabel={t("settings.alias")}
            value={draft.alias ?? ""}
            placeholder={identity?.alias ?? t("app.thisDevice")}
            onCommit={(value) => commit((s) => ({ ...s, alias: value.trim() || null }))}
          />
          <span className="mono">
            {t("app.port")} {identity?.port ?? draft.port}
            {identity ? ` · ${shortFingerprint(identity.fingerprint)}` : ""}
          </span>
        </div>
      </div>

      <h2 className="m-section">{t("settings.receive")}</h2>
      <div className="m-group">
        <Row icon="folder" tint="blue" label={t("settings.saveTo")}>
          <span className="m-value">{t("settings.filesApp")}</span>
        </Row>
        <SwitchRow icon="devices" tint="indigo" label={t("settings.folderByDevice")} checked={draft.organize.byDevice} onChange={(v) => commit((s) => ({ ...s, organize: { ...s.organize, byDevice: v } }))} />
        <SwitchRow icon="calendar" tint="orange" label={t("settings.folderByDate")} checked={draft.organize.byDate} onChange={(v) => commit((s) => ({ ...s, organize: { ...s.organize, byDate: v } }))} />
        <SwitchRow icon="file" tint="teal" label={t("settings.folderByType")} checked={draft.organize.byType} onChange={(v) => commit((s) => ({ ...s, organize: { ...s.organize, byType: v } }))} />
        <SelectRow
          icon="copy"
          tint="orange"
          label={t("settings.onConflict")}
          value={draft.onConflict}
          onChange={(v) => commit((s) => ({ ...s, onConflict: v }))}
          options={[
            { value: "rename", label: t("settings.conflict.rename") },
            { value: "overwrite", label: t("settings.conflict.overwrite") },
            { value: "ask", label: t("settings.conflict.ask") },
          ]}
        />
        <SwitchRow icon="link" tint="green" label={t("settings.autoAcceptPaired")} checked={draft.app.autoAcceptPaired} onChange={(v) => commit((s) => ({ ...s, app: { ...s.app, autoAcceptPaired: v } }))} />
        <SwitchRow icon="bell" tint="red" label={t("settings.notifications")} checked={draft.app.notifications} onChange={(v) => commit((s) => ({ ...s, app: { ...s.app, notifications: v } }))} />
      </div>

      <h2 className="m-section">{t("settings.security")}</h2>
      <div className="m-group">
        <Row icon="lock" tint="gray" label={t("settings.pin")}>
          <TextCell
            ariaLabel={t("settings.pin")}
            value={draft.pin ?? ""}
            placeholder={t("settings.notSet")}
            inputMode="numeric"
            onCommit={(value) => commit((s) => ({ ...s, pin: value.trim() || null }))}
          />
        </Row>
        <SwitchRow icon="shield" tint="blue" label={t("settings.requireCertsShort")} checked={draft.requireClientCerts} onChange={(v) => commit((s) => ({ ...s, requireClientCerts: v }))} />
        <SwitchRow icon="bolt" tint="teal" label={t("settings.resume")} checked={draft.resume} onChange={(v) => commit((s) => ({ ...s, resume: v }))} />
        <SwitchRow
          icon="check"
          tint="green"
          label={t("settings.checksumsShort")}
          checked={draft.createChecksums && draft.verifyChecksums}
          onChange={(v) => commit((s) => ({ ...s, createChecksums: v, verifyChecksums: v }))}
        />
        <SwitchRow icon="globe" tint="gray" label={t("settings.ipv6")} checked={draft.ipv6} onChange={(v) => commit((s) => ({ ...s, ipv6: v }))} />
      </div>

      <h2 className="m-section">{t("settings.advanced")}</h2>
      <div className="m-group">
        <Row icon="hash" tint="gray" label={t("settings.port")}>
          <TextCell ariaLabel={t("settings.port")} value={String(draft.port)} inputMode="numeric" onCommit={(value) => commit((s) => ({ ...s, port: Math.min(65535, count(value, s.port, 1)) }))} />
        </Row>
        <Row icon="transfers" tint="blue" label={t("settings.parallel")}>
          <TextCell
            ariaLabel={t("settings.parallel")}
            value={String(draft.parallelUploads)}
            inputMode="numeric"
            onCommit={(value) => commit((s) => ({ ...s, parallelUploads: Math.min(16, count(value, s.parallelUploads, 1)) }))}
          />
        </Row>
        <Row icon="history" tint="gray" label={t("settings.historyLimit")}>
          <TextCell ariaLabel={t("settings.historyLimit")} value={String(draft.historyLimit)} inputMode="numeric" onCommit={(value) => commit((s) => ({ ...s, historyLimit: count(value, s.historyLimit) }))} />
        </Row>
      </div>

      <h2 className="m-section">{t("settings.appearance")}</h2>
      <div className="m-group">
        <SelectRow
          icon="moon"
          tint="indigo"
          label={t("settings.theme")}
          value={draft.app.theme}
          onChange={(v) => commit((s) => ({ ...s, app: { ...s.app, theme: v } }))}
          options={[
            { value: "system", label: t("settings.theme.system") },
            { value: "light", label: t("settings.theme.light") },
            { value: "dark", label: t("settings.theme.dark") },
          ]}
        />
        <SelectRow<LanguageSetting>
          icon="language"
          tint="blue"
          label={t("settings.language")}
          value={(draft.app.language as LanguageSetting) || "system"}
          onChange={(v) => {
            // switchLanguage stores the choice itself.
            setDraft({ ...draft, app: { ...draft.app, language: v } });
            void switchLanguage(v);
          }}
          options={[{ value: "system", label: t("settings.languageSystem") }, ...LOCALES.map((locale) => ({ value: locale, label: LOCALE_NAMES[locale] }))]}
        />
        <Row icon="image" tint="purple" label={t("settings.thumbCache")}>
          <span className="m-value mono">{cacheSize == null ? "…" : formatBytes(cacheSize)}</span>
          <button type="button" className="m-link-btn" disabled={!cacheSize} onClick={() => void mediaCacheClear().then(() => setCacheSize(0))}>
            {t("app.clear")}
          </button>
        </Row>
      </div>

      {identity && (
        <p className="m-foot">
          {t("app.fingerprint")} <span className="mono breakable">{identity.fingerprint}</span>
          <br />
          LanSend {platform.version} · {t("settings.instant")}
        </p>
      )}
    </>
  );
}

function Row({ icon, tint, label, children }: { icon: IconName; tint: Tint; label: string; children?: ReactNode }) {
  return (
    <div className="m-row">
      <span className={`m-sq ${tint}`}>
        <Icon name={icon} size={18} />
      </span>
      <span className="m-row-label">{label}</span>
      {children}
    </div>
  );
}

function SwitchRow({ checked, onChange, ...row }: { icon: IconName; tint: Tint; label: string; checked: boolean; onChange: (value: boolean) => void }) {
  return (
    <label className="m-row">
      <span className={`m-sq ${row.tint}`}>
        <Icon name={row.icon} size={18} />
      </span>
      <span className="m-row-label">{row.label}</span>
      <input type="checkbox" role="switch" className="m-switch" checked={checked} onChange={(event) => onChange(event.target.checked)} />
    </label>
  );
}

function SelectRow<T extends string>({
  value,
  options,
  onChange,
  ...row
}: {
  icon: IconName;
  tint: Tint;
  label: string;
  value: T;
  options: { value: T; label: string }[];
  onChange: (value: T) => void;
}) {
  return (
    <label className="m-row">
      <span className={`m-sq ${row.tint}`}>
        <Icon name={row.icon} size={18} />
      </span>
      <span className="m-row-label">{row.label}</span>
      <span className="m-value">{options.find((o) => o.value === value)?.label ?? value}</span>
      <Icon name="chevron" size={14} className="m-chevron" />
      <select className="m-row-select" aria-label={row.label} value={value} onChange={(event) => onChange(event.target.value as T)}>
        {options.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
    </label>
  );
}

/** Edits in place and commits on blur or Return, so a half-typed port does
 *  not restart the network service on every keystroke. */
function TextCell({
  value,
  placeholder,
  inputMode,
  className = "m-cell",
  ariaLabel,
  onCommit,
}: {
  value: string;
  placeholder?: string;
  inputMode?: "numeric" | "text";
  className?: string;
  ariaLabel: string;
  onCommit: (value: string) => void;
}) {
  const [text, setText] = useState(value);
  useEffect(() => setText(value), [value]);
  return (
    <input
      className={className}
      aria-label={ariaLabel}
      value={text}
      placeholder={placeholder}
      inputMode={inputMode}
      enterKeyHint="done"
      onChange={(event) => setText(event.target.value)}
      onBlur={() => text !== value && onCommit(text)}
      onKeyDown={(event) => event.key === "Enter" && event.currentTarget.blur()}
    />
  );
}
