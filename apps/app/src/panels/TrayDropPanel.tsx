// macOS menu bar drop panel (ADR-0022): appears under the menu bar icon
// while files are dragged onto it; dropping on a row sends the files there.
// It never takes focus, so the app stays in the background; results arrive
// as system notifications.
import { useEffect, useRef, useState } from "react";
import { DeviceIcon } from "../components/ui";
import { formatBytes } from "../format";
import { ipc, type RuntimeEvent } from "../ipc";
import { setLanguage, t, type Locale } from "../i18n";
import { errorText } from "../store";
import type { DeviceView, Settings, TransferView } from "../types";

type Status = { kind: "idle" } | { kind: "sending"; name: string } | { kind: "error"; message: string };

/** Files dropped on the icon itself wait this long for a device. */
const STAGED_TIMEOUT_MS = 30_000;

function sortOnline(devices: DeviceView[]): DeviceView[] {
  return devices
    .filter((d) => d.online)
    .sort(
      (a, b) =>
        Number(b.favorite) - Number(a.favorite) ||
        Number(b.paired) - Number(a.paired) ||
        a.displayName.localeCompare(b.displayName),
    );
}

// wry reports macOS drag positions in points although Tauri types them as
// physical pixels; this panel only exists on macOS, so no scaling.
function deviceAt(x: number, y: number): string | null {
  return document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-device]")?.dataset.device ?? null;
}

function applyAppearance(settings: Settings) {
  const language = settings.app.language;
  setLanguage(language === "system" || language == null ? "system" : (language as Locale));
  const theme = settings.app.theme;
  const dark = theme === "system" ? window.matchMedia("(prefers-color-scheme: dark)").matches : theme === "dark";
  document.documentElement.dataset.theme = dark ? "dark" : "light";
}

function notifyResult(transfer: TransferView) {
  const alias = transfer.peerAlias;
  let title: string;
  let body: string;
  if (transfer.state === "finished") {
    title = t("trayDrop.notifySent", { alias });
    body = t("trayDrop.notifyFiles", { n: transfer.files.length, size: formatBytes(transfer.totalSize) });
  } else if (transfer.state === "declined") {
    title = t("trayDrop.notifyFailed", { alias });
    body = t("error.declined");
  } else if (transfer.state === "failed") {
    title = t("trayDrop.notifyFailed", { alias });
    body = transfer.error ? errorText(transfer.errorCode, transfer.error) : "";
  } else {
    return; // cancelled by the user: nothing to report
  }
  void ipc.app.notify(title, body);
}

