// Design preview: the real App with a mocked backend, one scenario per ?s=. Used for tests/ui screenshots.
// Not part of the app build (vite builds index.html only).
import { createRoot } from "react-dom/client";
import { AgentCursor, type Tone } from "./ui/AgentCursor";
import { mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import App from "./App";
import "@fontsource-variable/inter";
import "./styles.css";

const s = new URLSearchParams(location.search).get("s") ?? "start";
// started 75 s ago, so the header timer reads like a real run
const ID = ((d) => `${d.getFullYear()}${String(d.getMonth() + 1).padStart(2, "0")}${String(d.getDate()).padStart(2, "0")}-${[d.getHours(), d.getMinutes(), d.getSeconds()].map((n) => String(n).padStart(2, "0")).join("")}`)(new Date(Date.now() - 75000));
const TASK = s === "long" ? "Open Excel (or Notepad if Excel is not installed), and enter these three invoices into a table: Acme 1200, Northwind 850, Contoso 430. Then add a total row underneath, make the header bold, widen the columns so every value fits, and save the file to the Desktop as invoices-november-2026-final.xlsx" : "Open Notepad and write hello, then save it to the Desktop as hello";
const day = (n: number, h: number, m: number) => { const d = new Date(); d.setDate(d.getDate() - n); d.setHours(h, m, 4, 0); return d; };
const idOf = (d: Date) => `${d.getFullYear()}${String(d.getMonth() + 1).padStart(2, "0")}${String(d.getDate()).padStart(2, "0")}-${[d.getHours(), d.getMinutes(), d.getSeconds()].map((n) => String(n).padStart(2, "0")).join("")}`;
const past = (n: number, h: number, m: number, task: string, state: string, steps: number, app = "") => ({ id: idOf(day(n, h, m)), task, state, steps, started: "", ended: day(n, h, m + 3).toISOString(), app });
const runs = [
  { id: ID, task: TASK, state: s === "done" ? "done" : s === "failed" ? "failed" : s.startsWith("start") || s.startsWith("settings") || s === "long" || s.endsWith("signed-out") ? "done" : s === "waiting" ? "waiting" : "running", steps: 6, started: "" },
  // a clean, realistic history for README and gallery shots
  past(1, 16, 20, "Open Excel and enter these three invoices into a table: Acme 1200, Northwind 850, Contoso 430. Then save it to the Desktop.", "done", 18, "Excel"),
  past(1, 11, 2, "Write a shopping list in Notepad", "done", 7, "Notepad"),
  past(3, 18, 2, "Rename my trip photos by the date they were taken", "stopped", 9, "File Explorer"),
  past(4, 17, 15, "Resize every image in the Screenshots folder to 1280 px wide", "failed", 4, "Photos"),
  past(6, 9, 40, "Work out 15% of 2480 in Calculator", "done", 5, "Calculator"),
];
const brain = s === "signed-out"
  ? { state: "signed_out", text: "Claude isn't signed in on this PC. Open a terminal, run `claude`, sign in, then press Retry." }
  : s === "codex-signed-out"
  ? { state: "signed_out", text: "ChatGPT isn't signed in on this PC. Open a terminal, run `codex login`, sign in, then press Retry." }
  : { state: "ready", text: "Claude is signed in." };
const never = new Promise(() => {});

// update card scenarios: start-update, start-update-downloading, -ready, -error, -portable, settings-update, settings-uptodate
const NOTES = "Redesigned pointer label and a new logo.\nReach now tells you when a new version is out, and installs it when you press Install.\nFixes: the approval card no longer hides under the composer.";
const upd = (() => {
  const base = { phase: "available", installed: "0.3.0", hidden: false, got: 0, total: 9_840_000, note: "", error: "", portable: false, auto: true, beta: false, lastCheck: Math.floor(Date.now() / 1000) - 1800,
    latest: { version: "0.4.0", date: "2026-12-02T09:00:00Z", notes: NOTES, page: "https://github.com/carbongotfound/reach/releases/tag/v0.4.0", pre: false } as { version: string; date: string; notes: string; page: string; pre: boolean } | null };
  if (s.endsWith("uptodate")) return { ...base, phase: "none", latest: null, note: "You have the latest version, 0.3.0." };
  if (!s.includes("update")) return { ...base, phase: "none", latest: null };
  if (s.endsWith("downloading")) return { ...base, phase: "downloading", got: 4_120_000 };
  if (s.endsWith("ready")) return { ...base, phase: "ready", got: 9_840_000 };
  if (s.endsWith("error")) return { ...base, phase: "error", error: "The download didn't match its checksum, so I deleted it and didn't install it. Your current version still works." };
  if (s.endsWith("portable")) return { ...base, portable: true };
  return base;
})();

mockIPC((cmd, args) => {
  if (cmd === "check_brain") return s === "start-loading" ? never : brain;
  if (cmd === "list_runs") return s === "start-loading" ? never : s === "start-empty" ? [] : runs;
  if (cmd === "load_run") return [];
  if (cmd === "update_state" || cmd === "update_check") return upd;
  if (cmd === "get_speed") return "fast";
  if (cmd === "app_icon") return null;
  if (cmd === "list_screens") return [{ n: 1, primary: true, w: 2560, h: 1440, scale: 1.5 }, { n: 2, primary: false, w: 1920, h: 1080, scale: 1 }];
  if (cmd === "screen_thumb") return `/preview-assets/${(args as { n: number }).n === 2 && !s.startsWith("start") ? "desk-pad" : "desk"}.jpg`;
  if (cmd === "get_brain") return s.startsWith("codex") ? "codex" : "claude";
  if (cmd === "get_settings") return s === "settings-error" ? Promise.reject("The settings file couldn't be read.") : s === "settings-loading" ? never : { speed: "fast", pauseOnUser: true, pauseIdleMs: 2000, agentCursor: true, cursorRipple: true, maxDim: 1280, theme: "system", home: "C:\\Users\\you\\AppData\\Local\\Reach", version: "0.3.0", updateAuto: true, updateBeta: false };
  return null;
}, { shouldMockEvents: true });

const step = (n: number, tool: string, action: string, args: Record<string, unknown>, ms: number, ok = true, text = "") =>
  ({ type: "step", run: ID, n, tool, action, args, ok, text, ms });
const PTR: Record<string, [number, number, number]> = { "desk.jpg": [0.5, 0.5, 1 / 1280], "desk-pad.jpg": [0.31, 0.36, 1 / 1280] };
// the Test Pad window (a stand-in for Notepad) on a fenced test screen, as the real app shows it
const WIN = { h: 1, key: "", app: "Test Pad", screen: 2, box: [0.2578, 0.2375, 0.4891, 0.5163] };
const thumb = (n: number, src: string) => ({ type: "thumb", run: ID, n, src: `/preview-assets/${src}`, ptr: PTR[src], ...(src === "desk-pad.jpg" ? { win: WIN } : {}) });

const script: Record<string, unknown>[] = [
  { type: "start", id: ID, task: TASK },
  step(1, "apps", "open", { name: "Notepad" }, 812), thumb(1, "desk-pad.jpg"),
  { ...step(2, "ui", "inspect", { window: "0x65088A" }, 51), text: 'e1 edit "Text editor" 800,131 e4 menuitem "File" 704,67 e7 button "Save" 865,59' },
  step(3, "ui", "set", { ref: "e1", value: "hello" }, 27), thumb(3, "desk-pad.jpg"),
];
if (s === "failed") {
  script.push(step(4, "keyboard", "key", { text: "ctrl+s" }, 640, false, "That app is running as administrator, so I can't control it. Close it and reopen it normally, or run me as administrator."));
  script.push({ type: "status", run: ID, level: "error", text: "That app is running as administrator, so I can't control it. Close it and reopen it normally, or run me as administrator." });
  script.push({ type: "state", run: ID, state: "failed" });
} else if (s !== "running-early") {
  script.push(step(4, "keyboard", "key", { text: "ctrl+s" }, 233), thumb(4, "desk-pad.jpg"));
  script.push(step(5, "screenshot", "", { window: "0x65088A" }, 14), thumb(5, "desk-pad.jpg"));
  if (s === "waiting") {
    script.push({ type: "text", run: ID, text: "The Save dialog is open. A file named **hello.txt** already exists on the Desktop. Should I replace it, or save as `hello (2).txt`?" });
    script.push({ type: "state", run: ID, state: "waiting" });
  }
  if (s === "done") {
    script.push(step(6, "ui", "invoke", { ref: "e7" }, 31), thumb(6, "desk-pad.jpg"));
    script.push({ type: "text", run: ID, text: "I wrote \"hello\" in Notepad and saved it to the Desktop as hello.txt. I checked the title bar: it now shows hello.txt, so the save worked." });
    script.push({ type: "state", run: ID, state: "done" });
  }
  if (s === "long") {
    script.push({ type: "text", run: ID, text: "I entered the three invoices and added a total row. Here is what the sheet holds now:\n| Customer | Amount |\n|---|---:|\n| Acme | 1200 |\n| Northwind | 850 |\n| Contoso | 430 |\n| **Total** | **2480** |\nThe header is bold and the columns are wide enough for every value. A file named **invoices-november-2026-final.xlsx** already existed on the Desktop, so I saved this one as `invoices-november-2026-final (2).xlsx` instead of replacing it. Averyveryveryverylongwordwithoutanyspacesthatshouldwrapratherthanoverflowthecolumnandbreakthelayout." });
    script.push({ type: "state", run: ID, state: "done" });
  }
  if (s.includes("approval")) {
    script.push({ type: "text", run: ID, text: "The message is ready. Asking you before I send it." });
    script.push({ type: "approval", run: ID, id: 1, by: s === "approval-engine" ? "engine" : "agent", wait: 120, t: Date.now() - 14000, src: "/preview-assets/desk-pad.jpg",
      ...(s === "approval-engine"
        ? { box: [0.62, 0.255, 0.04, 0.03], action: "Press “Send”", where: "Mail", content: "button “Send”", why: "It looks like it sends, submits, pays, deletes or signs in, and it hasn't been approved.", undo: "unsure" }
        : { action: "Send email", where: "Mail, new message to maria@northwind.example", content: "Hi Maria, the invoice is attached. Total 850. Best, Alex", why: "You asked me to send the Northwind invoice.", undo: "no" }) });
    if (s === "approval-denied") script.push({ type: "approval-answer", run: ID, id: 1, decision: "deny", text: "" }, { type: "text", run: ID, text: "You denied it, so I didn't send the email. The draft is still open in Mail." }, { type: "state", run: ID, state: "waiting" });
  }
  if (s === "paused") script.push({ type: "pause", run: ID, paused: true, held: false, text: "Paused while you use the mouse or keyboard. I'll continue after 2 seconds of quiet, or press Resume." });
  if (s === "held") script.push({ type: "pause", run: ID, paused: true, held: true, text: "You have control. I won't do anything until you press Give back." });
}
if (s === "running-early") script.splice(1);

/** ?s=cursors: every pointer and tag the live view draws (the overlay's frames are in tests/overlay), on light and dark. */
function Cursors() {
  const all: [string, Tone, boolean?][] = [["Typing", "accent"], ["Needs your OK", "amber"], ["Paused", "dim"], ["Couldn't finish", "red"], ["Done", "accent", true], ["You", "you"]];
  return (
    <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", height: "100vh" }}>
      {["#ffffff", "#1b1c1f"].map((bg) => (
        <div key={bg} style={{ background: bg, padding: 24, display: "grid", gap: 8 }}>
          {all.map(([label, tone, done]) => (
            <div key={label} style={{ position: "relative", width: 300, height: 52, containerType: "inline-size" }}>
              <AgentCursor x={0.04} y={0.08} k={1 / 300} label={label} tone={tone} done={done} />
            </div>
          ))}
        </div>
      ))}
    </div>
  );
}

// ?s=start-menu: the task menu open on the first task; ?s=start-delete: its delete confirm
if (s === "start-menu" || s === "start-delete") setTimeout(() => {
  const row = document.querySelector<HTMLElement>("[data-run]");
  if (!row) return;
  const b = row.getBoundingClientRect();
  if (s === "start-menu") row.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: b.left + 60, clientY: b.top + 30 }));
  else { row.focus(); row.dispatchEvent(new KeyboardEvent("keydown", { key: "Delete", bubbles: true })); }
}, 300);

