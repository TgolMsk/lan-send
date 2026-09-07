import { useEffect, useState } from "react";
import { PageHead } from "../components/Layout";
import { Icon } from "../components/icons";
import { Badge, Button, Card, EmptyState, IconButton, Toggle } from "../components/ui";
import { ConfirmDialog } from "../dialogs/ConfirmDialog";
import { formatAgo, formatBytes, baseName } from "../format";
import { t } from "../i18n";
import { isTauri } from "../ipc";
import { clipboardClear, clipboardCopy, clipboardDelete, clipboardPush, loadClipboard, setClipboardSync, useStore } from "../store";
import type { ClipboardView } from "../types";

function ImageThumb({ item }: { item: ClipboardView }) {
  const [src, setSrc] = useState<string | null>(null);
  useEffect(() => {
    let cancelled = false;
    if (item.imagePath && isTauri) {
      import("@tauri-apps/api/core").then(({ convertFileSrc }) => {
        if (!cancelled) setSrc(convertFileSrc(item.imagePath as string));
      });
    }
    return () => {
      cancelled = true;
    };
  }, [item.imagePath]);
  return <div className="thumb">{src ? <img src={src} alt="" /> : <Icon name="image" size={18} />}</div>;
}

export function ClipboardPage() {
  const items = useStore((s) => s.clipboardItems);
  const sync = useStore((s) => s.clipboardSync);
  const settings = useStore((s) => s.settings);
  const identity = useStore((s) => s.identity);
  const pairedCount = useStore((s) => s.devices.filter((d) => d.paired).length);
  const [clearing, setClearing] = useState(false);
  const [pushing, setPushing] = useState(false);

  useEffect(() => {
    void loadClipboard();
  }, []);

  if (identity && !identity.clipboardSupported) {
    return (
      <>
        <PageHead title={t("clipboard.title")} />
        <Card>
          <EmptyState icon="clipboard" title={t("clipboard.unsupported")} />
        </Card>
      </>
    );
  }

  return (
    <>
      <PageHead
        title={t("clipboard.title")}
        subtitle={t("clipboard.subtitle")}
        actions={
          items.length > 0 ? (
            <Button variant="outline" icon="trash" onClick={() => setClearing(true)}>
              {t("app.clear")}
            </Button>
          ) : undefined
        }
      />
      <Card solid>
        <div className="row between wrap">
          <div className="row" style={{ gap: 16 }}>
            <Toggle checked={sync.active} onChange={(value) => void setClipboardSync(value)} label={<strong>{t("clipboard.sync")}</strong>} />
            <span className="muted">
              {sync.active ? t("clipboard.syncOn") : t("clipboard.syncOff")} · {pairedCount} {t("clipboard.peers")}
            </span>
          </div>
          <div className="row">
            {settings?.app.globalShortcut && <span className="kbd">{settings.app.globalShortcut}</span>}
            <Button
              icon="bolt"
              disabled={pairedCount === 0 || pushing}
              onClick={async () => {
                setPushing(true);
                await clipboardPush();
                setPushing(false);
              }}
            >
              {t("clipboard.push")}
            </Button>
          </div>
        </div>
        {pairedCount === 0 && <p className="muted" style={{ margin: "12px 0 0" }}>{t("clipboard.noPeers")}</p>}
      </Card>

      <h2 className="section-title">{t("clipboard.history")}</h2>
      <Card>
        {items.length === 0 ? (
          <EmptyState icon="clipboard" title={t("clipboard.empty")} />
        ) : (
          <div className="list">
            {items.map((item) => (
              <div key={item.id} className="list-row">
                {item.kind === "image" ? (
                  <ImageThumb item={item} />
                ) : (
                  <div className="thumb">
                    <Icon name={item.kind === "files" ? "folder" : "text"} size={18} />
                  </div>
                )}
                <div style={{ minWidth: 0 }}>
                  {item.kind === "text" ? (
                    <div className="clip-text">{item.text}</div>
                  ) : item.kind === "files" ? (
                    <div className="primary">{item.filePaths.map(baseName).join(", ")}</div>
                  ) : (
                    <div className="primary">
                      {t("clipboard.image")} {item.imageWidth}×{item.imageHeight}
                    </div>
                  )}
                  <div className="secondary">
                    <Badge tone={item.fromSelf ? "indigo" : "mint"}>{item.fromSelf ? t("clipboard.fromSelf") : item.originAlias ?? item.origin.slice(0, 8)}</Badge>{" "}
                    {formatAgo(item.createdAt / 1000)} · {formatBytes(item.size)}
                  </div>
                </div>
                <div className="actions">
                  <IconButton label={t("clipboard.copyBack")} icon="copy" onClick={() => void clipboardCopy(item.id)} />
                  <IconButton label={t("app.delete")} icon="trash" onClick={() => void clipboardDelete(item.id)} />
                </div>
              </div>
            ))}
          </div>
        )}
      </Card>
      <ConfirmDialog
        open={clearing}
        title={t("app.clear")}
        text={t("clipboard.history")}
        danger
        onClose={() => setClearing(false)}
        onConfirm={() => {
          void clipboardClear();
          setClearing(false);
        }}
      />
    </>
  );
}