export default function TrayDropPanel() {
  const [devices, setDevices] = useState<DeviceView[]>([]);
  const [hover, setHover] = useState<string | null>(null);
  const [staged, setStaged] = useState<string[]>([]);
  const [status, setStatus] = useState<Status>({ kind: "idle" });
  const stagedRef = useRef<string[]>([]);
  const sendRef = useRef<(fingerprint: string, paths: string[]) => Promise<void>>(async () => {});

  useEffect(() => {
    let disposed = false;
    let hideTimer: number | null = null;
    let notifications = true;
    let devicesNow: DeviceView[] = [];
    /** Transfers started here, reported when they end. */
    const sent = new Set<string>();
    const unlisten: (() => void)[] = [];

    const showDevices = (list: DeviceView[]) => {
      devicesNow = sortOnline(list);
      setDevices(devicesNow);
    };
    const cancelHide = () => {
      if (hideTimer !== null) window.clearTimeout(hideTimer);
      hideTimer = null;
    };
    const hide = async () => {
      cancelHide();
      stagedRef.current = [];
      setStaged([]);
      setHover(null);
      setStatus({ kind: "idle" });
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      await getCurrentWindow().hide();
    };
    const scheduleHide = (ms: number) => {
      cancelHide();
      hideTimer = window.setTimeout(() => void hide(), ms);
    };
    const reload = async () => {
      try {
        const settings = await ipc.app.settingsGet();
        applyAppearance(settings);
        notifications = settings.app.notifications;
        showDevices(await ipc.devices.list());
        // Ask the network again: devices that just woke up show up live.
        void ipc.devices.refresh().catch(() => undefined);
      } catch (err) {
        console.warn("tray drop: could not load devices", err);
      }
    };
    const send = async (fingerprint: string, paths: string[]) => {
      const device = devicesNow.find((d) => d.fingerprint === fingerprint);
      if (!device || paths.length === 0) return;
      cancelHide();
      stagedRef.current = [];
      setStaged([]);
      setStatus({ kind: "sending", name: device.displayName });
      try {
        sent.add(await ipc.transfer.send(device.fingerprint, paths));
        scheduleHide(900);
      } catch (err) {
        setStatus({ kind: "error", message: err instanceof Error ? err.message : String(err) });
        scheduleHide(2500);
      }
    };
    sendRef.current = send;

    const onRuntimeEvent = (event: RuntimeEvent) => {
      switch (event.type) {
        case "device-found":
        case "device-updated":
          showDevices([...devicesNow.filter((d) => d.fingerprint !== event.device.fingerprint), event.device]);
          break;
        case "device-lost":
          showDevices(devicesNow.filter((d) => d.fingerprint !== event.fingerprint));
          break;
        case "transfer-completed":
          if (sent.delete(event.transfer.id) && notifications) notifyResult(event.transfer);
          break;
      }
    };

    (async () => {
      const { getCurrentWebview } = await import("@tauri-apps/api/webview");
      const webview = getCurrentWebview();
      const subscriptions = await Promise.all([
        webview.listen("event:tray-drop-enter", () => {
          cancelHide();
          setStatus({ kind: "idle" });
          void reload();
        }),
        webview.listen("event:tray-drop-exit", () => {
          if (stagedRef.current.length === 0) scheduleHide(700);
        }),
        webview.listen<string[]>("event:tray-drop-staged", (event) => {
          stagedRef.current = event.payload;
          setStaged(event.payload);
          scheduleHide(STAGED_TIMEOUT_MS);
        }),
        webview.listen("event:tray-drop-dismiss", () => {
          if (stagedRef.current.length > 0) void hide();
        }),
        webview.onDragDropEvent((event) => {
          const payload = event.payload;
          if (payload.type === "enter" || payload.type === "over") {
            cancelHide();
            setHover(deviceAt(payload.position.x, payload.position.y));
          } else if (payload.type === "leave") {
            setHover(null);
            scheduleHide(stagedRef.current.length > 0 ? STAGED_TIMEOUT_MS : 350);
          } else if (payload.type === "drop") {
            setHover(null);
            const target = deviceAt(payload.position.x, payload.position.y);
            if (target) void send(target, payload.paths);
            else if (stagedRef.current.length === 0) scheduleHide(350);
          }
        }),
        ...(["device-found", "device-updated", "device-lost", "transfer-completed"] as const).map((name) =>
          webview.listen<RuntimeEvent>(`event:${name}`, (event) => onRuntimeEvent(event.payload)),
        ),
      ]);
      if (disposed) subscriptions.forEach((off) => off());
      else unlisten.push(...subscriptions);
      void reload();
    })();

    return () => {
      disposed = true;
      cancelHide();
      unlisten.forEach((off) => off());
    };
  }, []);

  const clickRow = (fingerprint: string) => {
    if (stagedRef.current.length > 0) void sendRef.current(fingerprint, stagedRef.current);
  };

  return (
    <div className="tray-drop">
      <header className="tray-drop-head">
        <strong>{t("trayDrop.title")}</strong>
        <span>{staged.length > 0 ? t("trayDrop.staged", { n: staged.length }) : t("trayDrop.hint")}</span>
      </header>
      {status.kind === "sending" ? (
        <div className="tray-drop-status">{t("trayDrop.sending", { name: status.name })}</div>
      ) : status.kind === "error" ? (
        <div className="tray-drop-status error">{t("trayDrop.failed", { message: status.message })}</div>
      ) : devices.length === 0 ? (
        <div className="tray-drop-status">{t("trayDrop.empty")}</div>
      ) : (
        <ul className="tray-drop-list">
          {devices.map((device) => (
            <li
              key={device.fingerprint}
              data-device={device.fingerprint}
              className={`${hover === device.fingerprint ? "hover" : ""} ${staged.length > 0 ? "clickable" : ""}`}
              onClick={() => clickRow(device.fingerprint)}
            >
              <DeviceIcon type={device.deviceType} model={device.deviceModel} />
              <div className="tray-drop-name">
                <strong>{device.displayName}</strong>
                <span>{device.deviceModel ?? device.host ?? ""}</span>
              </div>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
