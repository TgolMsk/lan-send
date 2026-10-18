import type { ButtonHTMLAttributes, HTMLAttributes, InputHTMLAttributes, ReactNode } from "react";
import { Icon, type IconName } from "./icons";
import { t } from "../i18n";

export function Button({
  variant = "primary",
  size = "md",
  icon,
  className = "",
  children,
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "primary" | "ghost" | "outline" | "danger";
  size?: "md" | "sm";
  icon?: IconName;
}) {
  return (
    <button className={`btn ${variant} ${size} ${className}`} type="button" {...rest}>
      {icon && <Icon name={icon} size={size === "sm" ? 15 : 17} />}
      {children}
    </button>
  );
}

export function IconButton({
  label,
  icon,
  on = false,
  className = "",
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & { label: string; icon: IconName; on?: boolean }) {
  return (
    <button className={`icon-btn ${on ? "on" : ""} ${className}`} type="button" title={label} aria-label={label} {...rest}>
      <Icon name={icon} size={17} fill={on && icon === "star" ? "currentColor" : undefined} />
    </button>
  );
}

/** A round glass toolbar button, as in the iOS 26 navigation bar. */
export function GlassButton({ label, icon, className = "", ...rest }: ButtonHTMLAttributes<HTMLButtonElement> & { label: string; icon: IconName }) {
  return (
    <button className={`glass-btn ${className}`} type="button" title={label} aria-label={label} {...rest}>
      <Icon name={icon} size={20} />
    </button>
  );
}

/** Circular progress, `value` 0–1; colours come from CSS (`.ring`). */
export function Ring({ value, size, stroke, children }: { value: number; size: number; stroke: number; children?: ReactNode }) {
  const radius = (size - stroke) / 2;
  const length = 2 * Math.PI * radius;
  const shown = Math.max(0, Math.min(1, value)) * length;
  return (
    <div className="ring" style={{ width: size, height: size }}>
      <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} aria-hidden>
        <circle className="ring-track" cx={size / 2} cy={size / 2} r={radius} strokeWidth={stroke} />
        <circle
          className="ring-value"
          cx={size / 2}
          cy={size / 2}
          r={radius}
          strokeWidth={stroke}
          strokeDasharray={`${shown} ${length}`}
        />
      </svg>
      {children && <div className="ring-inner">{children}</div>}
    </div>
  );
}

export function Card({
  solid = false,
  interactive = false,
  className = "",
  children,
  ...rest
}: HTMLAttributes<HTMLDivElement> & { solid?: boolean; interactive?: boolean }) {
  return (
    <div className={`card ${solid ? "solid" : ""} ${interactive ? "interactive" : ""} ${className}`} {...rest}>
      {children}
    </div>
  );
}

export function Badge({ tone = "muted", children }: { tone?: "success" | "muted" | "danger" | "accent"; children: ReactNode }) {
  return <span className={`badge ${tone}`}>{children}</span>;
}

export function Toggle({ checked, onChange, label }: { checked: boolean; onChange: (value: boolean) => void; label?: ReactNode }) {
  return (
    <label className={`toggle ${checked ? "on" : ""}`}>
      <input type="checkbox" checked={checked} onChange={(event) => onChange(event.target.checked)} hidden />
      <span className="track" />
      {label && <span>{label}</span>}
    </label>
  );
}

export function Field({ label, hint, children }: { label: ReactNode; hint?: ReactNode; children: ReactNode }) {
  return (
    <div className="field">
      <label>{label}</label>
      {children}
      {hint && <span className="hint">{hint}</span>}
    </div>
  );
}

export function TextInput({ className = "", ...rest }: InputHTMLAttributes<HTMLInputElement>) {
  return <input className={`input ${className}`} {...rest} />;
}

export function Select<T extends string>({
  value,
  onChange,
  options,
}: {
  value: T;
  onChange: (value: T) => void;
  options: { value: T; label: string }[];
}) {
  return (
    <select className="select" value={value} onChange={(event) => onChange(event.target.value as T)}>
      {options.map((option) => (
        <option key={option.value} value={option.value}>
          {option.label}
        </option>
      ))}
    </select>
  );
}

