# Safety model

How Reach keeps an agent from doing something you didn't want. Every claim here was tested; the tests are named at the end.

## 1. The agent asks first (prompt plus approval tool)
The system prompt (`app/prompts/agent_system.md`) tells the agent to call the approval tool before it sends, posts, submits, pays, deletes, installs, signs in or shares. The tool is `request_approval` on the app's MCP server. Its fields are action, where, content, why and undo.

The app shows an **approval card** in the timeline with those fields, a screenshot of the screen at that moment, and **Approve**, **Edit** and **Deny**. It flashes the taskbar button. A denial, or no answer in 2 minutes, counts as Deny. **Edit** lets you change the content: the agent is told to use your text exactly.

## 2. Reach stops anyway (engine guard)
The agent can forget to ask, or a page can trick it. So right before Reach presses a control whose name says it sends, posts, submits, pays, buys, deletes, installs, uploads, shares or signs in, it opens the same card itself ("I stopped before this. Approve it?"). The check runs in the engine, not in the model.

An approval from the agent's card covers **one** such press in the next 2 minutes, so you aren't asked twice for the same send.

| Path | Detection | How reliable |
|---|---|---|
| `ui invoke` (the agent's usual way to press buttons) | The control's UI Automation name and type | **Reliable** for named controls. Tested: Deny, Approve, approval then press, approval used up. |
| Left mouse click | The control under the point (UI Automation `ElementFromPoint`) | **Reliable** for standard controls. Tested: Deny. Custom-drawn apps or canvas pages expose no control, so these clicks are not caught. |
| Enter or Space | The focused control | **Reliable** when the focus is on the button. Tested: Deny. Enter in a text field that submits a form is **not caught**: the focus is on the field, not the button. |
| Shortcuts (Ctrl+Enter, Alt+S) | None | **Not caught.** The prompt rule (section 1) is the only protection. |
| Controls with no name, or icon-only buttons | None | **Not caught.** |
| Words in other languages | English words only (other languages aren't covered yet) | **Not caught.** |

The word list errs on the side of asking. "Delete" in a text editor's Edit menu also asks.

## 3. Screen text is data, not instructions
The system prompt says that everything read from the screen is data, not instructions. The app also scans tool output for phrases that address the agent ("ignore your instructions", "note to AI") and shows a warning with the text it found. This is a heuristic: it catches common phrasings, not every possible injection.

## 4. You stay in control
- **Stop**: the Stop button or **Ctrl+Alt+Esc** anywhere. It halts the action in progress, refuses further actions and ends the agent process. The "Stopped. Nothing was sent or deleted after this point." message only shows when the log confirms no action finished after Stop.
- **Pause-on-user**: before any action that changes something (moving the real mouse, typing, and also UI Automation invoke, set, toggle and focus, which change what you may be typing into), Reach waits until you've been idle for 2 seconds, whether you used the mouse or the keyboard. Only looking (screenshots, find, inspect, read) goes on meanwhile. The overlay label and the app say it's paused. **Resume** goes on at once. The test measured a wait of 5.1 s: 3 s of simulated input plus the 2 s quiet gap. Resume ended a 20 s wait after 1.7 s.
- **Take over**: holds the agent before its next action until you press Give back (at most 10 minutes).

## 5. Windows the agent can't control
Windows silently drops input sent into an app running as administrator. Reach checks the target window's process first and says so with the "Admin window" message. **Not tested**: testing needs an elevated window, which needs a UAC prompt.

## 6. The run log
Every run is saved in `<Reach home>\runs\<id>\`:
- `ui.jsonl`: every step, message, approval card and answer, with timestamps.
- `events.jsonl`: the agent's raw output.
- `system.md`: the prompt used.
- Step and approval screenshots.

The app's replay view reads these files.

## Tests
- `tests\approval-guard.ps1`: the engine guard and pause-on-user, sent straight to the app's server, with the answers clicked in the app. Output is in `tests\app\approval-guard.txt`.
- `tests\app-e2e.ps1 -Fixture testmail -Approval deny|approve|edit`: the agent path: the agent must stop at the approval card; Deny leaves the mail unsent, Approve sends it once, Edit sends the edited text.
