import { useEffect, useState } from "react";
import { PageHead } from "../components/Layout";
import { Button, Card, Field, Select, TextInput, Toggle } from "../components/ui";
import { getLocale, t, type Locale } from "../i18n";
import { pickFolder, saveSettings, switchLocale, useStore } from "../store";
import type { Settings } from "../types";

const MB = 1024 * 1024;

export function SettingsPage() {
  const stored = useStore((s) => s.settings);
  const identity = useStore((s) => s.identity);
  const platform = useStore((s) => s.platform);
  const locale = useStore((s) => s.locale);
  const [draft, setDraft] = useState<Settings | null>(stored);
  useEffect(() => setDraft(stored), [stored]);
  if (!draft) return <PageHead title={t("settings.title")} subtitle={t("app.loading")} />;

  const dirty = JSON.stringify(draft) !== JSON.stringify(stored);
  const patch = (fn: (s: Settings) => Settings) => setDraft((d) => (d ? fn(d) : d));
  const number = (value: string, fallback: number) => {
    const parsed = Number(value);
    return Number.isFinite(parsed) && parsed >= 0 ? parsed : fallback;
  };

  return (
    <>
      <PageHead
        title={t("settings.title")}
        subtitle={t("settings.subtitle")}
        actions={
          <Button icon="check" disabled={!dirty} onClick={() => void saveSettings(draft)}>
            {t("app.save")}
          </Button>
        }
      />
      <div className="settings-grid">
        <Card className="settings-card">
          <h3>{t("settings.general")}</h3>
          <Field label={t("settings.alias")} hint={t("settings.aliasHint")}>
            <TextInput value={draft.alias ?? ""} placeholder={identity?.alias} onChange={(e) => patch((s) => ({ ...s, alias: e.target.value || null }))} />
          </Field>
          <Field label={t("settings.receiveDir")}>
            <div className="row">
              <TextInput readOnly value={draft.receiveDir ?? ""} placeholder={identity?.receiveDir ?? t("settings.default")} />
              {!platform.mobile && (
                <Button
                  variant="outline"
                  size="sm"
                  onClick={async () => {
                    const dir = await pickFolder();
                    if (dir) patch((s) => ({ ...s, receiveDir: dir }));
                  }}
                >
                  {t("settings.choose")}
                </Button>
              )}
              {draft.receiveDir && (
                <Button variant="ghost" size="sm" icon="x" onClick={() => patch((s) => ({ ...s, receiveDir: null }))} />
              )}
            </div>
          </Field>
          <Field label={t("settings.organize")}>
            <div className="check-row">
              <Toggle checked={draft.organize.byDevice} onChange={(v) => patch((s) => ({ ...s, organize: { ...s.organize, byDevice: v } }))} label={t("settings.byDevice")} />
              <Toggle checked={draft.organize.byDate} onChange={(v) => patch((s) => ({ ...s, organize: { ...s.organize, byDate: v } }))} label={t("settings.byDate")} />
              <Toggle checked={draft.organize.byType} onChange={(v) => patch((s) => ({ ...s, organize: { ...s.organize, byType: v } }))} label={t("settings.byType")} />
            </div>
          </Field>
          <Field label={t("settings.onConflict")}>
            <Select
              value={draft.onConflict}
              onChange={(v) => patch((s) => ({ ...s, onConflict: v }))}
              options={[
                { value: "rename", label: t("settings.conflict.rename") },
                { value: "overwrite", label: t("settings.conflict.overwrite") },
                { value: "ask", label: t("settings.conflict.ask") },
              ]}
            />
          </Field>
          <Field label={t("settings.theme")}>
            <Select
              value={draft.app.theme}
              onChange={(v) => patch((s) => ({ ...s, app: { ...s.app, theme: v } }))}
              options={[
                { value: "dark", label: t("settings.theme.dark") },
                { value: "light", label: t("settings.theme.light") },
                { value: "system", label: t("settings.theme.system") },
              ]}
            />
          </Field>
          <Field label={t("settings.language")}>
            <Select<Locale>
              value={locale || getLocale()}
              onChange={(v) => switchLocale(v)}
              options={[
                { value: "zh", label: "中文" },
                { value: "en", label: "English" },
              ]}
            />
          </Field>
        </Card>

        <Card className="settings-card">
          <h3>{t("settings.network")}</h3>
          <Field label={t("settings.port")}>
            <TextInput type="number" min={1} max={65535} value={draft.port} onChange={(e) => patch((s) => ({ ...s, port: number(e.target.value, s.port) }))} />
          </Field>
          <Field label={t("settings.pin")} hint={t("settings.pinHint")}>
            <TextInput inputMode="numeric" value={draft.pin ?? ""} onChange={(e) => patch((s) => ({ ...s, pin: e.target.value.trim() || null }))} />
          </Field>
          <Toggle checked={draft.requireClientCerts} onChange={(v) => patch((s) => ({ ...s, requireClientCerts: v }))} label={t("settings.requireClientCerts")} />
          <Toggle checked={draft.ipv6} onChange={(v) => patch((s) => ({ ...s, ipv6: v }))} label={t("settings.ipv6")} />
          <Toggle checked={draft.resume} onChange={(v) => patch((s) => ({ ...s, resume: v }))} label={t("settings.resume")} />
          <Toggle
            checked={draft.createChecksums && draft.verifyChecksums}
            onChange={(v) => patch((s) => ({ ...s, createChecksums: v, verifyChecksums: v }))}
            label={t("settings.checksums")}
          />
          <Toggle checked={draft.skipHiddenFiles} onChange={(v) => patch((s) => ({ ...s, skipHiddenFiles: v }))} label={t("settings.skipHidden")} />
          <Field label={t("settings.parallel")}>
            <TextInput type="number" min={1} max={16} value={draft.parallelUploads} onChange={(e) => patch((s) => ({ ...s, parallelUploads: Math.max(1, number(e.target.value, s.parallelUploads)) }))} />
          </Field>
          <Field label={t("settings.historyLimit")}>
            <TextInput type="number" min={0} value={draft.historyLimit} onChange={(e) => patch((s) => ({ ...s, historyLimit: number(e.target.value, s.historyLimit) }))} />
          </Field>
        </Card>

        {!platform.mobile && (
          <Card className="settings-card">
            <h3>{t("settings.clipboard")}</h3>
            <Toggle checked={draft.clipboard.syncEnabled} onChange={(v) => patch((s) => ({ ...s, clipboard: { ...s.clipboard, syncEnabled: v } }))} label={t("settings.syncEnabled")} />
            <Field label={`${t("settings.textLimit")} (MB)`}>
              <TextInput type="number" min={0} step={0.5} value={draft.clipboard.textLimit / MB} onChange={(e) => patch((s) => ({ ...s, clipboard: { ...s.clipboard, textLimit: Math.round(number(e.target.value, 1) * MB) } }))} />
            </Field>
            <Field label={`${t("settings.imageLimit")} (MB)`}>
              <TextInput type="number" min={0} step={1} value={draft.clipboard.imageLimit / MB} onChange={(e) => patch((s) => ({ ...s, clipboard: { ...s.clipboard, imageLimit: Math.round(number(e.target.value, 10) * MB) } }))} />
            </Field>
            <Field label={t("settings.clipHistory")}>
              <TextInput type="number" min={0} value={draft.clipboard.historyLimit} onChange={(e) => patch((s) => ({ ...s, clipboard: { ...s.clipboard, historyLimit: number(e.target.value, s.clipboard.historyLimit) } }))} />
            </Field>
            <Toggle checked={draft.clipboard.neverStoreText} onChange={(v) => patch((s) => ({ ...s, clipboard: { ...s.clipboard, neverStoreText: v } }))} label={t("settings.neverStoreText")} />
          </Card>
        )}

        <Card className="settings-card">
          <h3>{t("settings.app")}</h3>
          {!platform.mobile && (
            <>
              <Field label={t("settings.shortcut")}>
                <TextInput value={draft.app.globalShortcut} onChange={(e) => patch((s) => ({ ...s, app: { ...s.app, globalShortcut: e.target.value } }))} placeholder="CmdOrCtrl+Shift+V" />
              </Field>
              <Toggle checked={draft.app.closeToTray} onChange={(v) => patch((s) => ({ ...s, app: { ...s.app, closeToTray: v } }))} label={t("settings.closeToTray")} />
            </>
          )}
          <Toggle checked={draft.app.autoAcceptPaired} onChange={(v) => patch((s) => ({ ...s, app: { ...s.app, autoAcceptPaired: v } }))} label={t("settings.autoAcceptPaired")} />
          <Toggle checked={draft.app.notifications} onChange={(v) => patch((s) => ({ ...s, app: { ...s.app, notifications: v } }))} label={t("settings.notifications")} />
          {identity && (
            <p className="muted breakable" style={{ margin: 0, fontSize: 12, userSelect: "text", WebkitUserSelect: "text" }}>
              {t("app.fingerprint")}: <span className="mono">{identity.fingerprint}</span>
              <br />
              lan-send {platform.version} · {identity.deviceModel} · {identity.configDir}
            </p>
          )}
        </Card>
      </div>
    </>
  );
}
