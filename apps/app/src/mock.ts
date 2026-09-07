// In-memory stand-in for the Rust runtime so the interface can be developed
// and screenshotted in a plain browser. Never loaded inside Tauri.

import type { DeviceView, IdentityView, RuntimeEvent, TransferView, Settings } from "./types";

type Listener = (payload: unknown) => void;
const listeners = new Map<string, Set<Listener>>();

export function mockListen(name: string, listener: Listener): () => void {
  const set = listeners.get(name) ?? new Set<Listener>();
  set.add(listener);
  listeners.set(name, set);
  return () => set.delete(listener);
}

function emit(event: RuntimeEvent) {
  listeners.get(event.type)?.forEach((listener) => listener(event));
}

const identity: IdentityView = {
  alias: "MacBook Pro",
  fingerprint: "3F9A1C5E7B2D4A6C8E0F1A2B3C4D5E6F7A8B9C0D1E2F3A4B5C6D7E8F9A0B1C2D",
  port: 53317,
  deviceType: "desktop",
  deviceModel: "macOS",
  configDir: "/Users/me/Library/Application Support/lan-send",
  receiveDir: "/Users/me/Downloads",
  clipboardSupported: true,
  features: ["pairing", "resume", "clipboard"],
  multicastError: null,
};

const devices: DeviceView[] = [
  {
    fingerprint: "A1B2C3D4E5F60718293A4B5C6D7E8F90A1B2C3D4E5F60718293A4B5C6D7E8F90",
    alias: "Nice Orange",
    customAlias: null,
    displayName: "Nice Orange",
    deviceType: "mobile",
    deviceModel: "iPhone",
    version: "2.1",
    host: "192.168.1.24",
    port: 53317,
    online: true,
    favorite: true,
    paired: true,
    features: ["pairing", "resume"],
    lastSeen: Date.now() / 1000,
  },
  {
    fingerprint: "0F1E2D3C4B5A69788796A5B4C3D2E1F00F1E2D3C4B5A69788796A5B4C3D2E1F0",
    alias: "DESKTOP-7Q2W",
    customAlias: "Work PC",
    displayName: "Work PC",
    deviceType: "desktop",
    deviceModel: "Windows",
    version: "2.2",
    host: "192.168.1.40",
    port: 53317,
    online: true,
    favorite: false,
    paired: false,
    features: ["pairing", "resume", "clipboard"],
    lastSeen: Date.now() / 1000,
  },
  {
    fingerprint: "9988776655443322110099887766554433221100998877665544332211009988",
    alias: "Pixel 8",
    customAlias: null,
    displayName: "Pixel 8",
    deviceType: "mobile",
    deviceModel: "Android",
    version: "2.1",
    host: "192.168.1.51",
    port: 53317,
    online: false,
    favorite: false,
    paired: false,
    features: [],
    lastSeen: Date.now() / 1000 - 86400,
  },
];

const settings: Settings = {
  alias: null,
  port: 53317,
  requireClientCerts: true,
  receiveDir: null,
  organize: { byDevice: false, byDate: false, byType: false },
  onConflict: "rename",
  createChecksums: true,
  verifyChecksums: true,
  parallelUploads: 3,
  skipHiddenFiles: true,
  historyLimit: 200,
  resume: true,
  ipv6: true,
  pin: null,
  clipboard: {
    textLimit: 1048576,
    imageLimit: 10485760,
    historyLimit: 50,
    pollIntervalMs: 300,
    neverStoreText: false,
    syncEnabled: true,
  },
  app: {
    globalShortcut: "CmdOrCtrl+Shift+V",
    closeToTray: true,
    theme: "dark",
    autoAcceptPaired: false,
    notifications: true,
  },
};

const transfers: TransferView[] = [];

