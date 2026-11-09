import { Boxes, Check, Home, LogOut, Puzzle, Settings, Shirt, UserPlus } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { NavLink } from "react-router-dom";
import { Sparkle } from "../components/ui";
import { cx } from "../lib/format";
import { useAccounts } from "../stores/accounts";

const NAV = [
  { to: "/", label: "Home", icon: Home, end: true },
  { to: "/instances", label: "Instances", icon: Boxes, end: false },
  { to: "/mods", label: "Mods", icon: Puzzle, end: false },
  { to: "/skins", label: "Skins", icon: Shirt, end: false },
] as const;

function NavItem({ to, label, icon: Icon, end }: { to: string; label: string; icon: typeof Home; end: boolean }) {
  return (
    <NavLink
      to={to}
      end={end}
      title={label}
      aria-label={label}
      className={({ isActive }) =>
        cx(
          "relative flex h-11 w-11 items-center justify-center rounded-md transition-colors",
          isActive ? "bg-accent/14 text-accent" : "text-muted hover:bg-raised hover:text-text",
        )
      }
    >
      {({ isActive }) => (
        <>
          {isActive && <span className="absolute -left-2.5 h-5 w-1 rounded-r-full bg-accent" />}
          <Icon size={20} strokeWidth={isActive ? 2.2 : 1.9} />
        </>
      )}
    </NavLink>
  );
}

function AccountMenu() {
  const accounts = useAccounts((s) => s.accounts);
  const active = accounts.accounts.find((a) => a.uuid === accounts.active) ?? null;
  const { beginLogin, setActive, remove } = useAccounts.getState();
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener("mousedown", close);
    return () => window.removeEventListener("mousedown", close);
  }, [open]);

  return (
    <div ref={ref} className="relative">
      <button
        type="button"
        title={active ? active.username : "Sign in"}
        aria-label="Accounts"
        onClick={() => setOpen(!open)}
        className={cx(
          "flex h-10 w-10 items-center justify-center rounded-full text-[15px] font-semibold transition-colors",
          active
            ? "bg-accent/14 text-accent hover:bg-accent/22"
            : "bg-raised text-muted hover:text-text",
        )}
      >
        {active ? active.username[0].toUpperCase() : <UserPlus size={18} />}
      </button>

      {open && (
        <div className="absolute bottom-0 left-14 z-30 w-60 rounded-md border border-border bg-raised p-1.5">
          {accounts.accounts.map((account) => (
            <div key={account.uuid} className="group flex items-center">
              <button
                type="button"
                onClick={() => {
                  void setActive(account.uuid);
                  setOpen(false);
                }}
                className="flex h-9 min-w-0 flex-1 items-center gap-2 rounded-sm px-2 text-left hover:bg-border"
              >
                <span className="w-4 shrink-0 text-accent">
                  {account.uuid === accounts.active && <Check size={15} />}
                </span>
                <span className="truncate">{account.username}</span>
              </button>
              <button
                type="button"
                title={`Sign out ${account.username}`}
                aria-label={`Sign out ${account.username}`}
                onClick={() => void remove(account.uuid)}
                className="flex h-9 w-9 shrink-0 items-center justify-center rounded-sm text-faint opacity-0 group-hover:opacity-100 hover:bg-border hover:text-danger focus-visible:opacity-100"
              >
                <LogOut size={15} />
              </button>
            </div>
          ))}
          {accounts.accounts.length > 0 && <div className="my-1.5 h-px bg-border" />}
          <button
            type="button"
            onClick={() => {
              setOpen(false);
              void beginLogin();
            }}
            className="flex h-9 w-full items-center gap-2 rounded-sm px-2 text-left hover:bg-border"
          >
            <UserPlus size={15} className="w-4 shrink-0 text-muted" />
            {accounts.accounts.length > 0 ? "Add another account" : "Sign in with Microsoft"}
          </button>
        </div>
      )}
    </div>
  );
}

export function Sidebar() {
  return (
    <nav className="flex w-16 shrink-0 flex-col items-center gap-1.5 border-r border-border bg-surface py-3">
      <div className="mb-3 flex h-11 w-11 items-center justify-center text-accent" title="Glint">
        <Sparkle size={26} />
      </div>
      {NAV.map((item) => (
        <NavItem key={item.to} {...item} />
      ))}
      <div className="flex-1" />
      <NavItem to="/settings" label="Settings" icon={Settings} end={false} />
      <div className="mt-2">
        <AccountMenu />
      </div>
    </nav>
  );
}
