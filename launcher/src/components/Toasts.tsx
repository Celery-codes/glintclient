import { AlertCircle, CheckCircle2, Info, X } from "lucide-react";
import { useToasts, type ToastKind } from "../stores/toasts";

const ICONS: Record<ToastKind, React.ReactNode> = {
  error: <AlertCircle size={18} className="text-danger" />,
  info: <Info size={18} className="text-muted" />,
  success: <CheckCircle2 size={18} className="text-success" />,
};

export function Toasts() {
  const { toasts, dismiss } = useToasts();
  return (
    <div className="pointer-events-none fixed right-5 bottom-5 z-50 flex w-[360px] flex-col gap-2">
      {toasts.map((t) => (
        <div
          key={t.id}
          role={t.kind === "error" ? "alert" : "status"}
          className="pointer-events-auto flex items-start gap-3 rounded-md border border-border bg-raised px-3.5 py-3"
        >
          <div className="mt-px shrink-0">{ICONS[t.kind]}</div>
          <p className="selectable min-w-0 flex-1 text-[13px] leading-snug break-words">{t.message}</p>
          <button
            type="button"
            aria-label="Dismiss"
            onClick={() => dismiss(t.id)}
            className="shrink-0 rounded-sm text-faint hover:text-text"
          >
            <X size={16} />
          </button>
        </div>
      ))}
    </div>
  );
}
