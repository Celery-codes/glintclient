// Small shared primitives. Everything takes its colours from the theme tokens.
import { X } from "lucide-react";
import {
  useEffect,
  useId,
  type ButtonHTMLAttributes,
  type InputHTMLAttributes,
  type ReactNode,
  type SelectHTMLAttributes,
} from "react";
import { cx } from "../lib/format";

type ButtonVariant = "primary" | "secondary" | "ghost" | "danger";

const BUTTON_VARIANTS: Record<ButtonVariant, string> = {
  primary: "bg-accent text-on-accent hover:bg-accent-hover font-semibold",
  secondary: "bg-raised text-text hover:bg-border",
  ghost: "text-muted hover:bg-raised hover:text-text",
  danger: "text-danger hover:bg-danger/10",
};

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: "sm" | "md";
  icon?: ReactNode;
}

export function Button({
  variant = "secondary",
  size = "md",
  icon,
  className,
  children,
  ...props
}: ButtonProps) {
  return (
    <button
      type="button"
      {...props}
      className={cx(
        "inline-flex shrink-0 items-center justify-center gap-2 rounded-md transition-colors",
        "disabled:pointer-events-none disabled:opacity-45",
        "[&>svg]:shrink-0",
        size === "sm" ? "h-8 text-[13px]" : "h-9",
        // Icon-only buttons are square; padding would squeeze the icon out.
        children ? (size === "sm" ? "px-3" : "px-4") : size === "sm" ? "w-8" : "w-9",
        BUTTON_VARIANTS[variant],
        className,
      )}
    >
      {icon}
      {children}
    </button>
  );
}

const FIELD =
  "h-9 rounded-md border border-border bg-bg px-3 text-text placeholder:text-faint " +
  "transition-colors hover:border-faint focus:border-accent focus:outline-none " +
  "disabled:opacity-50 disabled:hover:border-border";

/** Fields fill their container unless the caller sets a width. Two width classes on
 *  one element would leave the winner up to stylesheet order. */
function fieldWidth(className?: string): string | false {
  return !/(^|s)w-/.test(className ?? "") && "w-full";
}

export function Input({ className, ...props }: InputHTMLAttributes<HTMLInputElement>) {
  return (
    <input
      spellCheck={false}
      autoComplete="off"
      {...props}
      className={cx(FIELD, fieldWidth(className), className)}
    />
  );
}

export function Select({ className, ...props }: SelectHTMLAttributes<HTMLSelectElement>) {
  return <select {...props} className={cx(FIELD, fieldWidth(className), "pr-2", className)} />;
}

interface FieldProps {
  label: string;
  hint?: ReactNode;
  children: (id: string) => ReactNode;
}

/** A labelled form row. The render prop receives the id to put on the control. */
export function Field({ label, hint, children }: FieldProps) {
  const id = useId();
  return (
    <div className="flex flex-col gap-1.5">
      <label htmlFor={id} className="text-[13px] font-medium text-text">
        {label}
      </label>
      {children(id)}
      {hint && <p className="text-[12.5px] leading-snug text-muted">{hint}</p>}
    </div>
  );
}

interface DialogProps {
  title: string;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
  width?: string;
}

export function Dialog({ title, onClose, children, footer, width = "max-w-md" }: DialogProps) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div
      className="fixed inset-0 z-40 flex items-center justify-center bg-black/60 p-6"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label={title}
        className={cx(
          "flex max-h-full w-full flex-col rounded-lg border border-border bg-surface",
          width,
        )}
      >
        <header className="flex items-center justify-between px-5 pt-4 pb-1">
          <h2 className="text-[16px] font-semibold">{title}</h2>
          <Button variant="ghost" size="sm" aria-label="Close" onClick={onClose} icon={<X size={16} />} />
        </header>
        <div className="overflow-y-auto px-5 py-3">{children}</div>
        {footer && <footer className="flex justify-end gap-2 px-5 pt-2 pb-4">{footer}</footer>}
      </div>
    </div>
  );
}

interface EmptyStateProps {
  icon: ReactNode;
  title: string;
  children?: ReactNode;
  action?: ReactNode;
}

export function EmptyState({ icon, title, children, action }: EmptyStateProps) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 px-8 text-center">
      <div className="text-faint">{icon}</div>
      <h2 className="text-[16px] font-semibold">{title}</h2>
      {children && <p className="max-w-sm text-muted">{children}</p>}
      {action && <div className="mt-2">{action}</div>}
    </div>
  );
}

interface PageProps {
  title: string;
  actions?: ReactNode;
  children: ReactNode;
}

/** Standard page frame: a title row and a scrolling body. */
export function Page({ title, actions, children }: PageProps) {
  return (
    <div className="flex h-full flex-col">
      <header className="flex h-16 shrink-0 items-center justify-between px-8">
        <h1 className="text-[20px] font-semibold tracking-tight">{title}</h1>
        <div className="flex items-center gap-2">{actions}</div>
      </header>
      <div className="min-h-0 flex-1 overflow-y-auto px-8 pb-8">{children}</div>
    </div>
  );
}

export function Sparkle({ size = 20, className }: { size?: number; className?: string }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor" className={className} aria-hidden>
      <path d="M12 1.5c.7 6.2 4.3 9.8 10.5 10.5-6.2.7-9.8 4.3-10.5 10.5C11.3 16.3 7.7 12.7 1.5 12 7.7 11.3 11.3 7.7 12 1.5Z" />
    </svg>
  );
}
