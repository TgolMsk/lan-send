// Mirrors of the Rust views in crates/core/src/runtime/events.rs and the
// settings in crates/core/src/store/settings.rs (camelCase on the wire).

export interface DeviceView {
  fingerprint: string;
  alias: string;
  customAlias: string | null;
  displayName: string;
  deviceType: string | null;
  deviceModel: string | null;
  version: string | null;
  host: string | null;
  port: number | null;
  online: boolean;
  favorite: boolean;
  paired: boolean;
  features: string[];
  lastSeen: number;
}

export interface IdentityView {
  alias: string;
  fingerprint: string;
  port: number;
  deviceType: string;
  deviceModel: string;
  configDir: string;
  receiveDir: string | null;
  clipboardSupported: boolean;
  features: string[];
  multicastError: string | null;
}

export interface FileView {
  id: string;
  name: string;
  size: number;
  mime: string;
}

export interface IncomingRequestView {
  sessionId: string;
  peerFingerprint: string;
  peerAlias: string;
  peerHost: string;
  files: FileView[];
  totalSize: number;
  resumable: boolean;
  clipboardIntent: boolean;
  autoAccepted: boolean;
}

export type FileState = "pending" | "active" | "finished" | "failed" | "cancelled" | "skipped";

export type TransferState =
  | "preparing"
  | "waiting-pin"
  | "waiting-accept"
  | "active"
  | "finished"
  | "failed"
  | "cancelled"
  | "declined";

export interface TransferFileView {
  id: string;
  name: string;
  size: number;
  mime: string;
  done: number;
  state: FileState;
  path: string | null;
  error: string | null;
}

export interface TransferView {
  id: string;
  direction: "send" | "receive";
  sessionId: string | null;
  peerFingerprint: string;
  peerAlias: string;
  files: TransferFileView[];
  totalSize: number;
  doneSize: number;
  state: TransferState;
  clipboardIntent: boolean;
  error: string | null;
  startedAt: number;
  finishedAt: number | null;
}

export interface PairView {
  fingerprint: string;
  alias: string;
  code: string;
}

export interface ClipboardView {
  id: string;
  origin: string;
  originAlias: string | null;
  fromSelf: boolean;
  createdAt: number;
  kind: "text" | "image" | "files";
  size: number;
  text: string | null;
  imagePath: string | null;
  imageWidth: number | null;
  imageHeight: number | null;
  filePaths: string[];
  stored: boolean;
  description: string;
}

export interface TransferRecord {
  id: string;
  sessionId: string;
  direction: "send" | "receive";
  peerFingerprint: string;
  peerAlias: string;
  fileName: string;
  path: string | null;
  size: number;
  mime: string;
  status: "finished" | "failed" | "cancelled" | "skipped";
  error: string | null;
  startedAt: number;
  finishedAt: number | null;
}

/** Thumbnail and metadata of a local file (ADR-0015); every field may be absent. */
/** What the file dialog should offer; `media` is the iOS photo library. */
export type PickKind = "files" | "folders" | "media";

export interface MediaInfo {
  kind: "image" | "audio" | "video" | "other" | null;
  mime: string;
  thumbnail: string | null;
  width: number | null;
  height: number | null;
  title: string | null;
  artist: string | null;
  album: string | null;
  durationMs: number | null;
}

export interface OrganizeRules {
  byDevice: boolean;
  byDate: boolean;
  byType: boolean;
}

export interface Settings {
  alias: string | null;
  port: number;
  requireClientCerts: boolean;
  receiveDir: string | null;
  organize: OrganizeRules;
  onConflict: "rename" | "overwrite" | "ask";
  createChecksums: boolean;
  verifyChecksums: boolean;
  parallelUploads: number;
  skipHiddenFiles: boolean;
  historyLimit: number;
  resume: boolean;
  ipv6: boolean;
  pin: string | null;
  clipboard: {
    textLimit: number;
    imageLimit: number;
    historyLimit: number;
    pollIntervalMs: number;
    neverStoreText: boolean;
    syncEnabled: boolean;
  };
  app: {
    globalShortcut: string;
    closeToTray: boolean;
    theme: "dark" | "light" | "system";
    autoAcceptPaired: boolean;
    notifications: boolean;
  };
}

export interface PlatformInfo {
  os: string;
  mobile: boolean;
  version: string;
}

export interface RuntimeStateView {
  running: boolean;
  message: string | null;
}

// Events (payload = the Rust enum variant with a `type` tag).
export type RuntimeEvent =
  | { type: "device-found"; device: DeviceView }
  | { type: "device-updated"; device: DeviceView }
  | { type: "device-lost"; fingerprint: string }
  | { type: "incoming-request"; request: IncomingRequestView }
  | { type: "incoming-withdrawn"; sessionId: string }
  | { type: "incoming-conflict"; sessionId: string; fileId: string; existing: string; renamed: string }
  | {
      type: "transfer-progress";
      transferId: string;
      fileId: string;
      done: number;
      size: number;
      totalDone: number;
      totalSize: number;
    }
  | { type: "transfer-file-done"; transferId: string; file: TransferFileView }
  | { type: "transfer-updated"; transfer: TransferView }
  | { type: "transfer-completed"; transfer: TransferView }
  | { type: "transfer-needs-pin"; transferId: string; message: string }
  | { type: "pair-request"; fingerprint: string; alias: string; host: string; code: string }
  | { type: "pair-response"; fingerprint: string; alias: string; code: string }
  | { type: "pair-result"; fingerprint: string; alias: string; paired: boolean; message: string | null }
  | { type: "clipboard-received"; item: ClipboardView }
  | { type: "clipboard-local"; item: ClipboardView }
  | { type: "clipboard-sync"; active: boolean; peers: string[]; message: string | null }
  | { type: "media-ready"; path: string; media: MediaInfo }
  | { type: "error"; scope: string; message: string };

export type EventName = RuntimeEvent["type"] | "runtime-state";
