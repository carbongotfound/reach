// Left column: new task, search, tasks with a status line, the PC's screens, settings.
// Collapses to an icon rail (Ctrl+B, and on its own in a narrow window).
import { Copy, Ellipsis, FolderOpen, PanelLeft, Pencil, Plus, Search, Settings, Square, Trash2, type LucideIcon } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { clip, fmtDay, idDate, type RunMeta } from "../model";
import { taskTitle } from "../title";
import { active } from "../runstate";
import { Button, IconButton } from "./primitives";
import { Mark } from "./Mark";
import { S } from "../strings";

const T = S.sidebar;
/** One meaning per colour: the accent working, amber needs you, red didn't finish; no dot when done or stopped. */
const DOT: Record<string, string> = { running: "live", asking: "ask", failed: "bad" };

type Live = { id: string; doing: string; app: string; asking: boolean; screen: number };

function status(r: RunMeta, live: Live | null) {
  if (live && r.id === live.id) {
    if (live.asking) return T.needsYou(live.app);
    if (active(r.state) && live.doing) return T.doing(live.doing, live.app);
  }
  if (active(r.state)) return S.state.running;
  const when = fmtDay(r.ended ? new Date(r.ended) : idDate(r.id), T.yesterday);
  return T.ended(S.state[r.state] ?? r.state, when);
}

const runName = (r: RunMeta) => r.title || taskTitle(r.task);
const typingIn = (t: EventTarget | null) => !!(t as HTMLElement)?.closest?.("textarea, input, [role=menu], [role=alertdialog]");

