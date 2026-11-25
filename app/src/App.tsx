import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Hand, LoaderCircle, PanelRight, Play, Square, TriangleAlert, X } from "lucide-react";
import { describe, empty, fmtDur, idDate, live, reduce, rich, type Brain, type Ev, type Item, type RunMeta, type Step, type View } from "./model";
import { Composer } from "./ui/Composer";
import { Message, StepGroup } from "./ui/Message";
import { ApprovalCard } from "./ui/ApprovalCard";
import { LiveView } from "./ui/LiveView";
import { Sidebar } from "./ui/Sidebar";
import { Settings } from "./ui/Settings";
import { UpdateCard, showUpdate, useUpdate } from "./ui/UpdateCard";
import { Button, IconButton, Menu, Toast } from "./ui/primitives";
import { agentItems, IDEA_APPS, SPEEDS } from "./ui/options";
import { AppIcon } from "./ui/AppIcon";
import { taskTitle } from "./title";
import { activeRun } from "./runstate";
import { S } from "./strings";

export { describe, reduce } from "./model";

const STATE_LABEL: Record<string, string> = { ...S.state, idle: "" };
const NARROW = 860; // below this (never at the window minimum) the screen goes in a tab
const SMALL = 1000; // below this the sidebar starts collapsed

type Screen = "home" | "run" | "settings";
type Block = { k: "steps"; steps: Step[] } | Exclude<Item, { k: "step" }>;

/** Consecutive steps collapse into one group between messages. */
function blocks(items: Item[]): Block[] {
  const out: Block[] = [];
  for (const it of items) {
    if (it.k === "step") {
      const last = out[out.length - 1];
      if (last?.k === "steps") last.steps.push(it); else out.push({ k: "steps", steps: [it] });
    } else out.push(it);
  }
  return out;
}

/** Paint the native title bar to match the page (Windows 11), now and whenever the theme flips. */
function useTitlebar() {
  useEffect(() => {
    const mq = matchMedia("(prefers-color-scheme: dark)");
    const paint = () => {
      const cs = getComputedStyle(document.documentElement);
      invoke("set_titlebar", { bg: cs.getPropertyValue("--titlebar").trim(), fg: cs.getPropertyValue("--text").trim() }).catch(() => {});
    };
    paint();
    mq.addEventListener("change", paint);
    return () => mq.removeEventListener("change", paint);
  }, []);
}

