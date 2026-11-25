// A short title for a task (sidebar, run header): verb first, at most 40 characters.
// Worked out from the task text, so old history gets the same titles without rewriting anything.

const MAX = 40;
/** Polite openers and set-up phrases that aren't the task. */
const LEAD = /^(?:(?:hey|hi|ok|okay)[,!]?\s+)?(?:please\s+|can you\s+|could you\s+|would you\s+|i want you to\s+|i need you to\s+|i'd like you to\s+|go ahead and\s+)*/i;
const WHERE = /^(?:in|on|inside|using)\s+(?:the\s+)?(?:window|app|application|tab|page)\s+(?:titled|called|named)\s+(?:"[^"]*"|“[^”]*”|'[^']*'|\S+)\s*,?\s*/i;
/** Steps that only get somewhere; the title skips them when something follows. */
const MEANS = /^(?:open|launch|start|go to|switch to|bring up|focus)\s+(.+)$/i;

export function taskTitle(task: string): string {
  let t = task.replace(/\s+/g, " ").trim();
  t = t.replace(/\s*\([^)]*\)/g, "").replace(LEAD, "").replace(WHERE, "").replace(LEAD, "");
  // the first sentence holds the goal; details after a colon are the payload
  t = t.split(/(?<=[.!?])\s|[:;]\s?/)[0].replace(/[.!?]+$/, "");
  const parts = t.split(/,?\s+(?:and then|then|and|after that)\s+|,\s+/i).map((p) => p.trim()).filter(Boolean);
  let app = "";
  // "In VS Code, write …": the app goes after the goal, not first
  const lead = parts.length > 1 ? parts[0].match(/^(?:in|on|using)\s+(?:the\s+)?(.+)$/i) : null;
  if (lead) { app = lead[1]; parts.shift(); }
  while (parts.length > 1) {
    const m = parts[0].match(MEANS);
    if (!m) break;
    app = m[1].replace(/^the\s+/i, "");
    parts.shift();
  }
  let main = (parts[0] ?? t).replace(/\b(?:these|the following)\s+/i, "");
  if (app && !/\b(?:in|on|to|into|from)\s/i.test(main) && `${main} in ${app}`.length <= MAX) main = `${main} in ${app}`;
  main = main.charAt(0).toUpperCase() + main.slice(1);
  if (main.length <= MAX) return main;
  const cut = main.slice(0, MAX - 1);
  return cut.slice(0, cut.lastIndexOf(" ") > 20 ? cut.lastIndexOf(" ") : cut.length).replace(/[,\s]+$/, "").replace(/(?:\s+(?:in|on|of|to|the|a|an|and|for|with|from|into|at|by))+$/i, "") + "…";
}