export function Sidebar({ runs, current, live, collapsed, onToggle, onNew, onOpen, onSettings, settingsOn, updateDot = false, onStop, onRefresh, onDeleted, onToast }: {
  runs: RunMeta[] | null; current: string; live: Live | null; collapsed: boolean; onToggle: () => void; onNew: () => void; onOpen: (id: string) => void; onSettings: () => void; settingsOn: boolean; updateDot?: boolean;
  onStop: () => void; onRefresh: () => void; onDeleted: (id: string) => void; onToast: (t: string) => void;
}) {
  const [find, setFind] = useState("");
  const [menu, setMenu] = useState<{ id: string; x: number; y: number } | null>(null);
  const [editing, setEditing] = useState("");
  const [asking, setAsking] = useState("");
  const findRef = useRef<HTMLInputElement>(null);
  const q = find.toLowerCase();
  const shown = useMemo(() => (runs ?? []).filter((r) => !q || (r.title ?? "").toLowerCase().includes(q) || r.task.toLowerCase().includes(q)), [runs, q]);
  const byId = (id: string) => runs?.find((r) => r.id === id);
  const rowBtn = (id: string) => document.querySelector<HTMLElement>(`[data-run="${id}"]`);
  const refocus = (id: string) => setTimeout(() => rowBtn(id)?.focus(), 0);
  const ask = (id: string) => { const r = byId(id); if (r && !active(r.state)) setAsking(id); };
  const renamed = useRef(""); // the row whose rename already ended (Enter/Esc), so the blur that follows is ignored
  const rename = (id: string, title: string | null) => {
    if (renamed.current === id) return;
    renamed.current = id;
    setEditing("");
    const r = byId(id);
    if (r && title !== null && title.trim() !== runName(r)) invoke("rename_run", { id, title }).then(onRefresh).catch((e) => onToast(String(e)));
    refocus(id);
  };
  const remove = (id: string) => { setAsking(""); invoke("delete_run", { id }).then(() => onDeleted(id)).catch((e) => onToast(String(e))); };
  const cancelAsk = () => { const id = asking; setAsking(""); refocus(id); };
  const act = (id: string, what: string) => {
    const r = byId(id); setMenu(null); if (!r) return;
    if (what === "rename") setEditing(id);
    else if (what === "folder") invoke("open_run_folder", { id }).catch((e) => onToast(String(e)));
    else if (what === "copy") navigator.clipboard.writeText(r.task).then(() => onToast(T.copied)).catch((e) => onToast(String(e)));
    else if (what === "stop") onStop();
    else if (what === "delete") ask(id);
    if (what !== "rename" && what !== "delete") refocus(id);
  };
  // F2 renames and Delete deletes the open task when focus isn't in a field (a focused row handles its own keys)
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || !current || editing || asking || menu || typingIn(e.target)) return;
      if (e.key === "F2") { e.preventDefault(); setEditing(current); }
      else if (e.key === "Delete") { e.preventDefault(); ask(current); }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });
  return (
    <aside className={`side ${collapsed ? "collapsed" : ""}`} aria-label={T.label}>
      <div className="side-top">
        {!collapsed && <span className="side-brand"><Mark size={22} />{T.brand}</span>}
        <IconButton icon={PanelLeft} label={collapsed ? T.open : T.close} tip={collapsed ? "right" : "bottom"} onClick={onToggle} />
      </div>
      <button type="button" className="side-item new" onClick={onNew} data-tip={collapsed ? T.newTaskTip : undefined} data-tip-pos="right" aria-label={T.newTask}>
        <Plus size={18} aria-hidden /><span className="side-text">{T.newTask}</span><kbd className="side-text">{T.newTaskKeys}</kbd>
      </button>
      {collapsed ? (
        <button type="button" className="side-item" onClick={() => { onToggle(); setTimeout(() => findRef.current?.focus(), 250); }} data-tip={T.search} data-tip-pos="right" aria-label={T.search}>
          <Search size={18} aria-hidden />
        </button>
      ) : (
        <label className="side-find">
          <Search size={16} aria-hidden />
          <input ref={findRef} value={find} onChange={(e) => setFind(e.target.value)} placeholder={T.search} aria-label={T.search} onKeyDown={(e) => e.key === "Escape" && setFind("")} />
        </label>
      )}
      <div className="side-scroll" hidden={collapsed}>
        <h3 className="side-h">{T.tasks}</h3>
        {runs === null && <div className="side-skel" aria-label={T.loading}><i /><i /><i /><i /></div>}
        {runs?.length === 0 && <p className="side-empty">{T.empty}</p>}
        {runs && runs.length > 0 && shown.length === 0 && <p className="side-empty">{T.noMatch(clip(find, 30))}</p>}
        <nav aria-label={T.history}>
          <ul>
            {shown.map((r) => {
              const st = live?.id === r.id && live.asking ? "asking" : r.state;
              const line = status(r, live);
              const lineEl = <span className={`side-run-line ${DOT[st] ?? ""}`}>{DOT[st] && <span className={`dot ${DOT[st]}`} aria-hidden />}<span>{line}</span></span>;
              if (editing === r.id) return (
                <li key={r.id} className="side-li">
                  <div className={`side-run ${r.id === current ? "sel" : ""}`}>
                    <input className="side-rename" defaultValue={runName(r)} aria-label={T.renameLabel} autoFocus maxLength={80} onFocus={(e) => { renamed.current = ""; e.currentTarget.select(); }}
                      onKeyDown={(e) => { if (e.key === "Enter") { e.preventDefault(); rename(r.id, e.currentTarget.value); } else if (e.key === "Escape") { e.preventDefault(); rename(r.id, null); } }}
                      onBlur={(e) => rename(r.id, e.currentTarget.value)} />
                    {lineEl}
                  </div>
                </li>
              );
              return (
                <li key={r.id} className={`side-li ${menu?.id === r.id ? "menu-on" : ""}`}>
                  <button type="button" data-run={r.id} className={`side-run ${r.id === current ? "sel" : ""}`} onClick={() => onOpen(r.id)} aria-current={r.id === current || undefined}
                    aria-label={`${runName(r)}. ${line}`} title={T.runTitle(r.task, st === "asking" ? T.needsYou(live!.app) : S.state[st] ?? st, r.steps)}
                    onContextMenu={(e) => { e.preventDefault(); const b = e.currentTarget.getBoundingClientRect(); setMenu({ id: r.id, x: e.clientX || b.left + 24, y: e.clientY || b.bottom }); }}
                    onKeyDown={(e) => { if (e.key === "F2") { e.preventDefault(); setEditing(r.id); } else if (e.key === "Delete") { e.preventDefault(); ask(r.id); } }}>
                    <span className="side-run-title">{runName(r)}</span>
                    {lineEl}
                  </button>
                  <IconButton className="side-more" icon={Ellipsis} label={T.more(runName(r))} tip="none" size={16} aria-haspopup="menu" aria-expanded={menu?.id === r.id}
                    onMouseDown={(e) => menu?.id === r.id && e.stopPropagation()}
                    onClick={(e) => { const b = e.currentTarget.getBoundingClientRect(); setMenu(menu?.id === r.id ? null : { id: r.id, x: b.left, y: b.bottom + 4 }); }} />
                </li>
              );
            })}
          </ul>
        </nav>
        <Screens busy={live && active(runs?.find((r) => r.id === live.id)?.state ?? "") ? live.screen : 0} />
      </div>
      {menu && byId(menu.id) && <RunMenu at={menu} running={active(byId(menu.id)!.state)} onPick={(w) => act(menu.id, w)} onClose={() => { const id = menu.id; setMenu(null); refocus(id); }} />}
      {asking && (
        <div className="confirm-back" onMouseDown={(e) => e.target === e.currentTarget && cancelAsk()}>
          <div className="confirm" role="alertdialog" aria-modal="true" aria-describedby="confirm-text"
            onKeyDown={(e) => {
              if (e.key === "Escape") { e.preventDefault(); cancelAsk(); }
              if (e.key === "Tab") { const b = [...e.currentTarget.querySelectorAll("button")]; const i = b.indexOf(document.activeElement as HTMLButtonElement); e.preventDefault(); b[(i + (e.shiftKey ? b.length - 1 : 1)) % b.length]?.focus(); }
            }}>
            <p id="confirm-text">{T.deleteAsk}</p>
            <div className="confirm-row">
              <Button autoFocus onClick={cancelAsk}>{T.cancel}</Button>
              <Button variant="danger" icon={Trash2} onClick={() => remove(asking)}>{T.delete}</Button>
            </div>
          </div>
        </div>
      )}
      <div className="side-foot">
        <button type="button" className={`side-item ${settingsOn ? "sel" : ""}`} onClick={onSettings} aria-current={settingsOn ? "page" : undefined} aria-label={updateDot ? `${T.settings}. ${S.update.dot}` : T.settings}
          data-tip={collapsed ? (updateDot ? `${T.settings} · ${S.update.dot}` : T.settings) : undefined} data-tip-pos="right">
          <span className="side-icon"><Settings size={18} aria-hidden />{updateDot && <span className="side-badge" aria-hidden />}</span><span className="side-text">{T.settings}</span>
        </button>
      </div>
    </aside>
  );
}

