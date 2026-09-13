// Application state and the wiring from runtime events to it. One store,
// subscribed through useSyncExternalStore; actions are plain functions.

import { useSyncExternalStore } from "react";
import { IpcError, ipc, isTauri, listen } from "./ipc";
import { getLocale, hasKey, setLanguage, t, type LanguageSetting, type Locale } from "./i18n";
import type {
  ClipboardView,
  DeviceView,
  ErrorCode,
  IdentityView,
  IncomingRequestView,
  PlatformInfo,
  RuntimeEvent,
  RuntimeStateView,
  Settings,
  TransferRecord,
  TransferView,
  MediaInfo,
  PickKind,
} from "./types";

export interface Toast {
  id: number;
  kind: "info" | "success" | "error";
  text: string;
}

export interface PairFlow {
  kind: "outgoing" | "incoming";
  fingerprint: string;
  alias: string;
  code: string;
  /** waiting: the other side has not answered; confirm: the user must compare codes; done: result shown. */
  stage: "waiting" | "confirm" | "done";
  paired?: boolean;
  message?: string | null;
}

export interface PinRequest {
  transferId: string;
  message: string;
}

export interface Conflict {
  sessionId: string;
  fileId: string;
  existing: string;
  renamed: string;
}

export interface SendDraft {
  device: DeviceView | null;
  /** A typed address when no device is chosen. */
  address: string;
  paths: string[];
}

export interface State {
  platform: PlatformInfo;
  runtime: RuntimeStateView;
  identity: IdentityView | null;
  settings: Settings | null;
  devices: DeviceView[];
  transfers: TransferView[];
  incoming: IncomingRequestView[];
  pinRequests: PinRequest[];
  conflicts: Conflict[];
  pair: PairFlow | null;
  clipboardSync: { active: boolean; peers: string[]; message: string | null };
  clipboardItems: ClipboardView[];
  history: TransferRecord[];
  /** path -> thumbnail / metadata, filled lazily by `loadMedia`. */
  media: Record<string, MediaInfo>;
  /** transferId -> bytes/s samples, newest last. */
  speeds: Record<string, number[]>;
  toasts: Toast[];
  locale: Locale;
  sendDraft: SendDraft | null;
  dragging: boolean;
  page: Page;
}

export type Page = "devices" | "transfers" | "clipboard" | "history" | "settings";

const initial: State = {
  platform: { os: "unknown", mobile: false, version: "" },
  runtime: { running: false, message: null },
  identity: null,
  settings: null,
  devices: [],
  transfers: [],
  incoming: [],
  pinRequests: [],
  conflicts: [],
  pair: null,
  clipboardSync: { active: false, peers: [], message: null },
  clipboardItems: [],
  history: [],
  media: {},
  speeds: {},
  toasts: [],
  locale: getLocale(),
  sendDraft: null,
  dragging: false,
  page: "devices",
};

let state: State = initial;
const subscribers = new Set<() => void>();

function set(patch: Partial<State> | ((previous: State) => Partial<State>)) {
  const next = typeof patch === "function" ? patch(state) : patch;
  state = { ...state, ...next };
  subscribers.forEach((notify) => notify());
}

function subscribe(notify: () => void) {
  subscribers.add(notify);
  return () => subscribers.delete(notify);
}

export function getState(): State {
  return state;
}

/** Subscribes to the whole store (small app, cheap re-renders) and applies
 *  `selector`; selectors may return fresh arrays safely. */
export function useStore<T>(selector: (state: State) => T): T {
  const snapshot = useSyncExternalStore(subscribe, getState, getState);
  return selector(snapshot);
}

// ----- helpers ----------------------------------------------------------------

let toastId = 0;
export function toast(kind: Toast["kind"], text: string, ttl = 4500) {
  const id = ++toastId;
  set((s) => ({ toasts: [...s.toasts, { id, kind, text }] }));
  window.setTimeout(() => dismissToast(id), ttl);
}

export function dismissToast(id: number) {
  set((s) => ({ toasts: s.toasts.filter((toast) => toast.id !== id) }));
}

