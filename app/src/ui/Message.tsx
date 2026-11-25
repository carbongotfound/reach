// A turn in the conversation, and the steps between turns as a timeline: the app's icon, one
// sentence, and where it happened. Older groups fold into one row.
import { useState } from "react";
import { AppWindow, Camera, ChevronRight, CircleAlert, Info, Keyboard, MousePointer2, ScanSearch, TriangleAlert, X } from "lucide-react";
import { describe, md, rich, KIND, type Role, type Step } from "../model";
import { AppIcon } from "./AppIcon";
import { S } from "../strings";

const TOOL_ICON = { screenshot: Camera, keyboard: Keyboard, mouse: MousePointer2, ui: ScanSearch, apps: AppWindow } as const;

export function Message({ role, text }: { role: Role; text: string }) {
  if (role === "user") return <div className="msg user"><div className="bubble">{text}</div></div>;
  if (role === "agent") return <div className="msg agent">{md(text)}</div>;
  const Ic = role === "error" ? CircleAlert : role === "warn" ? TriangleAlert : Info;
  return <div className={`notice ${role}`} role={role === "error" ? "alert" : undefined}><Ic size={16} aria-hidden /><span>{rich(text)}</span></div>;
}

const dur = (ms: number) => (ms < 1000 ? S.steps.ms(ms) : S.steps.secs((ms / 1000).toFixed(1)));

/** One step: the app's icon in a tile, the short action, then app · screen (the time is in the tooltip). */
export function StepRow({ s, names, active, selected, onPick }: { s: Step; names: Record<string, string>; active: boolean; selected: boolean; onPick?: () => void }) {
  const text = describe(s, names);
  const where = [s.win?.app || KIND[s.tool] || s.tool, s.win ? S.sidebar.screen(s.win.screen) : ""].filter(Boolean).join(" · ");
  return (
    <li className={`step ${s.ok ? "" : "bad"} ${active ? "active" : ""} ${selected ? "sel" : ""}`}>
      <button type="button" className="step-btn" onClick={onPick} disabled={!onPick} title={dur(s.ms)}
        aria-label={S.steps.label(s.n, text) + (s.ok ? "" : S.steps.failed(s.text))} aria-current={selected || undefined}>
        <span className="step-tile" aria-hidden>
          <AppIcon appKey={s.win?.key} size={18} fallback={TOOL_ICON[s.tool as keyof typeof TOOL_ICON] ?? AppWindow} />
          {!s.ok && <span className="step-x"><X size={9} strokeWidth={3} /></span>}
        </span>
        <span className="step-tx">
          <span className="step-desc">{text}</span>
          {!s.ok && <span className="step-err">{s.text}</span>}
          <span className="step-meta">{where}</span>
        </span>
      </button>
    </li>
  );
}

/** The apps a group of steps used, in order of first use. */
const appsOf = (steps: Step[]) => {
  const seen = new Map<string, string>();
  for (const s of steps) if (s.win?.app && !seen.has(s.win.app)) seen.set(s.win.app, s.win.key);
  return [...seen];
};

/** Steps between two messages. Open while it's the one being worked on; otherwise a one-line summary. */
export function StepGroup({ steps, names, live, last, selected, onPick, working }: {
  steps: Step[]; names: Record<string, string>; live: boolean; last: boolean; selected: number | null; onPick: (s: Step) => void; working: boolean;
}) {
  const [open, setOpen] = useState<boolean | null>(null);
  const isOpen = open ?? (live && last);
  const failed = steps.filter((s) => !s.ok).length;
  const lastStep = steps[steps.length - 1];
  const apps = appsOf(steps);
  const summary = S.run.stepsIn(steps.length, apps.map(([a]) => a).slice(0, 3).join(", ")) + (failed ? S.steps.didntWork(failed) : "");
  return (
    <div className={`steps ${isOpen ? "open" : ""}`}>
      <button type="button" className="steps-head" aria-expanded={isOpen} onClick={() => setOpen(!isOpen)}>
        {apps.length > 0 && <span className="steps-apps" aria-hidden>{apps.slice(0, 3).map(([a, k]) => <span key={a} className="steps-app"><AppIcon appKey={k} size={14} /></span>)}</span>}
        <span>{summary}</span>
        <ChevronRight size={14} className="chev" aria-hidden />
      </button>
      {isOpen && (
        <ol className="step-list">
          {steps.map((s) => (
            <StepRow key={s.n} s={s} names={names} active={working && last && s === lastStep} selected={selected === s.n} onPick={s.src ? () => onPick(s) : undefined} />
          ))}
        </ol>
      )}
    </div>
  );
}
