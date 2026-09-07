import { PageHead } from "../components/Layout";
import { Icon } from "../components/icons";
import { Badge, Button, Card, EmptyState, Progress, Sparkline, Stat } from "../components/ui";
import { formatBytes, formatDuration, formatPercent, formatSpeed } from "../format";
import { t } from "../i18n";
import { cancelTransfer, dismissTransfer, navigate, revealPath, useStore } from "../store";
import type { TransferView } from "../types";

const finalStates = ["finished", "failed", "cancelled", "declined"];

function stateTone(state: TransferView["state"]): "mint" | "muted" | "danger" | "indigo" {
  if (state === "finished") return "mint";
  if (state === "failed") return "danger";
  if (state === "active") return "indigo";
  return "muted";
}

export function TransfersPage() {
  const transfers = useStore((s) => s.transfers);
  const speeds = useStore((s) => s.speeds);
  const platform = useStore((s) => s.platform);
  const running = transfers.filter((tr) => !finalStates.includes(tr.state));
  const hero = running[0];
  const rest = transfers.filter((tr) => tr !== hero);

  return (
    <>
      <PageHead title={t("transfers.title")} subtitle={t("transfers.subtitle")} />
      {transfers.length === 0 && (
        <Card>
          <EmptyState
            icon="transfers"
            title={t("transfers.empty")}
            hint={t("transfers.emptyHint")}
            action={
              <Button icon="devices" onClick={() => navigate("devices")}>
                {t("nav.devices")}
              </Button>
            }
          />
        </Card>
      )}
      {hero && <HeroTransfer transfer={hero} speeds={speeds[hero.id] ?? []} />}
      {rest.length > 0 && (
        <div className="stack" style={{ marginTop: 16 }}>
          {rest.map((transfer) => (
            <Card key={transfer.id}>
              <div className="row between">
                <div className="row">
                  <Icon name={transfer.direction === "send" ? "upload" : "download"} size={18} />
                  <div>
                    <div style={{ fontWeight: 600 }}>
                      {t(`transfers.direction.${transfer.direction}`)} {transfer.peerAlias}
                      {transfer.clipboardIntent && <span className="muted"> · {t("transfers.clipboardFiles")}</span>}
                    </div>
                    <div className="muted" style={{ fontSize: 12 }}>
                      {transfer.files.length} {t("transfers.files")} · {formatBytes(transfer.totalSize)}
                      {transfer.error ? ` · ${transfer.error}` : ""}
                    </div>
                  </div>
                </div>
                <div className="row">
                  <Badge tone={stateTone(transfer.state)}>{t(`transfers.state.${transfer.state}`)}</Badge>
                  {finalStates.includes(transfer.state) ? (
                    <>
                      {!platform.mobile && transfer.direction === "receive" && transfer.state === "finished" && transfer.files[0]?.path && (
                        <Button size="sm" variant="ghost" icon="folder" onClick={() => void revealPath(transfer.files[0].path as string)}>
                          {t("app.reveal")}
                        </Button>
                      )}
                      <Button size="sm" variant="ghost" icon="x" onClick={() => void dismissTransfer(transfer.id)}>
                        {t("transfers.dismiss")}
                      </Button>
                    </>
                  ) : (
                    <Button size="sm" variant="danger" onClick={() => void cancelTransfer(transfer.id)}>
                      {t("app.cancel")}
                    </Button>
                  )}
                </div>
              </div>
              {!finalStates.includes(transfer.state) && (
                <div style={{ marginTop: 12 }}>
                  <Progress value={transfer.totalSize ? transfer.doneSize / transfer.totalSize : 0} />
                </div>
              )}
            </Card>
          ))}
        </div>
      )}
    </>
  );
}

function HeroTransfer({ transfer, speeds }: { transfer: TransferView; speeds: number[] }) {
  const speed = speeds.length ? speeds[speeds.length - 1] : 0;
  const remaining = transfer.totalSize - transfer.doneSize;
  const eta = speed > 0 ? remaining / speed : Number.NaN;
  const ratio = transfer.totalSize ? transfer.doneSize / transfer.totalSize : 0;
  const waiting = transfer.state !== "active";
  return (
    <Card solid>
      <div className="transfer-hero">
        <div className="stack">
          <div className="row">
            <Icon name={transfer.direction === "send" ? "upload" : "download"} size={18} />
            <span style={{ fontWeight: 600 }}>
              {t(`transfers.direction.${transfer.direction}`)} {transfer.peerAlias}
            </span>
            <Badge tone={waiting ? "muted" : "mint"}>{t(`transfers.state.${transfer.state}`)}</Badge>
          </div>
          <div className="big-number mono">
            {formatPercent(transfer.doneSize, transfer.totalSize)}
            <small>{formatBytes(transfer.doneSize)} / {formatBytes(transfer.totalSize)}</small>
          </div>
          <Progress value={ratio} />
          <div className="row wrap" style={{ gap: 28 }}>
            <Stat label={t("transfers.speed")} value={formatSpeed(speed)} />
            <Stat label={t("transfers.remaining")} value={formatDuration(eta)} />
            <Stat label={t("transfers.filesLabel")} value={`${transfer.files.filter((f) => f.state === "finished").length} / ${transfer.files.length}`} />
            <span className="spacer" />
            <Button size="sm" variant="danger" onClick={() => void cancelTransfer(transfer.id)}>
              {t("app.cancel")}
            </Button>
          </div>
        </div>
        <Sparkline values={speeds} />
      </div>
      <div style={{ marginTop: 14 }}>
        {transfer.files.slice(0, 12).map((file) => (
          <div key={file.id} className="file-row">
            <Icon name={file.mime.startsWith("image/") ? "image" : "file"} size={16} />
            <div style={{ minWidth: 0 }}>
              <div className="name">{file.name}</div>
              {(file.state === "active" || file.state === "pending") && <Progress value={file.size ? file.done / file.size : 0} />}
            </div>
            <span className="muted mono" style={{ fontSize: 12 }}>
              {file.state === "finished" ? "✓ " : file.state === "failed" ? "✗ " : ""}
              {formatBytes(file.size)}
            </span>
          </div>
        ))}
        {transfer.files.length > 12 && (
          <div className="muted" style={{ paddingTop: 8, fontSize: 12 }}>
            {t("incoming.more", { n: transfer.files.length - 12 })}
          </div>
        )}
      </div>
    </Card>
  );
}
