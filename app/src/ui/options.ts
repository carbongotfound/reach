import { S } from "../strings";

// Keys are fixed; labels and hints live in strings.ts.
export const AGENTS = [["claude", S.agents.claude], ["codex", S.agents.codex]] as const;
/** The model picker's items: plain names, no service marks. */
export const agentItems = () => AGENTS.map(([value, label]) => ({ value, label, hint: S.agentHints[value] || undefined }));
export const SPEEDS = (["instant", "fast", "smooth"] as const).map((k) => [k, S.speeds[k].label, S.speeds[k].hint] as const);
/** Icons for the start screen's suggestions: the apps' Windows ids (an app that isn't installed gets the neutral icon). */
export const IDEA_APPS = [
  "Microsoft.WindowsNotepad_8wekyb3d8bbwe!App",
  "Microsoft.Office.EXCEL.EXE.15",
  "Microsoft.WindowsCalculator_8wekyb3d8bbwe!App",
  "windows.immersivecontrolpanel_cw5n1h2txyewy!microsoft.windows.immersivecontrolpanel",
].map((id) => `shell:AppsFolder\\${id}`);
