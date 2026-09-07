import { useState } from "react";
import { PageHead } from "../components/Layout";
import { Badge, Button, Card, DeviceIcon, EmptyState, Field, IconButton, Modal, TextInput } from "../components/ui";
import { ConfirmDialog } from "../dialogs/ConfirmDialog";
import { formatAgo } from "../format";
import { t } from "../i18n";
import {
  forgetDevice,
  openSend,
  refreshDevices,
  renameDevice,
  setFavorite,
  startPair,
  unpair,
  updateSendDraft,
  useStore,
} from "../store";
import type { DeviceView } from "../types";

export function DevicesPage() {
  const devices = useStore((s) => s.devices);
  const dragging = useStore((s) => s.dragging);
  const identity = useStore((s) => s.identity);
  const [query, setQuery] = useState("");
  const [address, setAddress] = useState("");
  const [renaming, setRenaming] = useState<DeviceView | null>(null);
  const [forgetting, setForgetting] = useState<DeviceView | null>(null);
  const [alias, setAlias] = useState("");
  const [refreshing, setRefreshing] = useState(false);

  const online = devices.filter((d) => d.online).length;
  const shown = devices.filter((d) => {
    const q = query.trim().toLowerCase();
    return !q || d.displayName.toLowerCase().includes(q) || d.alias.toLowerCase().includes(q) || (d.host ?? "").includes(q) || d.fingerprint.toLowerCase().startsWith(q);
  });

  const refresh = async () => {
    setRefreshing(true);
    await refreshDevices();
    window.setTimeout(() => setRefreshing(false), 800);
  };

  return (
    <>
      <PageHead
        title={t("devices.title")}
        subtitle={t("devices.subtitle")}
        actions={
          <>
            <Badge tone={online > 0 ? "mint" : "muted"}>
              <span className={`dot ${online > 0 ? "on" : ""}`} /> {online} {t("devices.onlineCount")}
            </Badge>
            <TextInput placeholder={t("app.search")} value={query} onChange={(e) => setQuery(e.target.value)} style={{ width: 180 }} />
            <Button variant="outline" icon="refresh" onClick={() => void refresh()} disabled={refreshing}>
              {t("devices.refresh")}
            </Button>
          </>
        }
      />
      {identity?.multicastError && (
        <div className="banner" style={{ marginBottom: 16 }}>
          <span className="muted">{identity.multicastError}</span>
        </div>
      )}
      {dragging && <p className="muted" style={{ margin: "0 0 12px" }}>{t("devices.dropHint")}</p>}
      {shown.length === 0 ? (
        <Card>
          <EmptyState
            icon="devices"
            title={t("devices.empty")}
            hint={t("devices.emptyHint")}
            action={
              <Button variant="outline" icon="refresh" onClick={() => void refresh()}>
                {t("devices.refresh")}
              </Button>
            }
          />
        </Card>
      ) : (
        <div className="grid devices">
          {shown.map((device) => (
            <Card
              key={device.fingerprint}
              interactive
              className={`device-card ${device.online ? "" : "offline"} ${dragging && device.online ? "drop-target" : ""}`}
              data-device={device.fingerprint}
              onDoubleClick={() => device.online && openSend(device)}
            >
              <div className="device-head">
                <DeviceIcon type={device.deviceType} model={device.deviceModel} />
                <div style={{ minWidth: 0, flex: 1 }}>
                  <div className="device-name">{device.displayName}</div>
                  <div className="device-meta">
                    {device.deviceModel ?? device.deviceType ?? "—"}
                    {device.host ? ` · ${device.host}${device.port && device.port !== 53317 ? `:${device.port}` : ""}` : ""}
                  </div>
                </div>
                <IconButton
                  label={device.favorite ? t("devices.unfavorite") : t("devices.favorite")}
                  icon="star"
                  on={device.favorite}
                  onClick={() => void setFavorite(device, !device.favorite)}
                />
              </div>
              <div className="row wrap">
                <Badge tone={device.online ? "mint" : "muted"}>
                  <span className={`dot ${device.online ? "on" : ""}`} /> {device.online ? t("app.online") : `${t("devices.lastSeen")} ${formatAgo(device.lastSeen)}`}
                </Badge>
                {device.paired && (
                  <Badge tone="indigo">
                    <span>🔗</span> {t("devices.paired")}
                  </Badge>
                )}
              </div>
              <div className="device-foot">
                <Button size="sm" icon="send" disabled={!device.online} onClick={() => openSend(device)}>
                  {t("devices.send")}
                </Button>
                {device.paired ? (
                  <Button size="sm" variant="outline" icon="unlink" onClick={() => void unpair(device)}>
                    {t("devices.unpair")}
                  </Button>
                ) : (
                  <Button size="sm" variant="outline" icon="link" disabled={!device.online} onClick={() => void startPair(device)}>
                    {t("devices.pair")}
                  </Button>
                )}
                <span className="spacer" />
                <IconButton
                  label={t("devices.rename")}
                  icon="pencil"
                  onClick={() => {
                    setAlias(device.customAlias ?? "");
                    setRenaming(device);
                  }}
                />
                <IconButton label={t("devices.forget")} icon="trash" onClick={() => setForgetting(device)} />
              </div>
            </Card>
          ))}
        </div>
      )}

      <h2 className="section-title">{t("devices.direct")}</h2>
      <Card>
        <form
          className="row"
          onSubmit={(event) => {
            event.preventDefault();
            if (address.trim()) {
              openSend(null);
              updateSendDraft({ address: address.trim() });
            }
          }}
        >
          <TextInput placeholder={t("devices.directPlaceholder")} value={address} onChange={(e) => setAddress(e.target.value)} />
          <Button icon="send" type="submit" disabled={!address.trim()}>
            {t("devices.send")}
          </Button>
          <Button variant="outline" icon="link" type="button" disabled={!address.trim()} onClick={() => void startPair(address.trim())}>
            {t("devices.pair")}
          </Button>
        </form>
      </Card>

      <Modal
        open={renaming !== null}
        title={t("devices.renameTitle")}
        onClose={() => setRenaming(null)}
        footer={
          <>
            <Button variant="ghost" onClick={() => setRenaming(null)}>
              {t("app.cancel")}
            </Button>
            <Button
              onClick={() => {
                if (renaming) void renameDevice(renaming, alias.trim() || null);
                setRenaming(null);
              }}
            >
              {t("app.save")}
            </Button>
          </>
        }
      >
        <Field label={renaming?.alias ?? ""} hint={t("devices.renameHint")}>
          <TextInput autoFocus value={alias} onChange={(e) => setAlias(e.target.value)} placeholder={renaming?.alias} />
        </Field>
      </Modal>

      <ConfirmDialog
        open={forgetting !== null}
        title={forgetting ? `${t("devices.forget")}: ${forgetting.displayName}` : ""}
        text={t("devices.forgetConfirm")}
        danger
        onClose={() => setForgetting(null)}
        onConfirm={() => {
          if (forgetting) void forgetDevice(forgetting);
          setForgetting(null);
        }}
      />
    </>
  );
}
