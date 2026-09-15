import { useEffect, useRef } from "react";
import { Icon } from "./icons";
import { dismissAllToasts, dismissToast, useStore } from "../store";

export function Toasts() {
  const toasts = useStore((s) => s.toasts);
  const container = useRef<HTMLDivElement>(null);

  // Tapping anywhere else clears the toasts. The listener runs on the capture
  // phase so it still fires when the tap lands on a control that stops
  // propagation, and the tap itself is left alone: whatever was under it stays
  // clickable, which is the point of getting the toasts out of the way.
  useEffect(() => {
    if (toasts.length === 0) return;
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target;
      if (target instanceof Node && container.current?.contains(target)) return;
      dismissAllToasts();
    };
    document.addEventListener("pointerdown", onPointerDown, true);
    return () => document.removeEventListener("pointerdown", onPointerDown, true);
  }, [toasts.length]);

  return (
    <div className="toasts" ref={container}>
      {toasts.map((toast) => (
        <div key={toast.id} className={`toast ${toast.kind}`} onClick={() => dismissToast(toast.id)}>
          <Icon name={toast.kind === "error" ? "x" : toast.kind === "success" ? "check" : "bell"} size={16} />
          <span>{toast.text}</span>
        </div>
      ))}
    </div>
  );
}
