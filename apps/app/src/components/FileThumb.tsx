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
  size?: "sm" | "md";
}) {
  const media = useStore((s) => (path ? s.media[path] : undefined));
  const wantsPreview = !!path && (mime.startsWith("image/") || mime.startsWith("audio/"));
  useEffect(() => {
    if (wantsPreview && path) loadMedia(path);
  }, [wantsPreview, path]);
  const src = useThumbSrc(media?.thumbnail);
  const iconSize = size === "sm" ? 14 : 18;
  return (
    <div className={`thumb ${size === "sm" ? "thumb-sm" : ""}`}>
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
