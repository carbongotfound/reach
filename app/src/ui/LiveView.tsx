// The right panel: the screen as the main element (live while working, a replay afterwards), the
// scrubber under it, then every window the agent worked in as a thumbnail with its app's icon.
// The agent's pointer and tag are drawn where they were, in the same design as the real overlay.
import { useState } from "react";
import { Image, Maximize2, Pause, Play } from "lucide-react";
import { describe, type Step } from "../model";
import { AgentCursor } from "./AgentCursor";
import { AppIcon, Crop } from "./AppIcon";
import { IconButton } from "./primitives";
import { S } from "../strings";

const L = S.liveView;

type Box = [number, number, number, number];
/** The part of the screen to show: the agent's window with a margin, between 1.2:1 and 2:1, kept on
 *  the screen. The whole screen when there's no window or it nearly fills the screen. */
export function viewOf(box: Box | undefined, ar: number): Box {
  if (!box || box[2] * box[3] > 0.7) return [0, 0, 1, 1];
  const m = 0.04;
  let [x, y, w, h] = [box[0] - m, box[1] - m * ar, box[2] + 2 * m, box[3] + 2 * m * ar];
  const a = (w * ar) / h;
  if (a < 1.2) { const nw = (1.2 * h) / ar; x -= (nw - w) / 2; w = nw; }
  if (a > 2) { const nh = (w * ar) / 2; y -= (nh - h) / 2; h = nh; }
  w = Math.min(w, 1); h = Math.min(h, 1);
  return [Math.min(Math.max(x, 0), 1 - w), Math.min(Math.max(y, 0), 1 - h), w, h];
}

/** Each window once, with the latest step that has a screenshot of it; newest first. */
function windowsOf(shots: Step[]) {
  const by = new Map<string, Step>();
  for (const s of shots) if (s.win) by.set(`${s.win.h}:${s.win.app}`, s);
  return [...by.values()].reverse();
}

/** The tag's short action for a step, like the overlay's ("Typing"); none for looks and reads. */
const verb = (s: Step) => {
  const a = s.action || "";
  if (s.tool === "keyboard") return a === "key" ? S.pill.pressing : S.pill.typing;
  if (s.tool === "mouse") return a.includes("scroll") ? S.pill.scrolling : a.includes("click") ? S.pill.clicking : "";
  if (s.tool === "ui") return a === "set" || a === "type" ? S.pill.typing : S.pill.clicking;
  return "";
};