function sortDevices(devices: DeviceView[]): DeviceView[] {
  return [...devices].sort(
    (a, b) =>
      Number(b.online) - Number(a.online) ||
      Number(b.favorite) - Number(a.favorite) ||
      Number(b.paired) - Number(a.paired) ||
      a.displayName.localeCompare(b.displayName),
  );
}

function upsertDevice(devices: DeviceView[], device: DeviceView): DeviceView[] {
  const others = devices.filter((d) => d.fingerprint !== device.fingerprint);
  return sortDevices([...others, device]);
}

function upsertTransfer(transfers: TransferView[], transfer: TransferView): TransferView[] {
  const others = transfers.filter((t) => t.id !== transfer.id);
  return [transfer, ...others].sort((a, b) => b.startedAt - a.startedAt);
}

const progressMarks: Record<string, { at: number; done: number }> = {};

function sampleSpeed(transferId: string, totalDone: number) {
  const now = performance.now();
  const last = progressMarks[transferId];
  progressMarks[transferId] = { at: now, done: totalDone };
  if (!last || now - last.at < 250) return;
  const perSecond = ((totalDone - last.done) * 1000) / (now - last.at);
  set((s) => {
    const samples = [...(s.speeds[transferId] ?? []), Math.max(0, perSecond)].slice(-40);
    return { speeds: { ...s.speeds, [transferId]: samples } };
  });
}

function applyLanguage(settings: Settings | null) {
  const language = settings?.app.language;
  setLanguage(language === "system" || language == null ? "system" : (language as Locale));
  set({ locale: getLocale() });
}

/** Translated text for an error code, or the raw message when unknown. */
export function errorText(code: ErrorCode | null | undefined, message: string): string {
  const key = code ? `error.${code}` : "";
  return key && hasKey(key) ? t(key) : message;
}

/** What to show for a failed command. */
export function describeError(err: unknown): string {
  if (err instanceof IpcError) return errorText(err.code, err.message);
  return String(err instanceof Error ? err.message : err);
}

function applyTheme(settings: Settings | null) {
  const theme = settings?.app.theme ?? "dark";
  const dark = theme === "system" ? window.matchMedia("(prefers-color-scheme: dark)").matches : theme === "dark";
  document.documentElement.dataset.theme = dark ? "dark" : "light";
}

function describeClipboard(item: ClipboardView): string {
  if (item.kind === "text") return item.text ? item.text.slice(0, 60) : t("clipboard.text");
  if (item.kind === "image") return `${t("clipboard.image")} ${item.imageWidth ?? "?"}×${item.imageHeight ?? "?"}`;
  return `${item.filePaths.length} ${t("clipboard.files")}`;
}

// ----- event wiring --------------------------------------------------------------