type MI = { v: string; label: string; icon: LucideIcon; off?: boolean; hint?: string };

/** The task menu (right-click or "..."): role=menu, arrow keys, Enter, Escape. */
function RunMenu({ at, running, onPick, onClose }: { at: { x: number; y: number }; running: boolean; onPick: (what: string) => void; onClose: () => void }) {
  const items: MI[] = [
    { v: "rename", label: T.rename, icon: Pencil }, { v: "folder", label: T.openFolder, icon: FolderOpen }, { v: "copy", label: T.copyTask, icon: Copy },
    ...(running ? [{ v: "stop", label: T.stop, icon: Square }] : []),
    { v: "delete", label: T.delete, icon: Trash2, off: running, hint: running ? T.deleteBusy : undefined },
  ];
  const [hi, setHi] = useState(0);
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const away = (e: MouseEvent) => { if (!root.current?.contains(e.target as Node)) onClose(); };
    document.addEventListener("mousedown", away);
    return () => document.removeEventListener("mousedown", away);
  }, []);
  const pick = (i: number) => { if (!items[i].off) onPick(items[i].v); };
  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "Escape" || e.key === "Tab") { e.preventDefault(); onClose(); }
    else if (e.key === "ArrowDown") { e.preventDefault(); setHi((h) => (h + 1) % items.length); }
    else if (e.key === "ArrowUp") { e.preventDefault(); setHi((h) => (h - 1 + items.length) % items.length); }
    else if (e.key === "Enter" || e.key === " ") { e.preventDefault(); pick(hi); }
  };
  const x = Math.max(8, Math.min(at.x, window.innerWidth - 240)), y = Math.max(8, Math.min(at.y, window.innerHeight - 44 * items.length - 24));
  return (
    <div ref={root} className="menu ctx" role="menu" aria-label={T.label} style={{ left: x, top: y }} onKeyDown={onKey}>
      {items.map((i, n) => (
        <button key={i.v} type="button" role="menuitem" aria-disabled={i.off || undefined} className={`menu-item ${n === hi ? "hi" : ""} ${i.v === "delete" && !i.off ? "danger" : ""}`} tabIndex={-1}
          onMouseEnter={() => setHi(n)} onClick={() => pick(n)} ref={(el) => { if (el && n === hi) el.focus({ preventScroll: true }); }}>
          <i.icon size={16} aria-hidden /><span className="mi-text"><span className="mi-label">{i.label}</span>{i.hint && <span className="mi-hint">{i.hint}</span>}</span>
        </button>
      ))}
    </div>
  );
}

