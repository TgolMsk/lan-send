// Milestone 5 placeholder: proves the IPC round trip (identity, devices,
// events). The real interface (milestone 6) replaces this file.

import { useEffect, useState } from "react";
import { ipc, listen } from "./ipc";
import type { DeviceView, IdentityView, RuntimeEvent } from "./types";

export default function App() {
  const [identity, setIdentity] = useState<IdentityView | null>(null);
  const [devices, setDevices] = useState<DeviceView[]>([]);
  const [log, setLog] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);

  const refresh = async () => {
    try {
      setIdentity(await ipc.app.identity());
      setDevices(await ipc.devices.list());
      setError(null);
    } catch (err) {
      setError(String(err instanceof Error ? err.message : err));
    }
  };

  useEffect(() => {
    void refresh();
    const names: RuntimeEvent["type"][] = [
      "device-found",
      "device-updated",
      "device-lost",
      "incoming-request",
      "transfer-completed",
      "error",
    ];
    const unsubscribers: Array<() => void> = [];
    for (const name of names) {
      void listen(name, (payload) => {
        setLog((lines) => [`${name}: ${JSON.stringify(payload)}`, ...lines].slice(0, 50));
        if (name.startsWith("device")) void refresh();
      }).then((unsubscribe) => unsubscribers.push(unsubscribe));
    }
    void listen("runtime-state", () => void refresh()).then((u) => unsubscribers.push(u));
    return () => unsubscribers.forEach((unsubscribe) => unsubscribe());
  }, []);

  return (
    <main className="placeholder">
      <h1>lan-send</h1>
      {error && <p className="error">{error}</p>}
      {identity && (
        <p>
          {identity.alias} · {identity.fingerprint.slice(0, 8)} · port {identity.port} ·{" "}
          {identity.deviceModel}
        </p>
      )}
      <button onClick={() => void ipc.devices.refresh().then(refresh)}>Refresh devices</button>
      <ul>
        {devices.map((device) => (
          <li key={device.fingerprint}>
            {device.online ? "●" : "○"} {device.displayName} ({device.deviceModel ?? "?"}) {device.host}:
            {device.port} {device.paired ? "paired" : ""}
          </li>
        ))}
      </ul>
      <pre>{log.join("\n")}</pre>
    </main>
  );
}