export function Modal({
  open,
  title,
  onClose,
  children,
  footer,
}: {
  open: boolean;
  title: ReactNode;
  onClose?: () => void;
  children: ReactNode;
  footer?: ReactNode;
}) {
  if (!open) return null;
  return (
    <div className="modal-backdrop" onMouseDown={(event) => event.target === event.currentTarget && onClose?.()}>
      <div className="modal" role="dialog" aria-modal>
        <div className="row between">
          <h2>{title}</h2>
          {onClose && <IconButton label={t("app.close")} icon="x" onClick={onClose} />}
        </div>
        <div className="modal-body">{children}</div>
        {footer && <div className="modal-foot">{footer}</div>}
      </div>
    </div>
  );
}

export function EmptyState({ icon, title, hint, action }: { icon: IconName; title: string; hint?: string; action?: ReactNode }) {
  return (
    <div className="empty">
      <Icon name={icon} size={36} />
      <h3>{title}</h3>
      {hint && <p>{hint}</p>}
      {action}
    </div>
  );
}

export function Progress({ value, failed = false }: { value: number; failed?: boolean }) {
  return (
    <div className={`progress ${failed ? "failed" : ""}`}>
      <i style={{ width: `${Math.max(0, Math.min(100, value * 100))}%` }} />
    </div>
  );
}

/** Single-colour smooth line; the colour comes from CSS (`color`). */
export function Sparkline({ values }: { values: number[] }) {
  const width = 160;
  const height = 46;
  const points = values.length >= 2 ? values : [0, ...values, 0];
  const max = Math.max(1, ...points);
  const step = width / (points.length - 1);
  const coords = points.map((value, index) => [index * step, height - 6 - (value / max) * (height - 12)]);
  let d = `M${coords[0][0]},${coords[0][1]}`;
  for (let i = 1; i < coords.length; i += 1) {
    const [x0, y0] = coords[i - 1];
    const [x1, y1] = coords[i];
    const cx = (x0 + x1) / 2;
    d += ` C${cx},${y0} ${cx},${y1} ${x1},${y1}`;
  }
  const last = coords[coords.length - 1];
  return (
    <svg className="sparkline" viewBox={`0 0 ${width} ${height}`} aria-hidden>
      <defs>
        <linearGradient id="spark-fill" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="currentColor" stopOpacity="0.28" />
          <stop offset="1" stopColor="currentColor" stopOpacity="0" />
        </linearGradient>
      </defs>
      <path className="area" d={`${d} L${width},${height} L0,${height} Z`} />
      <path d={d} />
      <circle cx={last[0]} cy={last[1]} r={3.5} />
    </svg>
  );
}

export function DeviceIcon({
  type,
  model,
  online,
  className = "",
}: {
  type: string | null;
  model: string | null;
  /** Draws the green presence dot (iOS tiles); omit to show none. */
  online?: boolean;
  className?: string;
}) {
  const name: IconName =
    type === "mobile" ? "phone" : type === "server" || type === "headless" ? "server" : type === "web" ? "globe" : model?.toLowerCase().includes("mac") ? "laptop" : "desktop";
  // Tinted by kind (ADR-0024): phones blue, computers graphite, the rest teal.
  const kind = name === "phone" ? "phone" : name === "laptop" || name === "desktop" ? "computer" : "other";
  return (
    <div className={`device-icon ${kind} ${className}`}>
      <Icon name={name} size={22} />
      {online && <span className="presence" />}
    </div>
  );
}

export function Avatar({ name }: { name: string }) {
  const initials = name
    .split(/\s+/)
    .map((part) => part[0])
    .filter(Boolean)
    .slice(0, 2)
    .join("")
    .toUpperCase();
  return <div className="avatar">{initials || "?"}</div>;
}

export function Stat({ label, value }: { label: ReactNode; value: ReactNode }) {
  return (
    <div className="stat">
      <span className="label">{label}</span>
      <span className="value mono">{value}</span>
    </div>
  );
}

export function Spinner() {
  return <span className="spinner" aria-hidden />;
}