export default function App({ initial = "home" as Screen }: { initial?: Screen }) {
  const [agent, setAgent] = useState<"claude" | "codex">("claude");
  const [brain, setBrain] = useState<Brain>({ state: "checking", text: S.checking });
  const [runs, setRuns] = useState<RunMeta[] | null>(null);
  const [view, setView] = useState<View>(empty);
  const [screen, setScreen] = useState<Screen>(initial);
  const [input, setInput] = useState("");
  const [error, setError] = useState("");
  const [toast, setToast] = useState("");
  const [zoom, setZoom] = useState<string | null>(null);
  const [pick, setPick] = useState<number | null>(null); // selected screenshot step, null = follow latest
  const [playing, setPlaying] = useState(false);
  const [now, setNow] = useState(Date.now());
  const [narrow, setNarrow] = useState(window.innerWidth < NARROW);
  const [tab, setTab] = useState<"steps" | "live">("steps");
  const [speed, setSpeed] = useState("fast");
  const [side, setSide] = useState(() => { try { return localStorage.getItem("side") !== "0"; } catch { return true; } });
  const [small, setSmall] = useState(window.innerWidth < SMALL);
  const [sideSmall, setSideSmall] = useState(false); // sidebar opened by hand while the window is small
  const [panel, setPanel] = useState(true);
  const viewId = useRef("");
  const screenRef = useRef(screen);
  screenRef.current = screen;
  const scroller = useRef<HTMLDivElement>(null);
  const stick = useRef(true);
  // only scrolling up (the reader) unsticks: content that grows between our scroll and its event must not
  const lastTop = useRef(0);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  viewId.current = view.id;
  useTitlebar();
  const update = useUpdate();

  const flash = (t: string) => { setToast(t); setTimeout(() => setToast(""), 3000); };
  const checkSeq = useRef(0); // only the latest check counts: a slow one for the brain switched away from must not win
  const check = (b?: string) => {
    const n = ++checkSeq.current;
    const set = (r: Brain) => { if (n === checkSeq.current) setBrain(r); };
    setBrain({ state: "checking", text: S.checking });
    invoke<Brain>("check_brain", b ? { brain: b } : {}).then(set).catch((e) => set({ state: "missing", text: String(e) }));
  };
  const pickAgent = (b: "claude" | "codex") => {
    if (b === agent) return;
    setAgent(b); invoke("set_brain", { brain: b }).catch((e) => flash(String(e))); check(b);
    // in an open task the next message goes to the new brain: say so once in the thread (a second switch replaces the line)
    if (screenRef.current === "run") setView((v) => {
      if (!v.id) return v;
      const items = v.items.filter((i, n) => !(n === v.items.length - 1 && i.k === "msg" && i.role === "info" && Object.values(S.agents).some((a) => i.text === S.run.switched(a))));
      return { ...v, items: [...items, { k: "msg", role: "info", text: S.run.switched(S.agents[b]) }] };
    });
  };
  const pickSpeed = (v: string) => { setSpeed(v); invoke("set_speed", { speed: v }).catch((e) => flash(String(e))); };
  const refreshRuns = () => invoke<RunMeta[]>("list_runs").then(setRuns).catch(() => setRuns([]));
  const toggleSide = () => {
    if (window.innerWidth < SMALL) { setSideSmall((v) => !v); return; }
    setSide((v) => { try { localStorage.setItem("side", v ? "0" : "1"); } catch { /* private mode */ } return !v; });
  };
  const sideOpen = small ? sideSmall : side;

  useEffect(() => {
    invoke<string>("get_brain").then((b) => b === "codex" && setAgent("codex")).catch(() => {});
    check();
    refreshRuns();
    invoke<string>("get_speed").then((v) => v && setSpeed(v)).catch(() => {});
    const un1 = listen<Ev>("run-event", ({ payload: e }) => {
      if (e.type === "start") { viewId.current = e.id; setPick(null); setScreen("run"); stick.current = true; }
      if (e.type === "start" || e.run === viewId.current) setView((v) => reduce(v, e));
      if (e.type === "start" || e.type === "state" || e.type === "step") refreshRuns();
    });
    const un2 = listen("hotkey-stop", () => flash(S.banners.stoppedByHotkey));
    const onResize = () => { setNarrow(window.innerWidth < NARROW); setSmall(window.innerWidth < SMALL); };
    window.addEventListener("resize", onResize);
    return () => { un1.then((f) => f()); un2.then((f) => f()); window.removeEventListener("resize", onResize); };
  }, []);

  const steps = view.items.filter((i): i is { k: "step" } & Step => i.k === "step");
  const shots = steps.filter((s) => s.src);
  const running = view.state === "running";
  const waiting = view.state === "waiting";
  const isLive = live(view.state);
  const ready = brain.state === "ready";
  const liveRun = activeRun(runs); // a turn is going somewhere: the one answer for the banner, composers and pickers
  const openAsk = isLive ? view.items.find((a): a is Extract<Item, { k: "approval" }> => a.k === "approval" && !a.decision) : undefined;
  const asking = !!openAsk;
  const lastWin = [...steps].reverse().find((s) => s.win)?.win;
  const sideLive = isLive ? { id: view.id, doing: steps.length ? describe(steps[steps.length - 1], view.names) : "", app: lastWin?.app ?? "", asking, screen: lastWin?.screen ?? 0 } : null;
  const working = running && !view.paused && !asking;

  // Follow the bottom while streaming unless the reader scrolled up; no jump when they did.
  useLayoutEffect(() => { const el = scroller.current; if (el && stick.current) el.scrollTop = el.scrollHeight; }, [view.items, screen]);
  // Content also grows after render (screenshots load, a step group opens): keep the bottom pinned then too.
  useEffect(() => {
    const el = scroller.current, col = el?.firstElementChild; if (!el || !col) return;
    const ro = new ResizeObserver(() => { if (stick.current) el.scrollTop = el.scrollHeight; });
    ro.observe(col); return () => ro.disconnect();
  }, [screen, narrow, tab]);
  useEffect(() => { if (!isLive) return; const t = setInterval(() => setNow(Date.now()), 1000); return () => clearInterval(t); }, [isLive]);
  useEffect(() => { if (waiting) inputRef.current?.focus(); }, [waiting]);

  // replay
  const shotIdx = pick === null ? shots.length - 1 : shots.findIndex((s) => s.n === pick);
  const shown = shotIdx >= 0 ? shots[shotIdx] : undefined;
  const go = (d: number) => { setPlaying(false); const i = Math.min(shots.length - 1, Math.max(0, (shotIdx < 0 ? 0 : shotIdx) + d)); if (shots[i]) setPick(shots[i].n); };
  useEffect(() => {
    if (!playing) return;
    if (shotIdx >= shots.length - 1) { setPlaying(false); return; }
    const t = setTimeout(() => { const n = shots[shotIdx + 1]; if (n) setPick(n.n); }, 700);
    return () => clearTimeout(t);
  }, [playing, shotIdx, shots.length]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const typing = (e.target as HTMLElement)?.closest("textarea, input, [role=menu]");
      if (e.key === "Escape" && zoom) { setZoom(null); return; }
      if (e.ctrlKey && !e.altKey && e.key.toLowerCase() === "n") { e.preventDefault(); newTask(); return; }
      if (e.ctrlKey && !e.altKey && e.key.toLowerCase() === "b") { e.preventDefault(); toggleSide(); return; }
      if (e.ctrlKey && e.key === ",") { e.preventDefault(); setScreen("settings"); return; }
      if (typing || screen !== "run" || zoom) return;
      if (e.key === "ArrowLeft") { e.preventDefault(); go(-1); }
      if (e.key === "ArrowRight") { e.preventDefault(); go(1); }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  async function submit() {
    const text = input.trim();
    if (!text) return;
    setError("");
    try {
      if (waiting && screen === "run") await invoke("send_message", { text });
      else await invoke("start_task", { text });
      setInput("");
    } catch (e) { setError(String(e)); }
  }
  async function stop() { try { await invoke("stop_task"); flash(S.banners.stopped); } catch (e) { flash(String(e)); } }
  async function takeOver(on: boolean) {
    try { await invoke("take_over", { on }); setView((v) => ({ ...v, held: on, paused: on })); } catch (e) { flash(String(e)); }
  }
  async function open(id: string) {
    try {
      const evs = await invoke<Ev[]>("load_run", { id });
      let v = empty;
      for (const e of evs) v = reduce(v, e);
      const meta = runs?.find((r) => r.id === id);
      setView({ ...v, id, task: v.task || meta?.task || "", state: meta?.state ?? v.state });
      setPick(null); setPlaying(false); setTab("steps"); setError(""); setScreen("run"); stick.current = true;
    } catch (e) { flash(S.errors.openRun(e)); }
  }
  function newTask() { if (!isLive) setView(empty); setError(""); setPick(null); setPlaying(false); setScreen("home"); setTimeout(() => inputRef.current?.focus(), 0); }

  const controls = (
    <>
      <Menu label={S.controls.agent} value={agent} disabled={!!liveRun} title={liveRun ? S.controls.agentBusy : undefined}
        lead={brain.state === "checking" ? <LoaderCircle size={14} className="spin" aria-hidden /> : <span className={`dot ${ready ? "ok" : "bad"}`} aria-hidden />}
        items={agentItems()} onPick={(v) => pickAgent(v as "claude" | "codex")} />
      <Menu label={S.controls.speed} value={speed} disabled={!!liveRun} title={liveRun ? S.controls.agentBusy : undefined} items={SPEEDS.map(([value, label, hint]) => ({ value, label, hint }))} onPick={pickSpeed} />
    </>
  );

  const errorNote = error && <div className="notice error" role="alert"><TriangleAlert size={16} aria-hidden /><span>{rich(error)}</span><IconButton icon={X} label={S.banners.dismiss} tip="none" size={16} onClick={() => setError("")} /></div>;

  const started = idDate(view.id)?.getTime() ?? now;
  const stateText = asking ? S.run.needsApproval : view.held ? S.run.held : view.paused ? S.run.paused : running ? S.run.working(fmtDur(now - started)) : STATE_LABEL[view.state] ?? view.state;
  const bl = blocks(view.items);
  const lastSteps = bl.map((b) => b.k).lastIndexOf("steps");

  const home = (
    <main className="home">
      <div className="home-col">
        <h1 className="greeting">{S.home.greeting.join("")}</h1>
        {brain.state !== "ready" && brain.state !== "checking" && (
          <div className="notice warn" role="alert"><TriangleAlert size={16} aria-hidden /><span>{rich(brain.text)}</span><Button size="sm" onClick={() => check()}>{S.retry}</Button></div>
        )}
        {liveRun && (
          <div className="notice info"><LoaderCircle size={16} className="spin" aria-hidden /><span>{S.home.stillRunning(liveRun.title || taskTitle(liveRun.task))}</span><Button size="sm" onClick={() => open(liveRun.id)}>{S.open}</Button></div>
        )}
        {errorNote}
        {update && showUpdate(update) && !liveRun && <UpdateCard u={update} busy={false} onError={flash} />}
        <Composer value={input} onChange={setInput} onSubmit={submit} inputRef={inputRef} label={S.home.composerLabel}
          placeholder={S.home.placeholder} disabled={brain.state === "checking" ? false : !ready}
          canSend={ready && !!input.trim() && !liveRun} sendTitle={liveRun ? S.home.otherRunning : !ready ? brain.text : S.home.start} controls={controls} />
        <div className="ideas" role="list" aria-label={S.home.suggestions}>
          {S.home.ideas.map(([label, t], i) => (
            <button key={t} type="button" role="listitem" className="idea" title={t} onClick={() => { setInput(t); inputRef.current?.focus(); }}><AppIcon appKey={IDEA_APPS[i]} size={18} />{label}</button>
          ))}
        </div>
      </div>
      <p className="home-foot">{S.home.stopAnytime}<kbd>{S.hotkey}</kbd></p>
    </main>
  );

  const conversation = (
    <div className="convo">
      <div className="convo-scroll" ref={scroller} onScroll={(e) => { const el = e.currentTarget; if (el.scrollHeight - el.scrollTop - el.clientHeight < 48) stick.current = true; else if (el.scrollTop < lastTop.current) stick.current = false; lastTop.current = el.scrollTop; }}>
        <div className="convo-col" aria-live="polite">
          {bl.map((b, i) => b.k === "steps" ? (
            <StepGroup key={`s${b.steps[0].n}`} steps={b.steps} names={view.names} live={isLive} last={i === lastSteps} working={working}
              selected={pick} onPick={(s) => { setPlaying(false); setPick(s.n); setPanel(true); if (narrow) setTab("live"); }} />
          ) : b.k === "approval" ? (
            isLive && !b.decision ? null : <ApprovalCard key={`a${b.id}`} a={b} agent={agent} live={isLive} now={now} onZoom={setZoom} onError={flash} />
          ) : (
            <Message key={i} role={b.role} text={b.text} />
          ))}
          {running && steps.length === 0 && <div className="steps"><div className="steps-head static"><LoaderCircle size={14} className="spin" aria-hidden /><span className="shimmer">{S.run.gettingStarted}</span></div></div>}
          {!isLive && view.state !== "idle" && <p className="convo-end">{S.run.end(STATE_LABEL[view.state] ?? view.state, steps.length)}</p>}
        </div>
      </div>
      <div className="convo-bottom">
        {errorNote}
        {openAsk && <ApprovalCard key={`a${openAsk.id}`} a={openAsk} agent={agent} live={isLive} now={now} onZoom={setZoom} onError={flash} region={lastWin?.box} />}
        <Composer value={input} onChange={setInput} onSubmit={submit} onStop={stop} inputRef={inputRef} running={running}
          label={waiting ? S.composer.replyLabel : S.composer.newLabel}
          placeholder={waiting ? S.composer.replyPlaceholder : isLive && view.held ? S.composer.heldPlaceholder : running && view.paused ? S.composer.pausedPlaceholder : running ? S.composer.workingPlaceholder : S.composer.newPlaceholder}
          disabled={!ready || running} canSend={ready && !!input.trim() && (waiting || !liveRun)}
          sendTitle={waiting ? S.composer.reply : S.composer.startNew} controls={controls} />
      </div>
    </div>
  );

  const runScreen = (
    <main className="run">
      <header className="run-head">
        <div className="run-title">
          <h2 title={view.task}>{runs?.find((r) => r.id === view.id)?.title || taskTitle(view.task)}</h2>
          <span className={`state ${asking ? "asking" : view.paused ? "paused" : view.state}`}>{working && <span className="pulse" aria-hidden />}{stateText}</span>
        </div>
        {narrow && (
          <div className="seg" role="tablist" aria-label={S.run.show}>
            <button role="tab" aria-selected={tab === "steps"} onClick={() => setTab("steps")}>{S.run.tabSteps}</button>
            <button role="tab" aria-selected={tab === "live"} onClick={() => setTab("live")}>{S.run.tabScreen}</button>
          </div>
        )}
        {running && view.paused && !view.held && <Button icon={Play} onClick={() => invoke("resume")}>{S.run.resume}</Button>}
        {running && (view.held
          ? <Button icon={Play} onClick={() => takeOver(false)}>{S.run.giveBack}</Button>
          : <Button icon={Hand} onClick={() => takeOver(true)} title={S.run.takeOverTip}>{S.run.takeOver}</Button>)}
        {running && <button type="button" className="btn danger lg" onClick={stop} title={S.run.stopTip}><Square size={14} fill="currentColor" aria-hidden />{S.run.stop}</button>}
        {!narrow && <IconButton icon={PanelRight} label={panel ? S.run.hideScreen : S.run.showScreen} onClick={() => setPanel(!panel)} aria-pressed={panel} />}
      </header>
      <div className={`run-body ${!narrow && panel ? "split" : ""}`}>
        {(!narrow || tab === "steps") && conversation}
        {((narrow && tab === "live") || (!narrow && panel)) && (
          <LiveView agentName={S.agents[agent]} shots={shots} shown={shown} idx={shotIdx} live={running} paused={view.paused} asking={!!openAsk} following={pick === null} playing={playing} names={view.names} steps={steps.length}
            onSeek={(i) => { setPlaying(false); if (shots[i]) setPick(isLive && i === shots.length - 1 ? null : shots[i].n); }} onFollow={() => { setPick(null); setPlaying(false); }} onZoom={setZoom}
            onPlay={() => { if (!playing && shotIdx >= shots.length - 1) setPick(shots[0].n); setPlaying(!playing); }} />
        )}
      </div>
    </main>
  );

  return (
    <div className={`app ${sideOpen ? "" : "side-closed"}`}>
      <Sidebar runs={runs} current={screen === "run" ? view.id : ""} live={sideLive} collapsed={!sideOpen} onToggle={toggleSide} onNew={newTask} onOpen={open}
        onStop={stop} onRefresh={refreshRuns} onToast={flash} onDeleted={(id) => { refreshRuns(); if (id === view.id) { setView(empty); setScreen((s) => (s === "run" ? "home" : s)); } }}
        onSettings={() => setScreen("settings")} settingsOn={screen === "settings"} updateDot={showUpdate(update)} />
      {screen === "settings" ? <Settings agent={agent} brain={brain} busy={!!liveRun} update={update} onAgent={pickAgent} onSpeed={pickSpeed} onError={flash} />
        : screen === "run" && view.id ? runScreen : home}
      {zoom && (
        <div className="zoom" role="dialog" aria-modal="true" aria-label={S.zoom.label} onClick={() => setZoom(null)}>
          <img src={zoom} alt="" />
          <IconButton className="zoom-close" icon={X} label={S.zoom.close} tip="none" onClick={() => setZoom(null)} autoFocus />
        </div>
      )}
      {toast && <Toast text={toast} />}
    </div>
  );
}