type Scr = { n: number; primary: boolean; w: number; h: number };

/** The PC's screens with a small live picture each, refreshed every 3 s (every second on the one being worked on).
 *  A screen without a picture (none yet, or nothing to show in test mode) isn't listed; with none, the section hides. */
function Screens({ busy }: { busy: number }) {
  const [list, setList] = useState<Scr[] | null>(null);
  const [pics, setPics] = useState<Record<number, string>>({});
  useEffect(() => { invoke<Scr[]>("list_screens").then(setList).catch(() => setList([])); }, []);
  useEffect(() => {
    if (!list?.length) return;
    let on = true, tick = 0;
    const grab = () => {
      if (document.hidden) return;
      for (const s of list) if (tick % 3 === 0 || s.n === busy)
        invoke<string | null>("screen_thumb", { n: s.n }).then((src) => on && setPics((p) => {
          if (src) return { ...p, [s.n]: src };
          const { [s.n]: _gone, ...rest } = p; return rest;
        })).catch(() => {});
      tick++;
    };
    grab();
    const t = setInterval(grab, 1000);
    return () => { on = false; clearInterval(t); };
  }, [list, busy]);
  const shown = (list ?? []).filter((s) => pics[s.n]);
  if (!shown.length) return null;
  return (
    <section className="side-screens" aria-label={T.screens}>
      <h3 className="side-h">{T.screens}</h3>
      <ul>
        {shown.map((s) => (
          <li key={s.n} className="side-screen" title={T.screenTip(s.n)}>
            <span className="side-thumb" style={{ aspectRatio: `${s.w} / ${s.h}` }}><img src={pics[s.n]} alt="" /></span>
            <span className="side-screen-text">
              <span className="side-run-title">{T.screen(s.n)}</span>
              <span className={`side-run-line ${s.n === busy ? "live" : ""}`}>{s.n === busy && <span className="dot live" aria-hidden />}<span>{s.n === busy ? T.screenWorking : T.screenIdle}</span></span>
            </span>
          </li>
        ))}
      </ul>
    </section>
  );
}
