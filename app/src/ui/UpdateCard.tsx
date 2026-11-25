// Update card: a calm, outlined card in the conversation column and in Settings (never a modal,
// never over a running task). Release notes are shown as plain text, never as HTML.
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ArrowUpRight, ChevronRight, CircleAlert, CircleArrowUp, LoaderCircle } from "lucide-react";
import { Button } from "./primitives";
import { S } from "../strings";

const T = S.update;

export type Update = {
  phase: "idle" | "checking" | "none" | "available" | "downloading" | "verifying" | "ready" | "installing" | "error";
  installed: string; hidden: boolean; got: number; total: number; note: string; error: string; portable: boolean;
  auto: boolean; beta: boolean; lastCheck: number;
  latest: { version: string; date: string; notes: string; page: string; pre: boolean } | null;
};

/** The backend's update state, live. */
export function useUpdate(): Update | null {
  const [u, setU] = useState<Update | null>(null);
  useEffect(() => {
    invoke<Update>("update_state").then(setU).catch(() => {});
    const un = listen<Update>("update", ({ payload }) => setU(payload));
    return () => { un.then((f) => f()); };
  }, []);
  return u;
}

/** Worth a card or a dot: a newer version the user hasn't skipped or postponed. */
export const showUpdate = (u: Update | null) => !!u?.latest && !u.hidden && u.phase !== "none" && u.phase !== "idle" && u.phase !== "checking";

export const fmtDate = (iso: string, time = false) => {
  const d = new Date(iso);
  return isNaN(d.getTime()) ? "" : d.toLocaleString("en-GB", time ? { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" } : { day: "numeric", month: "short", year: "numeric" });
};
const mb = (n: number) => (n / 1048576).toFixed(1);

export function UpdateCard({ u, busy, onError }: { u: Update; busy: boolean; onError: (t: string) => void }) {
  const [open, setOpen] = useState(false);
  const r = u.latest;
  if (!r) return null;
  const call = (cmd: string) => invoke(cmd).catch((e) => onError(String(e)));
  const pct = u.total > 0 ? Math.min(100, (u.got / u.total) * 100) : 0;

  return (
    <section className="update" aria-labelledby="upd-h">
      <header>
        <CircleArrowUp size={16} aria-hidden />
        <h2 id="upd-h">{T.title}</h2>
        {r.pre && <span className="update-tag">{T.beta}</span>}
      </header>
      <dl>
        <div><dt>{T.installed}</dt><dd>{u.installed}</dd></div>
        <div><dt>{T.latest}</dt><dd>{T.latestValue(r.version, fmtDate(r.date))}</dd></div>
      </dl>
      <div className="update-notes">
        <button type="button" className="update-more" aria-expanded={open} onClick={() => setOpen(!open)}>
          <ChevronRight size={14} aria-hidden className={open ? "turned" : ""} />{T.whatsNew}
        </button>
        {open && <p className="update-text">{r.notes.trim() || T.noNotes}</p>}
      </div>

      {u.phase === "downloading" || u.phase === "verifying" ? (
        <div className="update-progress">
          <div className="bar" role="progressbar" aria-label={T.downloading(mb(u.got), mb(u.total))} aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(pct)}>
            <span style={{ width: `${pct}%` }} />
          </div>
          <div className="update-row">
            <span className="update-status">{u.phase === "verifying" ? T.verifying : T.downloading(mb(u.got), mb(u.total))}</span>
            {u.phase === "downloading" && <Button size="sm" onClick={() => call("update_cancel")}>{T.cancel}</Button>}
          </div>
        </div>
      ) : u.phase === "ready" ? (
        <div className="update-row">
          <span className="update-status strong">{T.confirm(r.version)}</span>
          <Button size="sm" variant="primary" disabled={busy} title={busy ? T.installBusy : undefined} onClick={() => call("update_run")} autoFocus>{T.continue}</Button>
          <Button size="sm" onClick={() => call("update_cancel")}>{T.cancel}</Button>
        </div>
      ) : u.phase === "installing" ? (
        <div className="update-row"><LoaderCircle size={14} className="spin" aria-hidden /><span className="update-status">{T.installing}</span></div>
      ) : (
        <>
          {u.phase === "error" && <p className="update-error" role="alert"><CircleAlert size={14} aria-hidden /><span>{u.error}</span></p>}
          {u.portable && <p className="update-hint">{T.portable}</p>}
          <div className="update-row">
            {u.portable
              ? <Button size="sm" variant="primary" icon={ArrowUpRight} onClick={() => call("update_open")}>{T.getIt}</Button>
              : <Button size="sm" variant="primary" disabled={busy} title={busy ? T.installBusy : undefined} onClick={() => call("update_install")}>{u.phase === "error" ? T.retry : T.install}</Button>}
            <Button size="sm" onClick={() => call("update_later")}>{T.later}</Button>
            <Button size="sm" variant="ghost" onClick={() => call("update_skip")}>{T.skip}</Button>
            {!u.portable && <button type="button" className="link update-link" onClick={() => call("update_open")}>{T.view}<ArrowUpRight size={12} aria-hidden /></button>}
          </div>
          {busy && !u.portable && <p className="update-hint">{T.installBusy}</p>}
        </>
      )}
    </section>
  );
}
