// Run model: event reducer, step descriptions and formatting. No UI here.
import { S } from "./strings";

export type Brain = { state: "checking" | "ready" | "missing" | "signed_out"; text: string };
export type Role = "user" | "agent" | "info" | "warn" | "error";
/** The window a step worked in: `key` finds the app's icon, `box` is its place in the step's screenshot (fractions). */
export type Win = { h: number; key: string; app: string; screen: number; box: [number, number, number, number] };
export type Step = { n: number; tool: string; action: string; args: Record<string, unknown>; ok: boolean; text: string; ms: number; src?: string; ptr?: [number, number, number]; win?: Win };
export type Approval = { id: number; by: "agent" | "engine"; action: string; where: string; content: string; why: string; undo: string; wait: number; t: number; src?: string; box?: [number, number, number, number]; decision?: string; text?: string };
export type Item = { k: "msg"; role: Role; text: string } | ({ k: "step" } & Step) | ({ k: "approval" } & Approval);
export type RunMeta = { id: string; task: string; title?: string; state: string; steps: number; started: string; ended?: string; app?: string; appKey?: string };
export type View = { id: string; task: string; state: string; items: Item[]; paused: boolean; held: boolean; names: Record<string, string> };
export type Ev = Record<string, any> & { type: string; run?: string };

export const empty: View = { id: "", task: "", state: "idle", items: [], paused: false, held: false, names: {} };
/** Control names from ui inspect/find output (`e2 button "Save"`), so steps can say "Pressed Save" instead of "Pressed e2". */
export const NAME_RE = /\b(e\d+) ([a-z]+) "([^"]+)"/g;
export const live = (s: string) => s === "running" || s === "waiting";
export const msg = (role: Role, text: string): Item => ({ k: "msg", role, text });

export function reduce(v: View, e: Ev): View {
  switch (e.type) {
    case "start": return { ...empty, id: e.id, task: e.task, state: "running", items: [msg("user", e.task)] };
    case "user": return { ...v, items: [...v.items, msg("user", e.text)] };
    case "text": return { ...v, items: [...v.items, msg("agent", e.text)] };
    case "status": return { ...v, items: [...v.items, msg(e.level === "error" ? "error" : e.level === "warn" ? "warn" : "info", e.text)] };
    case "state": return { ...v, state: e.state, paused: live(e.state) && v.paused, held: live(e.state) && v.held };
    case "pause": {
      if (!e.paused || !e.text) return { ...v, paused: e.paused, held: e.paused ? !!e.held : false };
      // the same notice again (another pause): one line that moves to the latest place, not a pile of copies
      const last = v.items.map((i) => i.k).lastIndexOf("msg");
      const prev = v.items[last];
      const items = prev?.k === "msg" && prev.text === e.text ? v.items.filter((_, n) => n !== last) : v.items;
      return { ...v, paused: true, held: !!e.held, items: [...items, msg("warn", e.text)] };
    }
    case "step": {
      const names = { ...v.names };
      if (e.tool === "ui" && typeof e.text === "string") for (const m of e.text.matchAll(NAME_RE)) names[m[1]] = S.steps.control(clip(m[3], 40), m[2]);
      return { ...v, names, items: [...v.items, { k: "step", ...(e as unknown as Step) }] };
    }
    case "approval": return { ...v, items: [...v.items, { k: "approval", ...(e as unknown as Approval) }] };
    case "approval-answer": return { ...v, items: v.items.map((a) => (a.k === "approval" && a.id === e.id ? { ...a, decision: e.decision, text: e.text } : a)) };
    case "thumb": return { ...v, items: v.items.map((s) => (s.k === "step" && s.n === e.n ? { ...s, src: e.src, ptr: e.ptr, win: e.win } : s)) };
    default: return v;
  }
}

export const clip = (s: string, n = 60) => (s.length > n ? s.slice(0, n - 1) + "…" : s);
export const q = (v: unknown) => `“${clip(String(v))}”`;
export const cap = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);
export const keys = (k: string) => k.split("+").map((p) => (p.length === 1 || /^f\d+$/i.test(p) ? p.toUpperCase() : cap(p))).join(" + ");