export async function mockInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  await new Promise((resolve) => setTimeout(resolve, 60));
  switch (command) {
    case "cmd_app_platform":
      return { os: "macos", mobile: false, version: "0.1.0" } as T;
    case "cmd_app_runtime_state":
      return { running: true, message: null } as T;
    case "cmd_app_identity":
      return identity as T;
    case "cmd_app_settings_get":
      return settings as T;
    case "cmd_app_settings_update":
      Object.assign(settings, args?.settings as Settings);
      return false as T;
    case "cmd_app_pick_files":
      return ["/Users/me/Pictures/IMG_2041.HEIC", "/Users/me/Documents/report.pdf"] as T;
    case "cmd_app_pick_folder":
      return "/Users/me/Downloads/lan-send" as T;
    case "cmd_devices_list":
      return devices as T;
    case "cmd_devices_refresh":
      return undefined as T;
    case "cmd_devices_set_favorite": {
      const device = devices.find((d) => d.fingerprint === args?.fingerprint);
      if (device) device.favorite = Boolean(args?.favorite);
      return undefined as T;
    }
    case "cmd_transfer_send": {
      const request = args?.request as { device: string; paths: string[] };
      const id = `t-${Date.now()}`;
      const device = devices.find((d) => d.fingerprint === request.device || d.displayName === request.device);
      const transfer: TransferView = {
        id,
        direction: "send",
        sessionId: null,
        peerFingerprint: device?.fingerprint ?? "",
        peerAlias: device?.displayName ?? request.device,
        files: request.paths.map((path, index) => ({
          id: `f${index}`,
          name: path.split("/").pop() ?? path,
          size: 4_200_000 * (index + 1),
          mime: "application/octet-stream",
          done: 0,
          state: "pending",
          path,
          error: null,
        })),
        totalSize: 0,
        doneSize: 0,
        state: "active",
        clipboardIntent: false,
        error: null,
        startedAt: Date.now() / 1000,
        finishedAt: null,
      };
      transfer.totalSize = transfer.files.reduce((sum, file) => sum + file.size, 0);
      transfers.unshift(transfer);
      emit({ type: "transfer-updated", transfer });
      let tick = 0;
      const timer = setInterval(() => {
        tick += 1;
        for (const file of transfer.files) {
          file.done = Math.min(file.size, Math.round((file.size * tick) / 20));
          file.state = file.done >= file.size ? "finished" : "active";
        }
        transfer.doneSize = transfer.files.reduce((sum, file) => sum + file.done, 0);
        emit({
          type: "transfer-progress",
          transferId: id,
          fileId: transfer.files[0]?.id ?? "",
          done: transfer.files[0]?.done ?? 0,
          size: transfer.files[0]?.size ?? 0,
          totalDone: transfer.doneSize,
          totalSize: transfer.totalSize,
        });
        if (tick >= 20) {
          clearInterval(timer);
          transfer.state = "finished";
          transfer.finishedAt = Date.now() / 1000;
          emit({ type: "transfer-completed", transfer });
        }
      }, 150);
      return id as T;
    }
    case "cmd_transfer_list":
      return transfers as T;
    case "cmd_history_list":
      return [
        {
          id: "h1",
          sessionId: "s1",
          direction: "receive",
          peerFingerprint: devices[0]?.fingerprint ?? "",
          peerAlias: "Nice Orange",
          fileName: "IMG_2040.jpg",
          path: "/Users/me/Downloads/IMG_2040.jpg",
          size: 3_400_000,
          mime: "image/jpeg",
          status: "finished",
          error: null,
          startedAt: Date.now() / 1000 - 3600,
          finishedAt: Date.now() / 1000 - 3590,
        },
      ] as T;
    case "cmd_clipboard_history":
      return [
        {
          id: "c1",
          origin: identity.fingerprint,
          originAlias: identity.alias,
          fromSelf: true,
          createdAt: Date.now() - 120000,
          kind: "text",
          size: 42,
          text: "https://github.com/TgolMsk/lan-send",
          imagePath: null,
          imageWidth: null,
          imageHeight: null,
          filePaths: [],
          stored: true,
          description: "text, 42 B",
        },
      ] as T;
    case "cmd_clipboard_sync_get":
      return true as T;
    case "cmd_pair_start":
      return mockPairStart(String(args?.device)) as T;
    case "cmd_pair_confirm":
      mockPairConfirm(String(args?.fingerprint), Boolean(args?.matches));
      return undefined as T;
    case "cmd_pair_respond": {
      const device = devices.find((d) => d.fingerprint === args?.fingerprint);
      if (device) device.paired = Boolean(args?.accept);
      window.setTimeout(
        () => emit({ type: "pair-result", fingerprint: String(args?.fingerprint), alias: device?.displayName ?? "device", paired: Boolean(args?.accept), message: null }),
        200,
      );
      return undefined as T;
    }
    case "cmd_pair_unpair": {
      const device = devices.find((d) => d.fingerprint === args?.fingerprint);
      if (device) device.paired = false;
      return undefined as T;
    }
    case "cmd_transfer_respond_incoming": {
      if (args?.accept) {
        const transfer: TransferView = {
          id: String(args.sessionId),
          direction: "receive",
          sessionId: String(args.sessionId),
          peerFingerprint: devices[0]?.fingerprint ?? "",
          peerAlias: "Nice Orange",
          files: [
            { id: "a", name: "IMG_2041.HEIC", size: 3_800_000, mime: "image/heic", done: 0, state: "pending", path: null, error: null },
            { id: "b", name: "Q3 report.pdf", size: 1_200_000, mime: "application/pdf", done: 0, state: "pending", path: null, error: null },
            { id: "c", name: "notes.txt", size: 2_400, mime: "text/plain", done: 0, state: "pending", path: null, error: null },
          ],
          totalSize: 5_002_400,
          doneSize: 0,
          state: "active",
          clipboardIntent: false,
          error: null,
          startedAt: Date.now() / 1000,
          finishedAt: null,
        };
        transfers.unshift(transfer);
        emit({ type: "transfer-updated", transfer });
        let tick = 0;
        const timer = setInterval(() => {
          tick += 1;
          for (const file of transfer.files) {
            file.done = Math.min(file.size, Math.round((file.size * tick) / 30));
            file.state = file.done >= file.size ? "finished" : "active";
          }
          transfer.doneSize = transfer.files.reduce((sum, file) => sum + file.done, 0);
          emit({ type: "transfer-progress", transferId: transfer.id, fileId: "a", done: transfer.files[0].done, size: transfer.files[0].size, totalDone: transfer.doneSize, totalSize: transfer.totalSize });
          if (tick >= 30) {
            clearInterval(timer);
            transfer.state = "finished";
            transfer.finishedAt = Date.now() / 1000;
            emit({ type: "transfer-completed", transfer });
          }
        }, 200);
      }
      return undefined as T;
    }
    case "cmd_transfer_cancel": {
      const transfer = transfers.find((t) => t.id === args?.transferId);
      if (transfer) {
        transfer.state = "cancelled";
        emit({ type: "transfer-completed", transfer });
      }
      return undefined as T;
    }
    case "cmd_transfer_dismiss":
      return true as T;
    case "cmd_transfer_provide_pin":
    case "cmd_transfer_respond_conflict":
    case "cmd_devices_set_alias":
    case "cmd_devices_forget":
    case "cmd_history_delete":
    case "cmd_history_clear":
    case "cmd_clipboard_copy":
    case "cmd_clipboard_delete":
    case "cmd_clipboard_clear":
    case "cmd_clipboard_sync_set":
    case "cmd_app_open_path":
    case "cmd_app_reveal_path":
    case "cmd_app_restart":
      return undefined as T;
    case "cmd_clipboard_push":
      return {
        id: "c-push",
        origin: identity.fingerprint,
        originAlias: identity.alias,
        fromSelf: true,
        createdAt: Date.now(),
        kind: "text",
        size: 12,
        text: "hello there",
        imagePath: null,
        imageWidth: null,
        imageHeight: null,
        filePaths: [],
        stored: true,
        description: "text, 12 B",
      } as T;
    default:
      return undefined as T;
  }
}