function handleEvent(event: RuntimeEvent) {
  switch (event.type) {
    case "device-found":
    case "device-updated":
      set((s) => ({ devices: upsertDevice(s.devices, event.device) }));
      break;
    case "device-lost":
      set((s) => ({
        devices: sortDevices(
          s.devices.map((d) => (d.fingerprint === event.fingerprint ? { ...d, online: false } : d)),
        ),
      }));
      break;
    case "incoming-request":
      if (!event.request.autoAccepted) {
        set((s) => ({ incoming: [...s.incoming.filter((r) => r.sessionId !== event.request.sessionId), event.request] }));
      }
      break;
    case "incoming-withdrawn":
      set((s) => ({
        incoming: s.incoming.filter((r) => r.sessionId !== event.sessionId),
        transfers: s.transfers.filter((t) => t.id !== event.sessionId),
      }));
      break;
    case "incoming-conflict":
      set((s) => ({ conflicts: [...s.conflicts, { sessionId: event.sessionId, fileId: event.fileId, existing: event.existing, renamed: event.renamed }] }));
      break;
    case "transfer-progress":
      sampleSpeed(event.transferId, event.totalDone);
      set((s) => ({
        transfers: s.transfers.map((transfer) => {
          if (transfer.id !== event.transferId) return transfer;
          return {
            ...transfer,
            doneSize: event.totalDone,
            totalSize: event.totalSize,
            state: transfer.state === "preparing" || transfer.state === "waiting-accept" ? "active" : transfer.state,
            files: transfer.files.map((file) =>
              file.id === event.fileId ? { ...file, done: event.done, state: event.done >= event.size ? file.state : "active" } : file,
            ),
          };
        }),
      }));
      break;
    case "transfer-file-done":
      set((s) => ({
        transfers: s.transfers.map((transfer) =>
          transfer.id === event.transferId
            ? { ...transfer, files: transfer.files.map((file) => (file.id === event.file.id ? event.file : file)) }
            : transfer,
        ),
      }));
      break;
    case "transfer-updated":
      set((s) => ({ transfers: upsertTransfer(s.transfers, event.transfer) }));
      break;
    case "transfer-completed": {
      const transfer = event.transfer;
      set((s) => ({
        transfers: upsertTransfer(s.transfers, transfer),
        incoming: s.incoming.filter((r) => r.sessionId !== transfer.id),
        pinRequests: s.pinRequests.filter((p) => p.transferId !== transfer.id),
      }));
      delete progressMarks[transfer.id];
      if (transfer.state === "finished") toast("success", t("toast.transferFinished", { alias: transfer.peerAlias }));
      else if (transfer.state === "failed") toast("error", `${t("toast.transferFailed", { alias: transfer.peerAlias })}${transfer.error ? ` · ${errorText(transfer.errorCode, transfer.error)}` : ""}`);
      void loadHistory();
      break;
    }
    case "transfer-needs-pin":
      set((s) => ({ pinRequests: [...s.pinRequests.filter((p) => p.transferId !== event.transferId), { transferId: event.transferId, message: event.message }] }));
      break;
    case "pair-request":
      set({ pair: { kind: "incoming", fingerprint: event.fingerprint, alias: event.alias, code: event.code, stage: "confirm" } });
      break;
    case "pair-response":
      set((s) => ({
        pair: s.pair && s.pair.kind === "outgoing" && s.pair.fingerprint === event.fingerprint
          ? { ...s.pair, alias: event.alias, code: event.code, stage: "confirm" }
          : { kind: "outgoing", fingerprint: event.fingerprint, alias: event.alias, code: event.code, stage: "confirm" },
      }));
      break;
    case "pair-result":
      set((s) => ({
        pair: s.pair && s.pair.fingerprint === event.fingerprint ? { ...s.pair, stage: "done", paired: event.paired, message: event.message ? errorText(event.code, event.message) : event.message } : s.pair,
      }));
      if (event.paired) toast("success", t("pair.success", { alias: event.alias }));
      else if (event.message) toast("info", `${t("pair.failed")}: ${errorText(event.code, event.message)}`);
      void refreshDevices();
      void loadClipboardSync();
      break;
    case "clipboard-received":
      if (event.item.stored) set((s) => ({ clipboardItems: [event.item, ...s.clipboardItems.filter((i) => i.id !== event.item.id)].slice(0, 200) }));
      toast("info", t("toast.clipboardReceived", { alias: event.item.originAlias ?? event.item.origin.slice(0, 8), what: describeClipboard(event.item) }));
      break;
    case "clipboard-local":
      if (event.item.stored) set((s) => ({ clipboardItems: [event.item, ...s.clipboardItems.filter((i) => i.id !== event.item.id)].slice(0, 200) }));
      break;
    case "media-ready":
      set((s) => ({ media: { ...s.media, [event.path]: event.media } }));
      break;
    case "clipboard-sync":
      set({ clipboardSync: { active: event.active, peers: event.peers, message: event.message } });
      if (event.message) toast("info", event.message);
      break;
    case "error":
      toast("error", errorText(event.code, event.message));
      break;
  }
}

const eventNames: RuntimeEvent["type"][] = [
  "device-found", "device-updated", "device-lost",
  "incoming-request", "incoming-withdrawn", "incoming-conflict",
  "transfer-progress", "transfer-file-done", "transfer-updated", "transfer-completed", "transfer-needs-pin",
  "pair-request", "pair-response", "pair-result",
  "clipboard-received", "clipboard-local", "clipboard-sync",
  "error",
];

