// node tests/app/state-check.ts — a finished, stopped or failed run never shows as running.
// Each case is the state sequence the backend writes for a run (brain.rs set_state), applied to the run list.
import assert from "node:assert/strict";
import { activeRun } from "../../app/src/runstate.ts";

const cases: [string, string[]][] = [
  ["finish", ["running", "waiting"]],           // turn ended, the task can take a reply: shows Done
  ["finish, process exits", ["running", "waiting", "done"]],
  ["stop", ["running", "stopped"]],
  ["error", ["running", "failed"]],
  ["reply, then finish", ["running", "waiting", "running", "waiting"]],
];
for (const [name, seq] of cases) {
  const runs = [{ id: "20261126-010000", state: "done" }, { id: "20261126-020000", state: "idle" }];
  seq.forEach((s, i) => {
    runs[1].state = s;
    const want = s === "running";
    assert.equal(!!activeRun(runs), want, `${name}: step ${i} (${s}) should ${want ? "" : "not "}show a running banner`);
  });
  assert.equal(activeRun(runs), undefined, `${name}: no running banner at the end`);
}
assert.equal(activeRun(null), undefined);
console.log(`state-check: ${cases.length} ok`);
