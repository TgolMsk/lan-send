import { Icon } from "../components/icons";
import { Avatar, Badge, Button, Modal } from "../components/ui";
import { formatBytes } from "../format";
import { t } from "../i18n";
import { respondIncoming, useStore } from "../store";

export function IncomingDialog() {
  const request = useStore((s) => s.incoming[0]);
  if (!request) return null;
  const shown = request.files.slice(0, 8);
  return (
    <Modal
      open
      title={t("incoming.title")}
      footer={
        <>
          <Button variant="outline" onClick={() => void respondIncoming(request.sessionId, false)}>
            {t("incoming.decline")}
          </Button>
          <Button icon="download" autoFocus onClick={() => void respondIncoming(request.sessionId, true)}>
            {t("incoming.accept")}
          </Button>
        </>
      }
    >
      <div className="row" style={{ marginBottom: 14 }}>
        <Avatar name={request.peerAlias} />
        <div>
          <div style={{ fontWeight: 600 }}>{request.peerAlias}</div>
          <div className="muted" style={{ fontSize: 12 }}>
            {request.peerHost} · {request.files.length} {t("transfers.files")} · {formatBytes(request.totalSize)}
          </div>
        </div>
        <span className="spacer" />
        {request.resumable && <Badge tone="indigo">{t("incoming.resumable")}</Badge>}
        {request.clipboardIntent && <Badge tone="mint">{t("incoming.clipboard")}</Badge>}
      </div>
      <div>
        {shown.map((file) => (
          <div key={file.id} className="file-row">
            <Icon name={file.mime.startsWith("image/") ? "image" : "file"} size={16} />
            <span className="name">{file.name}</span>
            <span className="muted mono" style={{ fontSize: 12 }}>
              {formatBytes(file.size)}
            </span>
          </div>
        ))}
        {request.files.length > shown.length && (
          <div className="muted" style={{ paddingTop: 8, fontSize: 12 }}>
            {t("incoming.more", { n: request.files.length - shown.length })}
          </div>
        )}
      </div>
    </Modal>
  );
}
