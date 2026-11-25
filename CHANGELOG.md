# Changelog

## 0.3.0 (public beta)

The first release of the Reach desktop app. Earlier versions (0.1.0 to 0.2.1) were the command-line server only.

### New

- **Desktop app** (Windows 10 and 11). You type a task, and Claude Code or Codex works in your Windows apps through Reach. You need a Claude or ChatGPT account, signed in through that app's own CLI. Reach never sees your password or key.
- **Steps you can follow.** Every action is a line in a timeline: the app's own icon, one sentence, and the app and screen it happened on. The screen panel beside it shows the window the agent is working in after each step, with the agent's pointer where it acted, a Live badge and a scrubber to replay, plus a thumbnail of every window it used (click to enlarge). App icons come from your installed apps through Windows, Store apps included.
- **A calmer window.** Light and dark follow Windows. Inter, large bold headings, white and light grey surfaces, and one cobalt accent for your messages, the main buttons, the Live badge and the agent's pointer. The sidebar has New task, Search, your tasks with a status line (what it's doing now, or how it ended and when), a Screens section with a small live picture of each monitor and Idle or Working (hidden when there's nothing to show), and Settings at the bottom. Works from 900 x 600 at 100, 125 and 150 % scaling.
- **Approvals.** Before anything that sends, pays, deletes or signs in, the agent stops and shows a card ("Claude is asking" or "ChatGPT is asking"): what, where, the content, whether it can be undone, and a crop of the screen with the target outlined, in a full-width card above the composer. You pick Approve, Edit or Deny on one row. No answer in 2 minutes counts as Deny. The engine also stops on buttons named like Send, Pay or Delete, even if the agent didn't ask.
- **Stop and take over.** Stop in the window, or Ctrl+Alt+Esc from anywhere. Take over pauses the agent while you use the PC, and Give back lets it carry on.
- **Its own pointer, with a tag.** A slim arrow in the accent colour shows where the agent is working, separate from your mouse. A tag on the arrow says what it's doing in a word (Typing, Clicking), or Needs your OK, Paused, Stopped, Done with a check, or what went wrong. Amber when it needs your OK, dimmed when paused or stopped, red only on errors. No timers; the step itself is in the timeline. The live view draws the same pointer and tag. When an approval is waiting, the tag grows into a small card with Approve, Edit and Deny. It stays still when Windows' "Animation effects" is off, never takes the focus from the app you're in, and never appears at your own mouse pointer, only where the agent is working. Reach pauses while you use the mouse or keyboard: before clicks and keys, and also before it presses a button or fills a field for you.
- **New logo.** A pointing hand, the index finger reaching toward a dot, traced from a drawn concept into smooth curves, with no background tile. Hand and dot are one colour that follows the surface: near-black on light, white on dark. In the app it follows the Theme setting; the tray and taskbar icons follow Windows' taskbar theme and switch when it changes. At 16 and 24 px the curled fingers close into one shape, and 16 px drops the dot. The exe, installer and shortcut icon, which can't switch, is white with a thin dark outline so it reads on both.
- **Service names.** ChatGPT and Claude are shown by name only, with a small status dot; no service logos (see `docs/branding.md`).
- **Update check.** When a newer version is published on GitHub, a card in the window shows the installed and latest versions, what's new (as plain text), and Install, Remind me later and Skip this version. Install downloads the installer only from this project's GitHub release and checks it against the release's `SHA256SUMS.txt` (and its signature, once releases are signed). Then it asks once before closing Reach to install. It never shows over a running task or installs during one. The check sends no identifiers, and you can turn it off in Settings (see `docs/privacy.md`).
- **Task menu.** Right-click a task in the sidebar, or use its "..." button: Rename, Open run folder, Copy task text, Stop (while it runs) and Delete (after a confirm; removes its log and screenshots). F2 renames and Delete deletes.
- **Switch model or speed mid-task.** The pickers stay in the reply box and apply to your next message; a line in the thread says when you switched model.
- **Approvals keep your place.** After you press Approve or Edit, the app the agent was working in gets the focus back, so its next key goes to the right window.
- **Opens apps by name.** For example, "work something out in Calculator" starts Calculator itself.
- **Warns about text on screen aimed at the AI.** If a window contains text that tries to give the agent instructions, Reach shows a warning, and the agent is told not to follow it.
- **Settings:** speed (Instant, Fast, Human), pause while you use the PC, the agent's cursor, Theme (System, Light, Dark; Reach only, your Windows setting stays the same), and Updates (check automatically, include beta versions, Check now). Settings and About show the installed version.
- **Installer** for your user account only, no admin needed. A portable exe and the command-line server are built too.
- The MCP server for other agents keeps port 7420, the same token and the same tool names.

### Tested

On one Windows 11 PC, with Claude Code as the agent, in test windows Reach opens itself (a fake mail app, a text editor, an order-status window):

- A full task in Notepad, and one where the agent opened Calculator by name and used it.
- Approve, Edit and Deny on a mail that would be sent. Nothing real was sent.
- Stop in the window (under 1 second) and the Ctrl+Alt+Esc hotkey.
- Take over and Give back.
- A password field: the agent stops and asks you to type it.
- Light and dark themes, in the real window and in the design preview.
- A test mode (`REACH_TEST_SCREEN`) that keeps the agent inside the test window: no opening apps, no actions or clicks in other windows, screenshots fenced to the test window.
- 125 % and 150 % display scaling on two monitors, at the minimum size (900 x 560) and a large size.
- The redesigned window with five sample tasks at 900, 1200 and 1600 px wide in light and dark, and at 125 % and 150 % scaling (`docs/gallery/5-real-window/`).
- The installer: silent install, the installed app starting, and silent uninstall. Your run history and settings are kept.
- ChatGPT (Codex) with a stand-in CLI: a task, a question, a reply that resumes the run, the signed-out message, and Stop.
- The pointer and tag: every state and the approval card, light and dark, at 100, 125 and 150 % on a real display, over white, black, a photo and busy app windows; and two real runs in the Test Pad, one dark and one light, from the task to Done: the live view, the window thumbnails, the approval card, Approve and the Save box (`docs/gallery/2-real-run/`, and the README screenshots: VS Code, prices.py).
- The update check against a local fake GitHub: no update, update available, skipped, remind me later, beta versions on and off, drafts ignored, a wrong checksum (refused, file deleted), a cancelled download, a server error, rate limiting, offline, and Install refused during a task.
- A real update on a test PC: the installed 0.3.0 found a dummy 0.3.1 on the local fake GitHub, downloaded and checked it, closed, ran the installer, and came back as 0.3.1 with its settings. Then it was uninstalled and everything it touched was put back.

### Not tested yet

- **ChatGPT end to end.** Tested with a stand-in CLI and with real Codex up to sign-in, but no full task has run with a signed-in ChatGPT account.
- **The admin-window check.** Windows blocks input into apps running as administrator. Reach checks for this and says so, but the check hasn't been tried against a real elevated window.
- **Unsigned build.** The installer and exes aren't code-signed yet, so Windows SmartScreen warns before the first start. Check downloads against `SHA256SUMS.txt`. Signing is planned through SignPath Foundation (see `docs/signing.md`).
- **The update check against a real GitHub release.** The whole update has run on a test PC, but against a local copy of GitHub's API. 0.3.0 is the first release the check can find newer versions from.
- **Reporting hidden instructions.** The app's own warning works. The agent's prompt now asks it to quote such text in its reply, but that wording hasn't been run against the real model yet.
- The installer on a clean PC, and languages other than English (the words that make Reach stop and ask are English only).
