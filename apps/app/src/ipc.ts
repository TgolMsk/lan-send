// Typed access to the Tauri commands (`cmd_<module>_<action>`) and events
// (`event:<name>`). In a plain browser (UI development) the mock in
// ./mock.ts answers instead.

import type {
  ClipboardView,
  DeviceView,
  EventName,
  IdentityView,
  PairView,
  PlatformInfo,
  RuntimeEvent,
  RuntimeStateView,
  Settings,
  TransferRecord,
  TransferView,
} from "./types";

export const isTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

type Listener = (payload: unknown) => void;

async function tauriInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(command, args);
}

async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (isTauri) {
    try {
      return await tauriInvoke<T>(command, args);
    } catch (err) {
      const message =
        typeof err === "object" && err !== null && "message" in err
          ? String((err as { message: unknown }).message)
          : String(err);
      throw new Error(message);
    }
  }
  const { mockInvoke } = await import("./mock");
  return mockInvoke<T>(command, args);
}

/** Subscribes to `event:<name>`; returns the unsubscribe function. */
export async function listen(name: EventName, listener: Listener): Promise<() => void> {
  if (isTauri) {
    const { listen: tauriListen } = await import("@tauri-apps/api/event");
    return tauriListen(`event:${name}`, (event) => listener(event.payload));
  }
  const { mockListen } = await import("./mock");
  return mockListen(name, listener);
}

export type { RuntimeEvent };

export const ipc = {
  app: {
    platform: () => invoke<PlatformInfo>("cmd_app_platform"),
    runtimeState: () => invoke<RuntimeStateView>("cmd_app_runtime_state"),
    restart: () => invoke<void>("cmd_app_restart"),
    identity: () => invoke<IdentityView>("cmd_app_identity"),
    settingsGet: () => invoke<Settings>("cmd_app_settings_get"),
    settingsUpdate: (settings: Settings) =>
      invoke<boolean>("cmd_app_settings_update", { settings }),
    pickFiles: (folders = false) => invoke<string[]>("cmd_app_pick_files", { folders }),
    pickFolder: () => invoke<string | null>("cmd_app_pick_folder"),
    openPath: (path: string) => invoke<void>("cmd_app_open_path", { path }),
    revealPath: (path: string) => invoke<void>("cmd_app_reveal_path", { path }),
  },
  devices: {
    list: () => invoke<DeviceView[]>("cmd_devices_list"),
    refresh: () => invoke<void>("cmd_devices_refresh"),
    setFavorite: (fingerprint: string, favorite: boolean) =>
      invoke<void>("cmd_devices_set_favorite", { fingerprint, favorite }),
    setAlias: (fingerprint: string, alias: string | null) =>
      invoke<void>("cmd_devices_set_alias", { fingerprint, alias }),
    forget: (fingerprint: string) => invoke<void>("cmd_devices_forget", { fingerprint }),
  },
  transfer: {
    send: (device: string, paths: string[], pin?: string) =>
      invoke<string>("cmd_transfer_send", { request: { device, paths, pin: pin ?? null } }),
    list: () => invoke<TransferView[]>("cmd_transfer_list"),
    cancel: (transferId: string) => invoke<void>("cmd_transfer_cancel", { transferId }),
    providePin: (transferId: string, pin: string | null) =>
      invoke<void>("cmd_transfer_provide_pin", { transferId, pin }),
    dismiss: (transferId: string) => invoke<boolean>("cmd_transfer_dismiss", { transferId }),
    respondIncoming: (sessionId: string, accept: boolean) =>
      invoke<void>("cmd_transfer_respond_incoming", { sessionId, accept }),
    respondConflict: (sessionId: string, fileId: string, overwrite: boolean) =>
      invoke<void>("cmd_transfer_respond_conflict", { sessionId, fileId, overwrite }),
  },
  pair: {
    start: (device: string) => invoke<PairView>("cmd_pair_start", { device }),
    confirm: (fingerprint: string, matches: boolean) =>
      invoke<void>("cmd_pair_confirm", { fingerprint, matches }),
    respond: (fingerprint: string, accept: boolean) =>
      invoke<void>("cmd_pair_respond", { fingerprint, accept }),
    unpair: (fingerprint: string) => invoke<void>("cmd_pair_unpair", { fingerprint }),
  },
  history: {
    list: (limit = 200) => invoke<TransferRecord[]>("cmd_history_list", { limit }),
    delete: (id: string) => invoke<boolean>("cmd_history_delete", { id }),
    clear: () => invoke<number>("cmd_history_clear"),
  },
  clipboard: {
    history: (limit = 50) => invoke<ClipboardView[]>("cmd_clipboard_history", { limit }),
    copy: (id: string) => invoke<void>("cmd_clipboard_copy", { id }),
    delete: (id: string) => invoke<boolean>("cmd_clipboard_delete", { id }),
    clear: () => invoke<number>("cmd_clipboard_clear"),
    push: (device?: string) => invoke<ClipboardView>("cmd_clipboard_push", { device: device ?? null }),
    syncGet: () => invoke<boolean>("cmd_clipboard_sync_get"),
    syncSet: (enabled: boolean) => invoke<void>("cmd_clipboard_sync_set", { enabled }),
  },
};
