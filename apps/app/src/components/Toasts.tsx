import { Icon } from "./icons";
import { dismissToast, useStore } from "../store";

export function Toasts() {
  const toasts = useStore((s) => s.toasts);
  return (
    <div className="toasts">
      {toasts.map((toast) => (
        <div key={toast.id} className={`toast ${toast.kind}`} onClick={() => dismissToast(toast.id)}>
          <Icon name={toast.kind === "error" ? "x" : toast.kind === "success" ? "check" : "bell"} size={16} />
          <span>{toast.text}</span>
        </div>
      ))}
    </div>
  );
}