let wired = false;

/** Loads everything and subscribes to events. Safe to call once. */
export async function bootstrap() {
  if (wired) return;
  wired = true;
  for (const name of eventNames) {
    void listen(name, (payload) => handleEvent(payload as RuntimeEvent));
  }
  void listen("runtime-state", (payload) => {
    const runtime = payload as RuntimeStateView;
    set({ runtime });
    if (runtime.running) void loadAll();
    else toast("error", runtime.message ?? t("app.runtimeStopped"));
  });
  if (isTauri) wireDragDrop();
  try {
    set({ platform: await ipc.app.platform() });
  } catch {
    // mock or unavailable
  }
  try {
    const runtime = await ipc.app.runtimeState();
    set({ runtime });
    if (runtime.running) await loadAll();
  } catch (err) {
    set({ runtime: { running: false, message: String(err) } });
  }
}

async function wireDragDrop() {
  try {
    const { getCurrentWebview } = await import("@tauri-apps/api/webview");
    await getCurrentWebview().onDragDropEvent((event) => {
      const payload = event.payload;
      if (payload.type === "enter" || payload.type === "over") {
        if (!state.dragging) set({ dragging: true });
      } else if (payload.type === "leave") {
        set({ dragging: false });
      } else if (payload.type === "drop") {
        set({ dragging: false });
        const ratio = window.devicePixelRatio || 1;
        const target = document
          .elementFromPoint(payload.position.x / ratio, payload.position.y / ratio)
          ?.closest<HTMLElement>("[data-device]");
        const fingerprint = target?.dataset.device;
        const device = fingerprint ? state.devices.find((d) => d.fingerprint === fingerprint) : undefined;
        if (device && payload.paths.length > 0) {
          void sendFiles(device.fingerprint, payload.paths);
        } else if (payload.paths.length > 0) {
          set({ sendDraft: { device: state.sendDraft?.device ?? null, address: "", paths: payload.paths } });
        }
      }
    });
  } catch (err) {
    console.warn("drag and drop unavailable", err);
  }
}

async function loadAll() {
  const [identity, settings, devices, transfers] = await Promise.all([
    ipc.app.identity(),
    ipc.app.settingsGet(),
    ipc.devices.list(),
    ipc.transfer.list(),
  ]);
  applyTheme(settings);
  applyLanguage(settings);
  set({ identity, settings, devices: sortDevices(devices), transfers });
  void loadHistory();
  void loadClipboard();
  void loadClipboardSync();
}

// ----- actions -----------------------------------------------------------------

export function navigate(page: Page) {
  set({ page });
}

export async function refreshDevices() {
  try {
    await ipc.devices.refresh();
    set({ devices: sortDevices(await ipc.devices.list()) });
  } catch (err) {
    toast("error", describeError(err));
  }
}

export async function retryRuntime() {
  try {
    await ipc.app.restart();
  } catch (err) {
    toast("error", describeError(err));
  }
}

export function openSend(device: DeviceView | null, paths: string[] = []) {
  set({ sendDraft: { device, address: "", paths } });
}

export function updateSendDraft(patch: Partial<SendDraft>) {
  set((s) => ({ sendDraft: s.sendDraft ? { ...s.sendDraft, ...patch } : null }));
}

export function closeSend() {
  set({ sendDraft: null });
}

export async function pickFiles(kind: PickKind = "files"): Promise<string[]> {
  try {
    return await ipc.app.pickFiles(kind);
  } catch (err) {
    toast("error", describeError(err));
    return [];
  }
}

export async function sendFiles(device: string, paths: string[], pin?: string) {
  try {
    await ipc.transfer.send(device, paths, pin);
    set({ sendDraft: null, page: "transfers" });
  } catch (err) {
    toast("error", describeError(err));
  }
}