// ----- extra mock behaviour: pairing, incoming, demo triggers ----------------

declare global {
  interface Window {
    lanSendDemo?: {
      incoming: () => void;
      pin: () => void;
      pairRequest: () => void;
      conflict: () => void;
      clipboard: () => void;
    };
  }
}

window.lanSendDemo = {
  incoming: () =>
    emit({
      type: "incoming-request",
      request: {
        sessionId: "s-demo",
        peerFingerprint: devices[0]?.fingerprint ?? "",
        peerAlias: "Nice Orange",
        peerHost: "192.168.1.24",
        files: [
          { id: "a", name: "IMG_2041.HEIC", size: 3_800_000, mime: "image/heic" },
          { id: "b", name: "Q3 report.pdf", size: 1_200_000, mime: "application/pdf" },
          { id: "c", name: "notes.txt", size: 2_400, mime: "text/plain" },
        ],
        totalSize: 5_002_400,
        resumable: true,
        clipboardIntent: false,
        autoAccepted: false,
      },
    }),
  pin: () => emit({ type: "transfer-needs-pin", transferId: "t-demo", message: "PIN required" }),
  pairRequest: () =>
    emit({ type: "pair-request", fingerprint: devices[1]?.fingerprint ?? "", alias: "Work PC", host: "192.168.1.40", code: "482913" }),
  conflict: () =>
    emit({ type: "incoming-conflict", sessionId: "s-demo", fileId: "a", existing: "/Users/me/Downloads/IMG_2041.HEIC", renamed: "/Users/me/Downloads/IMG_2041 (1).HEIC" }),
  clipboard: () =>
    emit({
      type: "clipboard-received",
      item: {
        id: `c-${Date.now()}`,
        origin: devices[0]?.fingerprint ?? "",
        originAlias: "Nice Orange",
        fromSelf: false,
        createdAt: Date.now(),
        kind: "text",
        size: 27,
        text: "Meet at 3pm, room 4B",
        imagePath: null,
        imageWidth: null,
        imageHeight: null,
        filePaths: [],
        stored: true,
        description: "text, 27 B",
      },
    }),
};

export function mockPairStart(device: string) {
  const found = devices.find((d) => d.fingerprint === device || d.displayName === device);
  const fingerprint = found?.fingerprint ?? "FFFF0000FFFF0000FFFF0000FFFF0000FFFF0000FFFF0000FFFF0000FFFF0000";
  const alias = found?.displayName ?? device;
  window.setTimeout(() => emit({ type: "pair-response", fingerprint, alias, code: "271828" }), 1500);
  return { fingerprint, alias, code: "271828" };
}

export function mockPairConfirm(fingerprint: string, matches: boolean) {
  const device = devices.find((d) => d.fingerprint === fingerprint);
  if (device) device.paired = matches;
  window.setTimeout(
    () => emit({ type: "pair-result", fingerprint, alias: device?.displayName ?? "device", paired: matches, message: matches ? null : "the codes did not match" }),
    300,
  );
}
