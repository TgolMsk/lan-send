import { useEffect, useState } from "react";
import { PageHead } from "../components/Layout";
import { Icon } from "../components/icons";
import { Badge, Button, Card, EmptyState, IconButton } from "../components/ui";
import { ConfirmDialog } from "../dialogs/ConfirmDialog";
import { formatBytes, formatTime } from "../format";
import { t } from "../i18n";
import { clearHistory, deleteHistory, loadHistory, openPath, revealPath, useStore } from "../store";

export function HistoryPage() {
  const history = useStore((s) => s.history);
  const platform = useStore((s) => s.platform);
  const [clearing, setClearing] = useState(false);
  useEffect(() => {
    void loadHistory();
  }, []);
  const sent = history.filter((r) => r.direction === "send" && r.status === "finished").length;
  const received = history.filter((r) => r.direction === "receive" && r.status === "finished").length;
  return (
    <>
      <PageHead
        title={t("history.title")}
        subtitle={t("history.subtitle")}
        actions={
          <>
            <Badge tone="indigo">
              <Icon name="upload" size={13} /> {sent} {t("history.sent")}
            </Badge>
            <Badge tone="mint">
              <Icon name="download" size={13} /> {received} {t("history.received")}
            </Badge>
            {history.length > 0 && (
              <Button variant="outline" icon="trash" onClick={() => setClearing(true)}>
                {t("app.clear")}
              </Button>
            )}
          </>
        }
      />
      <Card>
        {history.length === 0 ? (
          <EmptyState icon="history" title={t("history.empty")} />
        ) : (
          <div className="list">
            {history.map((record) => (
              <div key={record.id} className="list-row">
                <div className="thumb">
                  <Icon name={record.mime.startsWith("image/") ? "image" : record.direction === "send" ? "upload" : "download"} size={18} />
                </div>
                <div style={{ minWidth: 0 }}>
                  <div className="primary">{record.fileName}</div>
                  <div className="secondary">
                    {record.direction === "send" ? `→ ${record.peerAlias}` : `← ${record.peerAlias}`} · {formatBytes(record.size)} · {formatTime(record.finishedAt ?? record.startedAt)}
                    {record.status !== "finished" && (
                      <>
                        {" "}
                        · <Badge tone={record.status === "failed" ? "danger" : "muted"}>{t(`history.status.${record.status}`)}</Badge>
                      </>
                    )}
                    {record.error ? ` · ${record.error}` : ""}
                  </div>
                </div>
                <div className="actions">
                  {!platform.mobile && record.path && record.status === "finished" && (
                    <>
                      <IconButton label={t("app.open")} icon="open" onClick={() => void openPath(record.path as string)} />
                      <IconButton label={t("app.reveal")} icon="folder" onClick={() => void revealPath(record.path as string)} />
                    </>
                  )}
                  <IconButton label={t("app.delete")} icon="trash" onClick={() => void deleteHistory(record.id)} />
                </div>
              </div>
            ))}
          </div>
        )}
      </Card>
      <ConfirmDialog
        open={clearing}
        title={t("app.clear")}
        text={t("history.clearConfirm")}
        danger
        onClose={() => setClearing(false)}
        onConfirm={() => {
          void clearHistory();
          setClearing(false);
        }}
      />
    </>
  );
}
