// Small building blocks: Button, IconButton, Tooltip, Menu, Toast, Switch.
import { useEffect, useRef, useState, type ButtonHTMLAttributes, type ReactNode } from "react";
import { Check, ChevronDown, type LucideIcon } from "lucide-react";

type BtnProps = ButtonHTMLAttributes<HTMLButtonElement> & { variant?: "primary" | "secondary" | "ghost" | "danger"; size?: "sm" | "md" | "lg"; icon?: LucideIcon };

export function Button({ variant = "secondary", size = "md", icon: Ic, className = "", children, ...p }: BtnProps) {
  return (
    <button type="button" className={`btn ${variant} ${size} ${className}`} {...p}>
      {Ic && <Ic size={size === "sm" ? 14 : 16} aria-hidden />}
      {children}
    </button>
  );
}

/** Square icon button. `label` is the accessible name and the tooltip. */
export function IconButton({ icon: Ic, label, tip = "bottom", className = "", size = 18, ...p }: ButtonHTMLAttributes<HTMLButtonElement> & { icon: LucideIcon; label: string; tip?: "bottom" | "top" | "right" | "none"; size?: number }) {
  return (
    <button type="button" className={`icon-btn ${className}`} aria-label={label} data-tip={tip === "none" ? undefined : label} data-tip-pos={tip} {...p}>
      <Ic size={size} aria-hidden />
    </button>
  );
}

/** A hover/focus tooltip on any element (CSS only, appears after a short delay). */
export function Tooltip({ label, pos = "top", children }: { label: string; pos?: "top" | "bottom" | "right"; children: ReactNode }) {
  return <span className="tip-wrap" data-tip={label} data-tip-pos={pos}>{children}</span>;
}

export type MenuItem = { value: string; label: string; hint?: string; disabled?: boolean; icon?: ReactNode };

/** A button that opens a list of choices (role=menu, menuitemradio). Arrow keys, Enter, Escape. */
export function Menu({ label, value, items, onPick, disabled, title, lead, up = true }: {
  label: string; value: string; items: MenuItem[]; onPick: (v: string) => void; disabled?: boolean; title?: string; lead?: ReactNode; up?: boolean;
}) {
  const [open, setOpen] = useState(false);
  const [hi, setHi] = useState(0);
  const root = useRef<HTMLDivElement>(null);
  const btn = useRef<HTMLButtonElement>(null);
  const cur = items.find((i) => i.value === value);
  useEffect(() => {
    if (!open) return;
    setHi(Math.max(0, items.findIndex((i) => i.value === value)));
    const away = (e: MouseEvent) => { if (!root.current?.contains(e.target as Node)) setOpen(false); };
    document.addEventListener("mousedown", away);
    return () => document.removeEventListener("mousedown", away);
  }, [open]);
  const pick = (i: MenuItem) => { if (i.disabled) return; onPick(i.value); setOpen(false); btn.current?.focus(); };
  const onKey = (e: React.KeyboardEvent) => {
    if (!open) { if (e.key === "ArrowDown" || e.key === "ArrowUp") { e.preventDefault(); setOpen(true); } return; }
    if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); setOpen(false); btn.current?.focus(); }
    else if (e.key === "ArrowDown") { e.preventDefault(); setHi((h) => (h + 1) % items.length); }
    else if (e.key === "ArrowUp") { e.preventDefault(); setHi((h) => (h - 1 + items.length) % items.length); }
    else if (e.key === "Enter" || e.key === " ") { e.preventDefault(); pick(items[hi]); }
    else if (e.key === "Tab") setOpen(false);
  };
  return (
    <div className="menu-wrap" ref={root} onKeyDown={onKey}>
      <button ref={btn} type="button" className={`pill ${open ? "on" : ""}`} aria-haspopup="menu" aria-expanded={open} aria-label={`${label}: ${cur?.label ?? value}`}
        disabled={disabled} title={title} onClick={() => setOpen(!open)}>
        {lead}{cur?.icon}{cur?.label ?? value}<ChevronDown size={14} aria-hidden className="chev" />
      </button>
      {open && (
        <div className={`menu ${up ? "up" : "down"}`} role="menu" aria-label={label}>
          <div className="menu-label" aria-hidden>{label}</div>
          {items.map((i, n) => (
            <button key={i.value} type="button" role="menuitemradio" aria-checked={i.value === value} aria-disabled={i.disabled}
              className={`menu-item ${n === hi ? "hi" : ""}`} tabIndex={-1} onMouseEnter={() => setHi(n)} onClick={() => pick(i)}
              ref={(el) => { if (el && n === hi) el.focus({ preventScroll: true }); }}>
              {i.icon}<span className="mi-text"><span className="mi-label">{i.label}</span>{i.hint && <span className="mi-hint">{i.hint}</span>}</span>
              {i.value === value && <Check size={16} aria-hidden className="mi-check" />}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

export function Switch({ on, onChange, label, disabled }: { on: boolean; onChange: (v: boolean) => void; label: string; disabled?: boolean }) {
  return (
    <button type="button" role="switch" aria-checked={on} aria-label={label} disabled={disabled} className={`switch ${on ? "on" : ""}`} onClick={() => onChange(!on)}>
      <span className="knob" aria-hidden />
    </button>
  );
}

export function Toast({ text }: { text: string }) {
  return <div className="toast" role="status" aria-live="polite">{text}</div>;
}
