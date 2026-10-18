import { KindMark } from "../components/FileThumb";
import { Icon } from "../components/icons";
import { Avatar, Badge, Button, DeviceIcon, Modal } from "../components/ui";
import { formatBytes } from "../format";
import { t } from "../i18n";
import { respondIncoming, useStore } from "../store";
import type { IncomingRequestView } from "../types";

export function IncomingDialog() {
  const request = useStore((s) => s.incoming[0]);
  const mobile = useStore((s) => s.platform.mobile);
  if (!request) return null;
  if (mobile) return <IncomingBanner request={request} />;
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
        {request.resumable && <Badge tone="accent">{t("incoming.resumable")}</Badge>}
        {request.clipboardIntent && <Badge tone="success">{t("incoming.clipboard")}</Badge>}
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

/** iOS (ADR-0025): a dark banner that grows out of the Dynamic Island area
 *  instead of a dialog in the middle of the page. */
function IncomingBanner({ request }: { request: IncomingRequestView }) {
  const device = useStore((s) => s.devices.find((d) => d.fingerprint === request.peerFingerprint));
  const preview = request.files.slice(0, 2);
  const extra = request.files.length - preview.length;
  return (
    <section className="m-island" role="alertdialog" aria-label={t("incoming.title")}>
      <div className="m-island-row">
        <DeviceIcon type={device?.deviceType ?? null} model={device?.deviceModel ?? null} />
        <div className="m-island-text">
          <strong>{request.peerAlias}</strong>
          <span>
            {t("incoming.wants", { n: request.files.length, size: formatBytes(request.totalSize) })}
            {request.resumable ? ` · ${t("incoming.resumable")}` : ""}
          </span>
        </div>
      </div>
      <div className="m-island-files">
        {preview.map((file) => (
          <span key={file.id} className="m-island-file">
            <KindMark mime={file.mime} name={file.name} iconSize={14} />
            <span>{file.name}</span>
          </span>
        ))}
        {extra > 0 && <span className="m-island-file more">+{extra}</span>}
        {request.clipboardIntent && <span className="m-island-tag">{t("incoming.clipboard")}</span>}
      </div>
      <div className="m-island-actions">
        <button type="button" onClick={() => void respondIncoming(request.sessionId, false)}>
          {t("incoming.decline")}
        </button>
        <button type="button" className="accept" onClick={() => void respondIncoming(request.sessionId, true)}>
          <Icon name="download" size={18} />
          {t("incoming.accept")}
        </button>
      </div>
    </section>
  );
}

