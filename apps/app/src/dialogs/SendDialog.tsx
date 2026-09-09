import { Icon } from "../components/icons";
import { Button, Field, IconButton, Modal, Select, TextInput } from "../components/ui";
import { baseName } from "../format";
import { t } from "../i18n";
import { closeSend, pickFiles, sendFiles, updateSendDraft, useStore } from "../store";

export function SendDialog() {
  const draft = useStore((s) => s.sendDraft);
  const devices = useStore((s) => s.devices.filter((d) => d.online));
  const platform = useStore((s) => s.platform);
  const dragging = useStore((s) => s.dragging);
  if (!draft) return null;
  const target = draft.device?.fingerprint ?? draft.address.trim();
  const title = draft.device ? t("send.title", { alias: draft.device.displayName }) : t("devices.direct");
  const addPaths = (paths: string[]) => {
    if (paths.length) updateSendDraft({ paths: Array.from(new Set([...draft.paths, ...paths])) });
  };
  return (
    <Modal
      open
      title={title}
      onClose={closeSend}
      footer={
        <>
          <Button variant="ghost" onClick={closeSend}>
            {t("app.cancel")}
          </Button>
          <Button icon="send" disabled={!target || draft.paths.length === 0} onClick={() => void sendFiles(target, draft.paths)}>
            {t("send.start")}
          </Button>
        </>
      }
    >
      {!draft.device && (
        <div className="stack" style={{ marginBottom: 14 }}>
          {devices.length > 0 && (
            <Field label={t("nav.devices")}>
              <Select
                value={draft.address && devices.some((d) => d.fingerprint === draft.address) ? draft.address : ""}
                onChange={(v) => updateSendDraft({ address: v })}
                options={[{ value: "", label: "—" }, ...devices.map((d) => ({ value: d.fingerprint, label: `${d.displayName} · ${d.host ?? ""}` }))]}
              />
            </Field>
          )}
          <Field label={t("devices.direct")}>
            <TextInput placeholder={t("devices.directPlaceholder")} value={draft.address} onChange={(e) => updateSendDraft({ address: e.target.value })} />
          </Field>
        </div>
      )}
      <div className="row" style={{ marginBottom: 12 }}>
        {platform.mobile && (
          <Button variant="outline" icon="image" onClick={async () => addPaths(await pickFiles("media"))}>
            {t("send.pickMedia")}
          </Button>
        )}
        <Button variant="outline" icon="file" onClick={async () => addPaths(await pickFiles("files"))}>
          {t("send.pick")}
        </Button>
        {!platform.mobile && (
          <Button variant="outline" icon="folder" onClick={async () => addPaths(await pickFiles("folders"))}>
            {t("send.pickFolder")}
          </Button>
        )}
      </div>
      {draft.paths.length === 0 ? (
        <div className={`drop-zone ${dragging ? "active" : ""}`}>
          <Icon name="upload" size={22} />
          <div style={{ marginTop: 6 }}>{t("send.dropHere")}</div>
        </div>
      ) : (
        <ul className="path-list">
          {draft.paths.map((path) => (
            <li key={path}>
              <Icon name="file" size={14} />
              <span title={path}>{baseName(path)}</span>
              <IconButton label={t("app.delete")} icon="x" onClick={() => updateSendDraft({ paths: draft.paths.filter((p) => p !== path) })} />
            </li>
          ))}
        </ul>
      )}
      {draft.paths.length > 0 && (
        <p className="muted" style={{ margin: "8px 0 0", fontSize: 12 }}>
          {draft.paths.length} {t("send.items")}
        </p>
      )}
    </Modal>
  );
}
