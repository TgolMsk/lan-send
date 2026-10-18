// The card a device tile opens on iOS (ADR-0025): two large send buttons,
// then round toggles for favourite, pairing, rename and forget.

import { useState, type ReactNode } from "react";
import { Icon, type IconName } from "../components/icons";
import { DeviceIcon } from "../components/ui";
import { ConfirmDialog } from "../dialogs/ConfirmDialog";
import { RenameDialog } from "../dialogs/RenameDialog";
import { formatAgo, shortFingerprint } from "../format";
import { t } from "../i18n";
import { forgetDevice, pickFiles, sendFiles, setFavorite, startPair, unpair } from "../store";
import type { DeviceView, PickKind } from "../types";

export function DeviceSheet({ device, onClose }: { device: DeviceView; onClose: () => void }) {
  const [renaming, setRenaming] = useState(false);
  const [forgetting, setForgetting] = useState(false);

  // Picking is the confirmation: whatever comes back from the picker goes
  // straight out, without the desktop's review dialog.
  const send = async (kind: PickKind) => {
    const paths = await pickFiles(kind);
    if (!paths.length) return;
    onClose();
    await sendFiles(device.fingerprint, paths);
  };
  const address = device.host ? `${device.host}${device.port && device.port !== 53317 ? `:${device.port}` : ""}` : null;

  return (
    <div className="m-sheet-backdrop" onClick={(event) => event.target === event.currentTarget && onClose()}>
      <section className="m-card" role="dialog" aria-modal aria-label={device.displayName}>
        <button type="button" className="m-close" aria-label={t("app.close")} onClick={onClose}>
          <Icon name="x" size={16} />
        </button>
        <div className="m-card-head">
          <DeviceIcon type={device.deviceType} model={device.deviceModel} className={device.online ? "" : "off"} />
          <div className="m-card-title">
            <h2>{device.displayName}</h2>
            <p>{[device.deviceModel ?? device.deviceType, address].filter(Boolean).join(" · ") || "—"}</p>
            <p className={`m-status ${device.online ? "on" : ""}`}>
              <span className={`dot ${device.online ? "on" : ""}`} />
              {device.online ? t("app.online") : `${t("devices.lastSeen")} ${formatAgo(device.lastSeen)}`}
            </p>
          </div>
        </div>
        <div className="m-send">
          <button type="button" className="m-send-btn primary" disabled={!device.online} onClick={() => void send("media")}>
            <Icon name="image" size={30} />
            {t("devices.sendMedia")}
          </button>
          <button type="button" className="m-send-btn" disabled={!device.online} onClick={() => void send("files")}>
            <Icon name="file" size={30} />
            {t("devices.sendFiles")}
          </button>
        </div>
        <div className="m-toggles">
          <RoundToggle
            icon="star"
            tone={device.favorite ? "fav" : ""}
            pressed={device.favorite}
            onClick={() => void setFavorite(device, !device.favorite)}
          >
            {device.favorite ? t("devices.favorited") : t("devices.favorite")}
          </RoundToggle>
          {device.paired ? (
            <RoundToggle icon="link" tone="accent" pressed onClick={() => void unpair(device)}>
              {t("devices.paired")}
            </RoundToggle>
          ) : (
            <RoundToggle icon="link" pressed={false} disabled={!device.online} onClick={() => void startPair(device)}>
              {t("devices.pair")}
            </RoundToggle>
          )}
          <RoundToggle icon="pencil" onClick={() => setRenaming(true)}>
            {t("devices.rename")}
          </RoundToggle>
          <RoundToggle icon="trash" tone="danger" onClick={() => setForgetting(true)}>
            {t("devices.forgetShort")}
          </RoundToggle>
        </div>
        <p className="m-card-foot mono">
          {t("app.fingerprint")} {shortFingerprint(device.fingerprint)} · {t("app.port")} {device.port ?? 53317}
        </p>
      </section>
      <RenameDialog device={renaming ? device : null} onClose={() => setRenaming(false)} />
      <ConfirmDialog
        open={forgetting}
        title={`${t("devices.forget")}: ${device.displayName}`}
        text={t("devices.forgetConfirm")}
        danger
        onClose={() => setForgetting(false)}
        onConfirm={() => {
          setForgetting(false);
          onClose();
          void forgetDevice(device);
        }}
      />
    </div>
  );
}

function RoundToggle({
  icon,
  tone = "",
  pressed,
  disabled,
  onClick,
  children,
}: {
  icon: IconName;
  tone?: "" | "fav" | "accent" | "danger";
  /** Set for on/off controls; leave out for plain actions. */
  pressed?: boolean;
  disabled?: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button type="button" className={`m-round ${tone}`} aria-pressed={pressed} disabled={disabled} onClick={onClick}>
      <span>
        <Icon name={icon} size={24} fill={icon === "star" && pressed ? "currentColor" : undefined} />
      </span>
      {children}
    </button>
  );
}