/** One plain sentence per action, for someone glancing at it. */
export function describe(s: Step, names: Record<string, string> = {}): string {
  const a = s.args;
  // never raw coordinates: the app the step worked in, when known
  const at = s.win?.app ? S.steps.in(s.win.app) : "";
  switch (s.tool) {
    case "screenshot": return a.region ? S.steps.zoomed : a.window ? S.steps.lookedAtWindow : S.steps.lookedAtScreen;
    case "apps":
      if (s.action === "open") return S.steps.opened(String(a.name ?? S.steps.anApp));
      if (s.action === "list") return a.filter ? S.steps.lookedFor(q(a.filter)) : S.steps.listedWindows;
      if (s.action === "displays") return S.steps.listedScreens;
      return `${cap(s.action)} ${a.name ?? S.steps.theWindow}`;
    case "ui": {
      const ref = a.ref ? names[String(a.ref)] ?? String(a.ref) : S.steps.aControl;
      if (s.action === "find") return S.steps.found(q(a.query ?? ""));
      if (s.action === "inspect") return S.steps.readControls;
      if (s.action === "invoke") return S.steps.pressed(ref);
      if (s.action === "set") return S.steps.entered(q(a.value ?? ""));
      if (s.action === "read") return S.steps.read(ref);
      if (s.action === "toggle") return S.steps.toggled(ref);
      if (s.action === "focus") return S.steps.focused(ref);
      return `${cap(s.action)} ${ref}`;
    }
    case "mouse":
      if (s.action === "scroll") return S.steps.scrolled(String(a.direction ?? S.steps.down), at);
      if (s.action === "drag") return S.steps.dragged(at);
      if (s.action === "position") return S.steps.checkedPointer;
      if (s.action === "wait") return S.steps.waited(a.duration ?? 1);
      return `${cap(s.action.replace("_", " "))}${at}`;
    case "keyboard":
      if (s.action === "type") return S.steps.typed(q(a.text ?? ""));
      return S.steps.pressed(a.text ? keys(String(a.text)) : S.steps.keys);
    default: return `${cap(s.tool)} ${s.action}`;
  }
}
export const KIND = S.steps.kind;

/** `code` spans in app copy. */
export const rich = (t: string) => t.split(/(`[^`]+`)/).map((p, i) => (p.startsWith("`") ? <code key={i}>{p.slice(1, -1)}</code> : p));
const inline = (t: string) => t.split(/(\*\*[^*]+\*\*|`[^`]+`)/).map((p, j) =>
  p.startsWith("**") ? <b key={j}>{p.slice(2, -2)}</b> : p.startsWith("`") ? <code key={j}>{p.slice(1, -1)}</code> : p);
const ROW = /^\s*\|.*\|\s*$/;
const cells = (l: string) => l.trim().slice(1, -1).split("|").map((c) => c.trim());

/** The little markdown agents use: ``` blocks, **bold**, `code`, | pipe | tables |. Everything else stays plain text. */
export function md(t: string) {
  return t.split(/```[a-z]*\n?/).map((part, i) => {
    if (i % 2) return <pre key={i}>{part.replace(/\n$/, "")}</pre>;
    // a run of pipe rows whose second line is a |---| rule becomes a table
    const out: React.ReactNode[] = [];
    const lines = part.split("\n");
    let text: string[] = [];
    const flush = () => { if (text.length) out.push(<span key={out.length}>{inline(text.join("\n"))}</span>); text = []; };
    for (let k = 0; k < lines.length; k++) {
      let e = k;
      while (e < lines.length && ROW.test(lines[e])) e++;
      if (e - k >= 2 && /^[\s|:-]+$/.test(lines[k + 1])) {
        flush();
        const [head, , ...body] = lines.slice(k, e).map(cells);
        out.push(<table key={out.length} className="md-table"><thead><tr>{head.map((c, j) => <th key={j}>{inline(c)}</th>)}</tr></thead>
          <tbody>{body.map((r, n) => <tr key={n}>{r.map((c, j) => <td key={j}>{inline(c)}</td>)}</tr>)}</tbody></table>);
        k = e - 1;
      } else text.push(lines[k]);
    }
    flush();
    return <span key={i}>{out}</span>;
  });
}

/** id = yyyyMMdd-HHmmss */
export const idDate = (id: string) => {
  const m = id.match(/^(\d{4})(\d{2})(\d{2})-(\d{2})(\d{2})(\d{2})/);
  return m ? new Date(+m[1], +m[2] - 1, +m[3], +m[4], +m[5], +m[6]) : null;
};
export function fmtWhen(id: string) {
  const d = idDate(id);
  if (!d) return id;
  const hm = d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  const today = new Date().toDateString() === d.toDateString();
  return today ? hm : `${d.toLocaleDateString([], { month: "short", day: "numeric" })}, ${hm}`;
}
export const fmtDur = (ms: number) => { const s = Math.max(0, Math.floor(ms / 1000)); return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`; };

/** Short "when" for status lines: 3:14 PM today, then Yesterday, a weekday this week, else Nov 12. */
export function fmtDay(d: Date | null, yesterday: string) {
  if (!d) return "";
  const days = Math.round((new Date().setHours(0, 0, 0, 0) - new Date(d).setHours(0, 0, 0, 0)) / 864e5);
  if (days <= 0) return d.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  if (days === 1) return yesterday;
  if (days < 7) return d.toLocaleDateString([], { weekday: "short" });
  return d.toLocaleDateString([], { month: "short", day: "numeric" });
}
