import type { ReactNode } from "react";
import { Icon, type IconName } from "./icons";
import { t } from "../i18n";
import { navigate, useStore, type Page } from "../store";
import { shortFingerprint } from "../format";
import { Avatar } from "./ui";
import type { PlatformInfo } from "../types";
import logo from "../assets/logo.png";

/** The window has no title bar on macOS, so chrome areas move the window
 *  instead. Desktop only: dragging is meaningless on iOS. */
function dragRegion(platform: PlatformInfo) {
  return platform.mobile ? {} : { "data-tauri-drag-region": "deep" };
}

const pages: { id: Page; icon: IconName; label: () => string }[] = [
  { id: "devices", icon: "devices", label: () => t("nav.devices") },
  { id: "transfers", icon: "transfers", label: () => t("nav.transfers") },
  { id: "clipboard", icon: "clipboard", label: () => t("nav.clipboard") },
  { id: "history", icon: "history", label: () => t("nav.history") },
  { id: "settings", icon: "settings", label: () => t("nav.settings") },
];

export function Layout({ children }: { children: ReactNode }) {
  const page = useStore((s) => s.page);
  const identity = useStore((s) => s.identity);
  const runtime = useStore((s) => s.runtime);
  const platform = useStore((s) => s.platform);
  const active = useStore((s) => s.transfers.filter((t) => !["finished", "failed", "cancelled", "declined"].includes(t.state)).length);
  const visible = pages.filter((p) => !(platform.mobile && p.id === "clipboard"));
  return (
    <div className="shell">
      {/* `deep` drags from anywhere in the subtree; Tauri excludes buttons and
          links on its own, so the navigation stays clickable. */}
      <aside className="sidebar" {...dragRegion(platform)}>
        {platform.os === "macos" && <div className="titlebar-space" />}
        <div className="brand">
          <img className="brand-mark" src={logo} alt="" draggable={false} />
          <span className="brand-name">Lan-Send</span>
        </div>
        {visible.map((p) => (
          <button key={p.id} className={`nav-item ${page === p.id ? "active" : ""}`} onClick={() => navigate(p.id)}>
            <Icon name={p.icon} size={18} />
            <span>{p.label()}</span>
            {p.id === "transfers" && active > 0 && <span className="badge-count">{active}</span>}
          </button>
        ))}
        <div className="sidebar-foot">
          <div className="me-card">
            <Avatar name={identity?.alias ?? "?"} />
            <div style={{ minWidth: 0 }}>
              <div className="me-name">{identity?.alias ?? t("app.thisDevice")}</div>
              <div className="me-meta">
                <span className={`dot ${runtime.running ? "on" : ""}`} /> {identity ? `${shortFingerprint(identity.fingerprint)} · ${identity.port}` : t("app.runtimeStopped")}
              </div>
            </div>
          </div>
        </div>
      </aside>
      <main className="content">
        <div className="content-inner">{children}</div>
      </main>
      <nav className="tabbar">
        {visible.map((p) => (
          <button key={p.id} className={`tab-item ${page === p.id ? "active" : ""}`} onClick={() => navigate(p.id)}>
            <Icon name={p.icon} size={20} />
            <span>{p.label()}</span>
          </button>
        ))}
      </nav>
    </div>
  );
}

export function PageHead({ title, subtitle, actions }: { title: string; subtitle?: string; actions?: ReactNode }) {
  const platform = useStore((s) => s.platform);
  return (
    <header className="page-head" {...dragRegion(platform)}>
      <div>
        <h1 className="page-title">{title}</h1>
        {subtitle && <p className="page-subtitle">{subtitle}</p>}
      </div>
      {actions && <div className="page-actions">{actions}</div>}
    </header>
  );
}
