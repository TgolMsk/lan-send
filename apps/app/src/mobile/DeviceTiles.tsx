// iOS devices page (ADR-0025): one tile per device, like Control Center.
// Tapping a tile opens its card (DeviceSheet); the running transfer sits on
// top as a wide tile.

import { useState } from "react";
import { PageHead } from "../components/Layout";
import { Icon } from "../components/icons";
import { Card, DeviceIcon, EmptyState, GlassButton, Ring } from "../components/ui";
import { formatAgo, formatBytes, formatPercent, formatSpeed } from "../format";
import { t } from "../i18n";
import { isTauri } from "../ipc";
import { matchDevice, navigate, refreshDevices, useStore } from "../store";
import type { DeviceView } from "../types";
import { AddressSheet } from "./AddressSheet";
import { DeviceSheet } from "./DeviceSheet";

const finalStates = ["finished", "failed", "cancelled", "declined"];

export function DeviceTiles() {
  const devices = useStore((s) => s.devices);
  const identity = useStore((s) => s.identity);
  const [query, setQuery] = useState("");
  const [searching, setSearching] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [addressOpen, setAddressOpen] = useState(false);
  // `?sheet=<fingerprint>` opens a card in the mock frontend, for screenshots.
  const [openId, setOpenId] = useState<string | null>(() => (isTauri ? null : new URLSearchParams(window.location.search).get("sheet")));
  const open = devices.find((d) => d.fingerprint === openId) ?? null;
  const shown = devices.filter((d) => matchDevice(d, query));

  const refresh = async () => {
    setRefreshing(true);
    await refreshDevices();
    window.setTimeout(() => setRefreshing(false), 800);
  };

  return (
    <>
      <PageHead
        title={t("devices.title")}
        actions={
          <>
            <GlassButton
              label={t("app.search")}
              icon="search"
              className={searching ? "on" : ""}
              onClick={() => {
                if (searching) setQuery("");
                setSearching(!searching);
              }}
            />
            <GlassButton label={t("devices.refresh")} icon="refresh" className={refreshing ? "busy" : ""} disabled={refreshing} onClick={() => void refresh()} />
          </>
        }
      />
      {searching && (
        <label className="m-search">
          <Icon name="search" size={18} />
          <input autoFocus type="search" placeholder={t("app.search")} value={query} onChange={(e) => setQuery(e.target.value)} />
        </label>
      )}
      {identity?.multicastError && (
        <div className="banner" style={{ marginBottom: 12 }}>
          <span className="muted">{identity.multicastError}</span>
        </div>
      )}
      <LiveTile />
      {devices.length === 0 && (
        <Card style={{ marginBottom: 12 }}>
          <EmptyState icon="devices" title={t("devices.empty")} hint={t("devices.emptyHint")} />
        </Card>
      )}
      <div className="m-tiles">
        {shown.map((device) => (
          <DeviceTile key={device.fingerprint} device={device} onOpen={() => setOpenId(device.fingerprint)} />
        ))}
        {!query && (
          <button type="button" className="m-tile add" onClick={() => setAddressOpen(true)}>
            <span className="m-add-icon">
              <Icon name="plus" size={22} />
            </span>
            <span>{t("devices.direct")}</span>
          </button>
        )}
      </div>
      {open && <DeviceSheet device={open} onClose={() => setOpenId(null)} />}
      <AddressSheet open={addressOpen} onClose={() => setAddressOpen(false)} />
    </>
  );
}

function DeviceTile({ device, onOpen }: { device: DeviceView; onOpen: () => void }) {
  const detail = device.online
    ? [device.deviceModel ?? device.deviceType, device.paired ? t("devices.paired") : device.host].filter(Boolean).join(" · ")
    : `${t("devices.lastSeen")} ${formatAgo(device.lastSeen)}`;
  return (
    <button type="button" className={`m-tile ${device.online ? "" : "offline"}`} data-device={device.fingerprint} onClick={onOpen}>
      <span className="m-tile-top">
        <DeviceIcon type={device.deviceType} model={device.deviceModel} online={device.online} />
        {device.favorite && <Icon name="star" size={20} fill="currentColor" className="m-fav" />}
      </span>
      <span className="m-tile-name">{device.displayName}</span>
      <span className="m-tile-detail">
        {device.online && device.paired && <Icon name="link" size={12} className="m-link" />}
        {detail || "—"}
      </span>
    </button>
  );
}

/** The first running transfer; tapping it opens the transfers page. */
function LiveTile() {
  const transfers = useStore((s) => s.transfers);
  const speeds = useStore((s) => s.speeds);
  const running = transfers.filter((tr) => !finalStates.includes(tr.state));
  const hero = running[0];
  if (!hero) return null;
  const samples = speeds[hero.id] ?? [];
  const speed = samples.length ? samples[samples.length - 1] : 0;
  const detail =
    hero.state === "active"
      ? `${formatPercent(hero.doneSize, hero.totalSize)} · ${formatBytes(hero.doneSize)} / ${formatBytes(hero.totalSize)} · ${formatSpeed(speed)}`
      : t(`transfers.state.${hero.state}`);
  return (
    <button type="button" className="m-live" onClick={() => navigate("transfers")}>
      <Ring value={hero.totalSize ? hero.doneSize / hero.totalSize : 0} size={44} stroke={4}>
        <Icon name={hero.direction === "send" ? "upload" : "download"} size={18} />
      </Ring>
      <span className="m-live-text">
        <strong>
          {t(`transfers.direction.${hero.direction}`)} {hero.peerAlias}
        </strong>
        <span className="mono">
          {detail}
          {running.length > 1 ? ` · +${running.length - 1}` : ""}
        </span>
      </span>
      <Icon name="chevron" size={14} className="m-chevron" />
    </button>
  );
}
