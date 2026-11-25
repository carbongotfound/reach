// node tests/app/model-check.mjs runs this (bundled, since model.tsx has JSX):
// repeated pause notices collapse into one line, and step labels never show raw coordinates.
import assert from "node:assert/strict";
import { describe, empty, reduce, type Step, type View } from "../../app/src/model.tsx";

const PAUSE = "Paused while you use the mouse or keyboard. I'll continue after 2 seconds of quiet, or press Resume.";
let v: View = reduce(empty, { type: "start", id: "r1", task: "Scroll the list" });
for (let i = 0; i < 3; i++) {
  v = reduce(v, { type: "pause", paused: true, held: false, text: PAUSE });
  v = reduce(v, { type: "pause", paused: false });
  v = reduce(v, { type: "step", n: i + 1, tool: "mouse", action: "scroll", args: { coordinate: [15, 250], direction: "down" }, ok: true, text: "", ms: 10 });
}
assert.equal(v.items.filter((i) => i.k === "msg" && i.text === PAUSE).length, 1, "three pauses, one notice");
v = reduce(v, { type: "text", text: "Done scrolling." });
v = reduce(v, { type: "pause", paused: true, held: false, text: PAUSE });
assert.equal(v.items.filter((i) => i.k === "msg" && i.text === PAUSE).length, 2, "a pause after a message is a new notice");

const step = (args: Record<string, unknown>, app?: string): Step => ({ n: 1, tool: "mouse", action: "scroll", args, ok: true, text: "", ms: 1, ...(app ? { win: { h: 1, key: "", app, screen: 1, box: [0, 0, 1, 1] } } : {}) });
assert.equal(describe(step({ coordinate: [15, 250], direction: "down" })), "Scrolled down");
assert.equal(describe(step({ coordinate: [15, 250], direction: "down" }, "Notepad")), "Scrolled down in Notepad");
assert.equal(describe({ ...step({ coordinate: [15, 250] }), action: "left_click" }), "Left click");
for (const a of ["scroll", "left_click", "drag", "double_click"]) assert.doesNotMatch(describe({ ...step({ coordinate: [15, 250], direction: "up" }, "Paint"), action: a }), /\d/, a);
console.log("model-check: ok");
