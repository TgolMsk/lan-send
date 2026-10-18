// iOS transfers page (ADR-0025): the running transfer as one large tile with
// a progress ring, its files as a grid of thumbnails, the rest as small tiles.

import { FileThumb, KindMark, fileKind } from "../components/FileThumb";
import { PageHead } from "../components/Layout";
import { Icon } from "../components/icons";
import { Button, Card, EmptyState, IconButton, Progress, Ring, Stat } from "../components/ui";
import { formatBytes, formatDuration, formatPercent, formatSpeed } from "../format";
import { t } from "../i18n";
import { cancelTransfer, dismissTransfer, errorText, navigate, useStore } from "../store";
import type { TransferView } from "../types";

const finalStates = ["finished", "failed", "cancelled", "declined"];

export function TransferTiles() {
  const transfers = useStore((s) => s.transfers);
  const speeds = useStore((s) => s.speeds);
  const hero = transfers.find((tr) => !finalStates.includes(tr.state));
  const rest = transfers.filter((tr) => tr !== hero);
  return (
    <>
      <PageHead title={t("transfers.title")} />
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
      {hero && <HeroTile transfer={hero} speeds={speeds[hero.id] ?? []} />}
      {hero && <FileGrid transfer={hero} />}
      {rest.length > 0 && (
        <>
          <h2 className="m-section">{t("transfers.recent")}</h2>
          <div className="m-minis">
            {rest.map((transfer) => (
              <MiniTile key={transfer.id} transfer={transfer} />
            ))}
          </div>
        </>
      )}
    </>
  );
}

function HeroTile({ transfer, speeds }: { transfer: TransferView; speeds: number[] }) {
  const speed = speeds.length ? speeds[speeds.length - 1] : 0;
  const eta = speed > 0 ? (transfer.totalSize - transfer.doneSize) / speed : Number.NaN;
  const waiting = transfer.state !== "active";
  return (
    <section className="m-hero" aria-label={`${t(`transfers.direction.${transfer.direction}`)} ${transfer.peerAlias}`}>
      <Ring value={transfer.totalSize ? transfer.doneSize / transfer.totalSize : 0} size={150} stroke={12}>
        <span className="m-hero-pct mono">{formatPercent(transfer.doneSize, transfer.totalSize)}</span>
        <span className="m-hero-size mono">
          {formatBytes(transfer.doneSize)} / {formatBytes(transfer.totalSize)}
        </span>
      </Ring>
      <div className="m-hero-side">
        <div className="m-hero-peer">
          <span>{t(`transfers.direction.${transfer.direction}`)}</span>
          <strong>{transfer.peerAlias}</strong>
        </div>
        {waiting ? (
          <span className="m-hero-state">{t(`transfers.state.${transfer.state}`)}</span>
        ) : (
          <div className="m-hero-stats">
            <Stat label={t("transfers.speed")} value={formatSpeed(speed)} />
            <Stat label={t("transfers.remaining")} value={formatDuration(eta)} />
          </div>
        )}
        <Button size="sm" variant="danger" onClick={() => void cancelTransfer(transfer.id)}>
          {t("app.cancel")}
        </Button>
      </div>
    </section>
  );
}

function FileGrid({ transfer }: { transfer: TransferView }) {
  const done = transfer.files.filter((f) => f.state === "finished").length;
  const shown = transfer.files.slice(0, 12);
  return (
    <>
      <h2 className="m-section">
        {t("transfers.filesLabel")} · {done} / {transfer.files.length}
      </h2>
      <div className="m-files">
        {shown.map((file) => (
          <div key={file.id} className={`m-file ${file.state}`}>
            <div className="m-file-thumb">
              {file.state === "finished" && file.path && ["image", "audio"].includes(fileKind(file.mime, file.name)) ? (
                <FileThumb path={file.path} mime={file.mime} size="lg" />
              ) : (
                <KindMark mime={file.mime} name={file.name} iconSize={28} />
              )}
              {file.state === "finished" && (
                <span className="m-file-mark ok" role="img" aria-label={t("transfers.state.finished")}>
                  <Icon name="check" size={14} />
                </span>
              )}
              {file.state === "failed" && (
                <span className="m-file-mark bad" role="img" aria-label={t("transfers.state.failed")}>
                  <Icon name="x" size={14} />
                </span>
              )}
              {file.state === "active" && (
                <span className="m-file-bar">
                  <Progress value={file.size ? file.done / file.size : 0} />
                </span>
              )}
              {file.state === "pending" && <span className="m-file-wait">{t("transfers.queued")}</span>}
            </div>
            <span className="m-file-name">{file.name}</span>
            <span className="m-file-size mono">
              {file.state === "active" ? `${formatBytes(file.done)} / ${formatBytes(file.size)}` : formatBytes(file.size)}
            </span>
          </div>
        ))}
      </div>
      {transfer.files.length > shown.length && <p className="m-more">{t("incoming.more", { n: transfer.files.length - shown.length })}</p>}
    </>
  );
}

function MiniTile({ transfer }: { transfer: TransferView }) {
  const final = finalStates.includes(transfer.state);
  const tone =
    transfer.state === "finished" ? "ok" : transfer.state === "failed" || transfer.state === "declined" ? "bad" : final ? "" : "accent";
  const detail = transfer.error
    ? errorText(transfer.errorCode, transfer.error)
    : `${t(`transfers.state.${transfer.state}`)} · ${transfer.files.length} ${t("transfers.files")} · ${formatBytes(transfer.totalSize)}`;
  return (
    <div className="m-mini">
      <span className={`m-mini-icon ${tone}`}>
        <Icon name={tone === "bad" ? "x" : transfer.direction === "send" ? "upload" : "download"} size={18} />
      </span>
      <span className="m-mini-text">
        <strong>
          {t(`transfers.direction.${transfer.direction}`)} {transfer.peerAlias}
        </strong>
        <span title={detail}>{detail}</span>
        {!final && <Progress value={transfer.totalSize ? transfer.doneSize / transfer.totalSize : 0} />}
      </span>
      <IconButton
        label={final ? t("transfers.dismiss") : t("app.cancel")}
        icon="x"
        onClick={() => void (final ? dismissTransfer(transfer.id) : cancelTransfer(transfer.id))}
      />
    </div>
  );
}