export async function respondIncoming(sessionId: string, accept: boolean) {
  set((s) => ({ incoming: s.incoming.filter((r) => r.sessionId !== sessionId) }));
  try {
    await ipc.transfer.respondIncoming(sessionId, accept);
    if (accept) set({ page: "transfers" });
  } catch (err) {
    toast("error", describeError(err));
  }
}

export async function providePin(transferId: string, pin: string | null) {
  set((s) => ({ pinRequests: s.pinRequests.filter((p) => p.transferId !== transferId) }));
  try {
    await ipc.transfer.providePin(transferId, pin);
  } catch (err) {
    toast("error", describeError(err));
  }
}

export async function respondConflict(conflict: Conflict, overwrite: boolean) {
  set((s) => ({ conflicts: s.conflicts.filter((c) => c !== conflict) }));
  try {
    await ipc.transfer.respondConflict(conflict.sessionId, conflict.fileId, overwrite);
  } catch (err) {
    toast("error", describeError(err));
  }
}

export async function cancelTransfer(transferId: string) {
  try {
    await ipc.transfer.cancel(transferId);
  } catch (err) {
    toast("error", describeError(err));
  }
}

export async function dismissTransfer(transferId: string) {
  set((s) => ({ transfers: s.transfers.filter((t) => t.id !== transferId) }));
  try {
    await ipc.transfer.dismiss(transferId);
  } catch {
    // already gone
  }
}

export async function startPair(device: DeviceView | string) {
  const query = typeof device === "string" ? device : device.fingerprint;
  const alias = typeof device === "string" ? device : device.displayName;
  set({ pair: { kind: "outgoing", fingerprint: query, alias, code: "", stage: "waiting" } });
  try {
    const view = await ipc.pair.start(query);
    set((s) => ({
      pair: s.pair ? { ...s.pair, fingerprint: view.fingerprint, alias: view.alias, code: view.code } : null,
    }));
  } catch (err) {
    set({ pair: null });
    toast("error", describeError(err));
  }
}

export async function confirmPair(matches: boolean) {
  const flow = state.pair;
  if (!flow) return;
  try {
    if (flow.kind === "outgoing") await ipc.pair.confirm(flow.fingerprint, matches);
    else await ipc.pair.respond(flow.fingerprint, matches);
    if (!matches) set({ pair: null });
  } catch (err) {
    set({ pair: null });
    toast("error", describeError(err));
  }
}

export function closePair() {
  set({ pair: null });
}

export async function unpair(device: DeviceView) {
  try {
    await ipc.pair.unpair(device.fingerprint);
    toast("info", t("pair.unpaired", { alias: device.displayName }));
    await refreshDevices();
  } catch (err) {
    toast("error", describeError(err));
  }
}

export async function setFavorite(device: DeviceView, favorite: boolean) {
  set((s) => ({ devices: upsertDevice(s.devices, { ...device, favorite }) }));
  try {
    await ipc.devices.setFavorite(device.fingerprint, favorite);
  } catch (err) {
    toast("error", describeError(err));
  }
}

export async function renameDevice(device: DeviceView, alias: string | null) {
  try {
    await ipc.devices.setAlias(device.fingerprint, alias);
    set({ devices: sortDevices(await ipc.devices.list()) });
  } catch (err) {
    toast("error", describeError(err));
  }
}

export async function forgetDevice(device: DeviceView) {
  set((s) => ({ devices: s.devices.filter((d) => d.fingerprint !== device.fingerprint) }));
  try {
    await ipc.devices.forget(device.fingerprint);
  } catch (err) {
    toast("error", describeError(err));
  }
}

export async function saveSettings(settings: Settings) {
  try {
    const restarted = await ipc.app.settingsUpdate(settings);
    applyTheme(settings);
    applyLanguage(settings);
    set({ settings });
    toast("success", restarted ? t("settings.restarted") : t("settings.saved"));
    if (restarted) {
      set({ identity: await ipc.app.identity() });
    }
  } catch (err) {
    toast("error", describeError(err));
  }
}

export async function loadHistory() {
  try {
    set({ history: await ipc.history.list(200) });
  } catch {
    // runtime not running
  }
}

