// The agent's pointer and tag inside a screenshot, drawn from the same numbers as the real overlay
// (crates/reach-core/src/overlay.rs: CURSOR, CURSOR_R and the tag geometry G), so the live view shows
// exactly what was on screen. Units: --u is one overlay pixel at the screenshot's scale.
// "you" is the user's own pointer: near-black with a "You" tag, the multiplayer-cursor pattern.

// overlay.rs CURSOR, offset by (1.2, 1.6) like the painter does
const PTS = [[0, 0], [0, 18.5], [4.6, 14.3], [7.6, 21.2], [10.6, 19.9], [7.6, 13.1], [13.6, 13.1]].map(([x, y]) => `${x + 1.2},${y + 1.6}`).join(" ");

export type Tone = "accent" | "amber" | "dim" | "red" | "you";

export function AgentCursor({ x, y, k, label, tone = "accent", done = false }: { x: number; y: number; k: number; label: string; tone?: Tone; done?: boolean }) {
  const body = (outline: boolean) => (
    <svg className={`ac-svg ${outline ? "ac-outline" : "ac-body"}`} viewBox="-4 -4 28 32" aria-hidden>
      <polygon points={PTS} strokeLinejoin="round" strokeWidth={outline ? 4.4 : 1.4} />
    </svg>
  );
  return (
    <span className={`agent-cursor ${tone}`} style={{ left: `${x * 100}%`, top: `${y * 100}%`, ["--k" as string]: k }} aria-hidden>
      {body(true)}
      {label && <span className="ac-tag">{done && <svg className="ac-check" viewBox="0 0 10 8"><path d="M0.5 4 L3.5 7 L9 1" /></svg>}<span className="ac-text">{label}</span></span>}
      {body(false)}
    </span>
  );
}
