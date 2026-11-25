// Settings: one column of grouped rows, each a label, a one-line explanation and a control.
import { useEffect, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { CircleAlert, FolderOpen, LoaderCircle, RefreshCw } from "lucide-react";
import { Button, Menu, Switch } from "./primitives";
import { SPEEDS, agentItems } from "./options";
import type { Brain } from "../model";
import { UpdateCard, fmtDate, type Update } from "./UpdateCard";
import { S } from "../strings";

const T = S.settings;
type Conf = { speed: string; pauseOnUser: boolean; pauseIdleMs: number; agentCursor: boolean; cursorRipple: boolean; maxDim: number; theme: string; home: string; version: string; updateAuto: boolean; updateBeta: boolean };

function Row({ title, hint, children }: { title: string; hint: string; children: ReactNode }) {
  return (
    <div className="set-row">
      <div className="set-text"><span className="set-title">{title}</span><span className="set-hint">{hint}</span></div>
      <div className="set-ctl">{children}</div>
    </div>
  );
}

export function Settings({ agent, brain, busy, update, onAgent, onSpeed, onError }: {
  agent: string; brain: Brain; busy: boolean; update: Update | null; onAgent: (v: "claude" | "codex") => void; onSpeed: (v: string) => void; onError: (t: string) => void;
}) {
  const [s, setS] = useState<Conf | null>(null);
  const [failed, setFailed] = useState("");
  const load = () => { setFailed(""); invoke<Conf>("get_settings").then(setS).catch((e) => setFailed(String(e))); };
  useEffect(load, []);
  const set = (key: keyof Conf, value: unknown) => {
    if (!s) return;
    const before = s;
    setS({ ...s, [key]: value });
    invoke("set_setting", { key, value }).catch((e) => { setS(before); onError(String(e)); });
  };
  return (
    <main className="page settings" aria-labelledby="set-h">
      <div className="page-col">
        <h1 id="set-h">{T.title}</h1>
        {failed ? (
          <div className="notice error" role="alert"><CircleAlert size={16} aria-hidden /><span>{T.loadFailed(failed)}</span><Button size="sm" onClick={load}>{S.retry}</Button></div>
        ) : !s ? (
          <div className="page-loading"><LoaderCircle size={16} className="spin" aria-hidden />{T.loading}</div>
        ) : (
          <>
            <section className="set-group" aria-labelledby="g0"><h2 id="g0">{T.groupAppearance}</h2>
              <Row title={T.theme} hint={T.themeHint}>
                <Menu label={T.theme} value={s.theme} up={false} items={(["system", "light", "dark"] as const).map((value) => ({ value, label: T.themes[value] }))} onPick={(v) => set("theme", v)} />
              </Row>

            </section>
            <section className="set-group" aria-labelledby="g1"><h2 id="g1">{T.groupAgent}</h2>
              <Row title={T.model} hint={brain.state === "ready" ? T.modelHint : brain.text}>
                <Menu label={S.controls.agent} value={agent} items={agentItems()} up={false} disabled={busy} onPick={(v) => onAgent(v as "claude" | "codex")} />
              </Row>
              <Row title={T.speed} hint={T.speedHint}>
                <Menu label={S.controls.speed} value={s.speed} up={false} items={SPEEDS.map(([value, label, hint]) => ({ value, label, hint }))} onPick={(v) => { setS({ ...s, speed: v }); onSpeed(v); }} />
              </Row>
            </section>
            <section className="set-group" aria-labelledby="g2"><h2 id="g2">{T.groupWhileWorking}</h2>
              <Row title={T.pauseOnUser} hint={T.pauseOnUserHint}>
                <Switch label={T.pauseOnUser} on={s.pauseOnUser} onChange={(v) => set("pauseOnUser", v)} />
              </Row>
              <Row title={T.idleWait} hint={T.idleWaitHint}>
                <Menu label={T.idleWait} value={String(s.pauseIdleMs)} up={false} disabled={!s.pauseOnUser}
                  items={[1000, 2000, 3000, 5000].map((v) => ({ value: String(v), label: T.seconds(v / 1000) }))} onPick={(v) => set("pauseIdleMs", Number(v))} />
              </Row>
              <Row title={T.agentCursor} hint={T.agentCursorHint}>
                <Switch label={T.agentCursor} on={s.agentCursor} onChange={(v) => set("agentCursor", v)} />
              </Row>
              <Row title={T.ripple} hint={T.rippleHint}>
                <Switch label={T.ripple} on={s.cursorRipple} onChange={(v) => set("cursorRipple", v)} />
              </Row>
            </section>
            <section className="set-group" aria-labelledby="g3"><h2 id="g3">{T.groupScreenshots}</h2>
              <Row title={T.shotSize} hint={T.shotSizeHint}>
                <Menu label={T.shotSize} value={String(s.maxDim)} up={false} items={[1024, 1280, 1600, 1920].map((v) => ({ value: String(v), label: T.px(v) }))} onPick={(v) => set("maxDim", Number(v))} />
              </Row>
              <Row title={T.history} hint={s.home}>
                <Button size="sm" icon={FolderOpen} onClick={() => invoke("open_home").catch((e) => onError(String(e)))}>{T.openFolder}</Button>
              </Row>
            </section>
            <section className="set-group" aria-labelledby="g4"><h2 id="g4">{T.groupSafety}</h2>
              <Row title={T.hotkey} hint={T.hotkeyHint}><kbd>{S.hotkey}</kbd></Row>
              <Row title={T.approvals} hint={T.approvalsHint}><span className="set-fixed">{T.on}</span></Row>
            </section>
            <section className="set-group" aria-labelledby="g5"><h2 id="g5">{T.groupUpdates}</h2>
              {update?.latest && update.phase !== "none" && <UpdateCard u={update} busy={busy} onError={onError} />}
              <Row title={T.installedVersion} hint={update?.note || (update?.lastCheck ? T.lastChecked(fmtDate(new Date(update.lastCheck * 1000).toISOString(), true)) : T.neverChecked)}>
                <span className="set-fixed">{s.version}</span>
                <Button size="sm" icon={update?.phase === "checking" ? LoaderCircle : RefreshCw} className={update?.phase === "checking" ? "checking" : ""}
                  disabled={update?.phase === "checking"} onClick={() => invoke("update_check").catch((e) => onError(String(e)))}>{T.checkNow}</Button>
              </Row>
              <Row title={T.autoCheck} hint={T.autoCheckHint}>
                <Switch label={T.autoCheck} on={s.updateAuto} onChange={(v) => set("updateAuto", v)} />
              </Row>
              <Row title={T.beta} hint={T.betaHint}>
                <Switch label={T.beta} on={s.updateBeta} onChange={(v) => set("updateBeta", v)} />
              </Row>
            </section>
            <p className="set-version">{T.version(s.version)}</p>
            <p className="set-version">{S.trademarks}</p>
          </>
        )}
      </div>
    </main>
  );
}