const mediaInFlight = new Set<string>();

/** Fetches thumbnail / metadata for `path` once; results land in `state.media`. */
export function loadMedia(path: string) {
  if (!path || state.media[path] || mediaInFlight.has(path)) return;
  mediaInFlight.add(path);
  ipc.media
    .info(path)
    .then((media) => set((s) => ({ media: { ...s.media, [path]: media } })))
    .catch(() => {})
    .finally(() => mediaInFlight.delete(path));
}

export async function mediaCacheSize(): Promise<number> {
  try {
    return await ipc.media.cacheSize();
  } catch {
    return 0;
  }
}

export async function mediaCacheClear(): Promise<number> {
  try {
    const freed = await ipc.media.cacheClear();
    set({ media: {} });
    return freed;
  } catch (err) {
    toast("error", describeError(err));
    return 0;
  }
}

export async function deleteHistory(id: string) {
  set((s) => ({ history: s.history.filter((r) => r.id !== id) }));
  try {
    await ipc.history.delete(id);
  } catch (err) {
    toast("error", describeError(err));
  }
}

export async function clearHistory() {
  set({ history: [] });
  try {
    await ipc.history.clear();
  } catch (err) {
    toast("error", describeError(err));
  }
}

export async function loadClipboard() {
  try {
    set({ clipboardItems: await ipc.clipboard.history(100) });
  } catch {
    // unsupported
  }
}

export async function loadClipboardSync() {
  try {
    const active = await ipc.clipboard.syncGet();
    const peers = state.devices.filter((d) => d.paired).map((d) => d.fingerprint);
    set((s) => ({ clipboardSync: { ...s.clipboardSync, active, peers } }));
  } catch {
    // unsupported
  }
}

export async function setClipboardSync(enabled: boolean) {
  try {
    await ipc.clipboard.syncSet(enabled);
    set((s) => ({
      clipboardSync: { ...s.clipboardSync, active: enabled },
      settings: s.settings ? { ...s.settings, clipboard: { ...s.settings.clipboard, syncEnabled: enabled } } : s.settings,
    }));
  } catch (err) {
    toast("error", describeError(err));
  }
}

export async function clipboardPush(device?: string) {
  try {
    const item = await ipc.clipboard.push(device);
    toast("success", t("toast.clipboardLocal", { what: describeClipboard(item) }));
  } catch (err) {
    toast("error", describeError(err));
  }
}

export async function clipboardCopy(id: string) {
  try {
    await ipc.clipboard.copy(id);
    toast("success", t("app.copied"));
  } catch (err) {
    toast("error", describeError(err));
  }
}

export async function clipboardDelete(id: string) {
  set((s) => ({ clipboardItems: s.clipboardItems.filter((i) => i.id !== id) }));
  try {
    await ipc.clipboard.delete(id);
  } catch (err) {
    toast("error", describeError(err));
  }
}

export async function clipboardClear() {
  set({ clipboardItems: [] });
  try {
    await ipc.clipboard.clear();
  } catch (err) {
    toast("error", describeError(err));
  }
}

/** Switches the language now and stores the choice in the settings. */
export async function switchLanguage(language: LanguageSetting) {
  setLanguage(language);
  set({ locale: getLocale() });
  const settings = state.settings;
  if (!settings || settings.app.language === language) return;
  const next: Settings = { ...settings, app: { ...settings.app, language } };
  try {
    await ipc.app.settingsUpdate(next);
    set({ settings: next });
  } catch (err) {
    toast("error", describeError(err));
  }
}

export async function openPath(path: string) {
  try {
    await ipc.app.openPath(path);
  } catch (err) {
    toast("error", describeError(err));
  }
}

export async function revealPath(path: string) {
  try {
    await ipc.app.revealPath(path);
  } catch (err) {
    toast("error", describeError(err));
  }
}

export async function pickFolder(): Promise<string | null> {
  try {
    return await ipc.app.pickFolder();
  } catch (err) {
    toast("error", describeError(err));
    return null;
  }
}
