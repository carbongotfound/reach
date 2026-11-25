// Approve / Edit / Deny. Denied or no reply in 2 minutes counts as deny (the backend times it).
import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Check, Pencil, ShieldAlert, ShieldCheck, ShieldX, X } from "lucide-react";
import { cap, fmtDur, type Approval } from "../model";
import { Button } from "./primitives";
import { S } from "../strings";

const ANSWERED = S.approval.answered;

export function ApprovalCard({ a, agent, live, now, onZoom, onError, region }: { a: Approval; agent: string; live: boolean; now: number; onZoom: (s: string) => void; onError: (s: string) => void; region?: [number, number, number, number] }) {
  const [editing, setEditing] = useState(false);
  const [text, setText] = useState(a.content);
  const [busy, setBusy] = useState(false);
  const open = live && !a.decision;
  // Focus the card, not the Approve button: a stray Space or Enter (typing elsewhere when the
  // window comes to the front) must never approve. Tab reaches Approve next.
  const card = useRef<HTMLElement>(null);
  useEffect(() => { if (open) card.current?.focus({ preventScroll: true }); }, [open]);
  const left = Math.max(0, Math.ceil(a.wait - (now - a.t) / 1000));
  const answer = async (decision: string) => {
    setBusy(true);
    try { await invoke("answer_approval", { id: a.id, decision, text: decision === "edit" ? text : null }); } catch (e) { onError(String(e)); setBusy(false); }
  };
  const ok = a.decision === "approve" || a.decision === "edit";
  const title = open ? (a.by === "engine" ? S.approval.engineTitle : S.approval.asking(S.agents[agent as "claude"] ?? S.agents.claude)) : ANSWERED[a.decision ?? ""] ?? (a.decision ? cap(a.decision) : S.approval.notAnswered);
  const rows: [string, string][] = [[S.approval.where, a.where], [S.approval.why, a.why], [S.approval.undo, a.undo ? cap(a.undo) : ""]];
  if (!open) {
    return (
      <div className={`approval done ${ok ? "ok" : "no"}`} aria-label={S.approval.doneLabel(a.action, title)}>
        {ok ? <ShieldCheck size={16} aria-hidden /> : <ShieldX size={16} aria-hidden />}
        <span className="approval-sum"><b>{a.action}</b> · {title}</span>
        {a.decision === "edit" && a.text && <span className="approval-changed">{S.approval.changedTo(a.text)}</span>}
      </div>
    );
  }
  return (
    <section className="approval open" aria-label={S.approval.label(a.action)} role="group" ref={card} tabIndex={-1}>
      <header>
        <ShieldAlert size={16} aria-hidden className="approval-icon" />
        <span className="approval-title">{title}</span>
        <span className="approval-left" aria-label={S.approval.secondsLeft(left)}>{fmtDur(left * 1000)}</span>
      </header>
      <p className="approval-action">{a.action}</p>
      {editing ? (
        <textarea className="approval-edit" value={text} onChange={(e) => setText(e.target.value)} aria-label={S.approval.editLabel} rows={4} autoFocus />
      ) : a.content ? <blockquote className="approval-content">{a.content}</blockquote> : null}
      <div className="approval-body">
        <dl>
          {rows.filter(([, v]) => v).map(([k, v]) => <div key={k}><dt>{k}</dt><dd>{v}</dd></div>)}
        </dl>
        {a.src && <Shot src={a.src} box={a.box} region={region} where={a.where} onZoom={onZoom} />}
      </div>
      <div className="approval-actions">
        {editing ? (
          <>
            <Button variant="primary" icon={Check} disabled={busy || !text.trim()} onClick={() => answer("edit")}>{S.approval.approveWithChanges}</Button>
            <Button variant="ghost" disabled={busy} onClick={() => { setEditing(false); setText(a.content); }}>{S.approval.cancel}</Button>
          </>
        ) : (
          <>
            <Button variant="primary" icon={Check} disabled={busy} onClick={() => answer("approve")}>{S.approval.approve}</Button>
            {a.by === "agent" && <Button icon={Pencil} disabled={busy} onClick={() => setEditing(true)}>{S.approval.edit}</Button>}
            <Button icon={X} disabled={busy} onClick={() => answer("deny")}>{S.approval.deny}</Button>
          </>
        )}
        <span className="approval-hint">{S.approval.hint}</span>
      </div>
    </section>
  );
}

/** A crop of the screen around the control it will press (the guard's box), outlined; else the window or the whole shot. */
function Shot({ src, box, region, where, onZoom }: { src: string; box?: [number, number, number, number]; region?: [number, number, number, number]; where: string; onZoom: (s: string) => void }) {
  const [ar, setAr] = useState(16 / 9);
  // no target: the window it's working in, else the whole screen
  let crop: [number, number, number, number] = region ?? [0, 0, 1, 1];
  if (box) {
    let cw = Math.min(1, Math.max(box[2] * 3, 0.32));
    let ch = (cw * ar) / 1.6;
    if (ch > 1) { ch = 1; cw = Math.min(1, 1.6 / ar); }
    const fit = (c: number, s: number) => Math.min(1 - s, Math.max(0, c - s / 2));
    crop = [fit(box[0] + box[2] / 2, cw), fit(box[1] + box[3] / 2, ch), cw, ch];
  }
  const [x, y, w, h] = crop;
  return (
    <button type="button" className="approval-shot" onClick={() => onZoom(src)} aria-label={S.approval.zoomShot} style={{ aspectRatio: `${(w * ar) / h}` }}>
      <img src={src} alt={S.approval.shotAlt(where)} onLoad={(e) => { const i = e.currentTarget; if (i.naturalHeight) setAr(i.naturalWidth / i.naturalHeight); }}
        style={{ width: `${100 / w}%`, height: `${100 / h}%`, left: `${(-x / w) * 100}%`, top: `${(-y / h) * 100}%` }} />
      {box && <span className="approval-target" aria-hidden style={{ left: `${((box[0] - x) / w) * 100}%`, top: `${((box[1] - y) / h) * 100}%`, width: `${(box[2] / w) * 100}%`, height: `${(box[3] / h) * 100}%` }} />}
    </button>
  );
}