createRoot(document.getElementById("root")!).render(s === "cursors" ? <Cursors /> : <App initial={s.startsWith("settings") ? "settings" : "home"} />);
if (!s.startsWith("start") && !s.endsWith("signed-out") && !s.startsWith("settings")) {
  setTimeout(async () => { for (const e of script) await emit("run-event", e); document.body.dataset.ready = "1"; }, 300);
} else document.body.dataset.ready = "1";
// settings-update*: bring the Updates group into view
if (s.startsWith("settings-up")) setTimeout(() => document.getElementById("g5")?.scrollIntoView({ block: "start" }), 900);
// ?focus=<css selector> focuses that element after render (no pointer used, so it shows the keyboard focus ring)
const focusSel = new URLSearchParams(location.search).get("focus");
if (focusSel) setTimeout(() => document.querySelector<HTMLElement>(focusSel)?.focus(), 1500);
// debug: ?debug=1 writes element boxes into the DOM for --dump-dom
if (new URLSearchParams(location.search).get("debug")) setTimeout(() => {
  const W = innerWidth;
  const out = [`inner ${innerWidth}x${innerHeight} scroll ${document.documentElement.scrollWidth} body ${document.body.scrollWidth}`, ...[...document.querySelectorAll("*")].filter((e) => { const r = e.getBoundingClientRect(); return r.right > W + 1 || r.left < -1; }).slice(0, 15).map((e) => { const r = e.getBoundingClientRect(); return `${e.tagName}.${e.className} ${r.x},${r.y} ${r.width}x${r.height}`; })];
  const tabbable = [...document.querySelectorAll<HTMLElement>("button, [href], input, textarea, select, [tabindex]")].filter((e) => !(e as HTMLButtonElement).disabled && e.tabIndex >= 0 && e.offsetParent);
  out.push(`tabbable ${tabbable.length}: ` + tabbable.map((e) => e.getAttribute("aria-label") || e.textContent?.trim().slice(0, 24) || e.tagName).join(" | "));
  const unlabelled = tabbable.filter((e) => !(e.getAttribute("aria-label") || e.textContent?.trim() || (e as HTMLInputElement).placeholder));
  out.push(`unlabelled ${unlabelled.length}: ` + unlabelled.map((e) => e.outerHTML.slice(0, 80)).join(" | "));
  for (const q of [".side", "main", ".run-head", ".run-title", ".convo", ".screen-panel"]) { const e = document.querySelector(q); if (e) { const r = e.getBoundingClientRect(); out.push(`${q} ${r.x},${r.y} ${r.width}x${r.height}`); } }
  const pre = document.createElement("pre"); pre.id = "dbg"; pre.textContent = out.join("\n"); document.body.appendChild(pre);
}, 2500);