export function LiveView({ shots, shown, idx, live, paused, asking, following, playing, names, onSeek, onPlay, onFollow, onZoom, steps, agentName }: {
  shots: Step[]; shown?: Step; idx: number; live: boolean; paused?: boolean; asking?: boolean; following: boolean; playing: boolean; names: Record<string, string>; steps: number; agentName: string;
  onSeek: (i: number) => void; onPlay: () => void; onFollow: () => void; onZoom: (src: string) => void;
}) {
  const atLive = live && following;
  const [ar, setAr] = useState(16 / 9);
  const label = shown ? describe(shown, names) : "";
  // like the real overlay, the pointer is drawn for pointer and keyboard actions only, not looks and reads
  const acts = !!shown && (shown.tool === "mouse" || shown.tool === "keyboard" || (shown.tool === "ui" && !["inspect", "find", "read"].includes(shown.action)));
  const tone = shown && !shown.ok ? "red" : atLive && asking ? "amber" : atLive && paused ? "dim" : "accent";
  const tag = shown && !shown.ok ? S.pill.error : atLive && asking ? S.pill.asking : atLive && paused ? S.pill.paused : (shown && verb(shown)) || agentName;
  const wins = windowsOf(shots);
  const [vx, vy, vw, vh] = viewOf(shown?.win?.box, ar);
  const crop = vw < 1 || vh < 1;
  const px = shown?.ptr && [(shown.ptr[0] - vx) / vw, (shown.ptr[1] - vy) / vh] as const;
  const inView = !!px && px[0] >= 0 && px[0] <= 1 && px[1] >= 0 && px[1] <= 1;
  const newest = shots[shots.length - 1];
  return (
    <section className="screen-panel" aria-label={atLive ? L.live : L.screenshots}>
      <header className="live-head">
        <h2>{L.onScreen}</h2>
        {shown?.win && <span className="live-where">{L.where(shown.win.screen, shown.win.app)}</span>}
      </header>
      <div className="live-stage" style={{ ["--ar" as string]: (ar * vw) / vh }}>
        {shown ? (
          <div className="live-box">
            <button type="button" className="live-shot" onClick={() => onZoom(shown.src!)} aria-label={L.zoom}>
              <img src={shown.src} alt={L.shotAlt(shown.n, label)} className={crop ? "cropped" : undefined}
                style={crop ? { width: `${100 / vw}%`, height: `${100 / vh}%`, left: `${(-vx / vw) * 100}%`, top: `${(-vy / vh) * 100}%` } : undefined} onLoad={(e) => { const i = e.currentTarget; if (i.naturalHeight) setAr(i.naturalWidth / i.naturalHeight); }} />
              <span className="live-zoom" aria-hidden><Maximize2 size={14} /></span>
            </button>
            {shown.ptr && px && inView && acts && <AgentCursor x={px[0]} y={px[1]} k={shown.ptr[2] / vw} label={tag} tone={tone} />}
          </div>
        ) : (
          <div className="live-empty">
            <Image size={20} aria-hidden />
            <p>{live ? L.emptyLive : steps ? L.emptyNoShots : L.emptyNothing}</p>
          </div>
        )}
      </div>
      <div className="live-controls">
        {shots.length > 1 && <IconButton icon={playing ? Pause : Play} label={playing ? L.pause : L.replay} tip="top" size={16} onClick={onPlay} />}
        <p className="live-caption">{shown ? <><span className="live-n">{L.step(shown.n)}</span>{label}</> : " "}</p>
        {shots.length > 0 && <span className="live-count">{L.count(idx + 1, shots.length)}</span>}
        {live && (
          <button type="button" className={`live-pill ${atLive ? "on" : ""}`} onClick={onFollow} disabled={atLive} aria-label={atLive ? L.showingLive : L.backToLive}>
            <span className="dot" aria-hidden />{L.livePill}
          </button>
        )}
      </div>
      {shots.length > 1 && (
        <input type="range" className="scrub" min={0} max={shots.length - 1} step={1} value={Math.max(0, idx)}
          aria-label={L.scrubber} aria-valuetext={shown ? S.steps.label(shown.n, label) : undefined}
          style={{ ["--p" as string]: `${(Math.max(0, idx) / (shots.length - 1)) * 100}%` }}
          onChange={(e) => onSeek(Number(e.target.value))} />
      )}
      <section className="wins" aria-label={L.windows}>
        <h3 className="wins-h">{L.windows}{wins.length > 0 && <span>{wins.length}</span>}</h3>
        {wins.length === 0 ? <p className="wins-empty">{L.windowsEmpty}</p> : (
          <ul className="wins-grid">
            {wins.map((s) => {
              const isLive = live && s === newest;
              const w = s.win!;
              return (
                <li key={`${w.h}:${w.app}`}>
                  <button type="button" className={`win ${wins.length > 1 && shown?.win?.h === w.h ? "sel" : ""}`} onClick={() => onZoom(s.src!)} aria-label={L.windowZoom(w.app)}>
                    <Crop src={s.src!} box={w.box} ar={ar} />
                    <span className="win-name"><AppIcon appKey={w.key} size={16} /><span>{w.app}</span></span>
                    <span className={`win-line ${isLive ? "on" : ""}`}>{isLive && <span className="dot" aria-hidden />}{isLive ? L.windowLive(w.screen) : L.windowAt(s.n, w.screen)}</span>
                  </button>
                </li>
              );
            })}
          </ul>
        )}
      </section>
    </section>
  );
}
