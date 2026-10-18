// A file's thumbnail (from the media cache) or a type icon while there is
// none. Loads lazily through the store so rows stay cheap.

import { useEffect, useState } from "react";
import { isTauri } from "../ipc";
import { loadMedia, useStore } from "../store";
import { Icon, type IconName } from "./icons";

/** Resolves a cached thumbnail file to something an `<img>` can show. */
function useThumbSrc(thumbnail: string | null | undefined): string | null {
  const [src, setSrc] = useState<string | null>(null);
  useEffect(() => {
    let cancelled = false;
    if (!thumbnail) {
      setSrc(null);
    } else if (!isTauri) {
      setSrc(thumbnail);
    } else {
      import("@tauri-apps/api/core").then(({ convertFileSrc }) => {
        if (!cancelled) setSrc(convertFileSrc(thumbnail));
      });
    }
    return () => {
      cancelled = true;
    };
  }, [thumbnail]);
  return src;
}

export type FileKind = "image" | "video" | "audio" | "doc";

/** Upper-case extension of a short suffix (`HEIC`), or "" when there is none. */
export function extension(name: string): string {
  const dot = name.lastIndexOf(".");
  return dot > 0 && name.length - dot <= 5 ? name.slice(dot + 1).toUpperCase() : "";
}

/** By MIME type, falling back to the extension: peers often send
 *  `application/octet-stream` for photos and videos. */
export function fileKind(mime: string, name: string): FileKind {
  const ext = extension(name).toLowerCase();
  if (mime.startsWith("image/") || ["heic", "heif", "jpg", "jpeg", "png", "gif", "webp", "tif", "tiff", "dng"].includes(ext)) return "image";
  if (mime.startsWith("video/") || ["mov", "mp4", "m4v", "avi", "mkv", "webm"].includes(ext)) return "video";
  if (mime.startsWith("audio/") || ["mp3", "m4a", "aac", "wav", "flac", "ogg", "aiff"].includes(ext)) return "audio";
  return "doc";
}

/** Stands in for a file that has no thumbnail (yet): a glyph for media, the
 *  extension for documents. Fills its container. */
export function KindMark({ mime, name, iconSize = 24 }: { mime: string; name: string; iconSize?: number }) {
  const kind = fileKind(mime, name);
  const glyph: Record<FileKind, IconName> = { image: "image", video: "play", audio: "audio", doc: "file" };
  return <span className={`kind ${kind}`}>{kind === "doc" && extension(name) ? extension(name) : <Icon name={glyph[kind]} size={iconSize} />}</span>;
}

export function iconForMime(mime: string, fallback: IconName = "file"): IconName {
  if (mime.startsWith("image/")) return "image";
  if (mime.startsWith("audio/")) return "audio";
  return fallback;
}

export function FileThumb({
  path,
  mime,
  fallback = "file",
  size = "md",
}: {
  path: string | null | undefined;
  mime: string;
  fallback?: IconName;
  /** `lg` fills its container (iOS file tiles). */
  size?: "sm" | "md" | "lg";
}) {
  const media = useStore((s) => (path ? s.media[path] : undefined));
  const wantsPreview = !!path && (mime.startsWith("image/") || mime.startsWith("audio/"));
  useEffect(() => {
    if (wantsPreview && path) loadMedia(path);
  }, [wantsPreview, path]);
  const src = useThumbSrc(media?.thumbnail);
  const iconSize = size === "sm" ? 14 : size === "lg" ? 28 : 18;
  return (
    <div className={`thumb ${size === "sm" ? "thumb-sm" : size === "lg" ? "thumb-lg" : ""}`}>
      {src ? <img src={src} alt="" loading="lazy" /> : <Icon name={iconForMime(mime, fallback)} size={iconSize} />}
    </div>
  );
}

/** `4032×3024`, `3:45 · Artist` … for the secondary line of a row. */
export function useMediaCaption(path: string | null | undefined): string | null {
  const media = useStore((s) => (path ? s.media[path] : undefined));
  if (!media) return null;
  if (media.kind === "image" && media.width && media.height) return `${media.width}×${media.height}`;
  if (media.kind === "audio") {
    const parts: string[] = [];
    if (media.durationMs != null) {
      const total = Math.round(media.durationMs / 1000);
      parts.push(`${Math.floor(total / 60)}:${String(total % 60).padStart(2, "0")}`);
    }
    const who = [media.title, media.artist].filter(Boolean).join(" · ");
    if (who) parts.push(who);
    return parts.length ? parts.join(" · ") : null;
  }
  return null;
}
