# What leaves your PC

Reach itself sends nothing about you over the internet. It has no telemetry and no account. The one request it makes on its own is the update check (below). Its only network listener is the control server on `127.0.0.1` (this PC only), protected by a random token.

The AI does leave your PC. Reach runs the agent CLI you picked (Claude Code or Codex), and that CLI sends the conversation to its company to be answered. Everything below goes to **Anthropic** (Claude) or **OpenAI** (ChatGPT), under your own account and their terms.

## The update check

The app asks GitHub for the latest release of Reach after it starts and then every 6 hours: one HTTPS request to `api.github.com/repos/carbongotfound/reach/releases/latest` (or `/releases` when **Include beta versions** is on). The request carries no identifiers. It has no user ID, install ID or settings, and no information about your PC beyond what any HTTPS request carries (your IP address and the fixed user agent `Reach-update-check`). It repeats the cached ETag so an unchanged answer costs nothing.

Downloading an update happens only when you press **Install**, and only from that release's own files on GitHub.

To turn it off, clear **Settings > Updates > Check for updates automatically**. **Check now** still works when you press it. The `reach` command-line tool never checks.

## Sent to the AI company

| What | When | Notes |
|---|---|---|
| Your task and your replies | Every task | Exactly what you type. |
| The system prompt | Every task | Includes the date and time, Windows version, screen sizes and scaling, the Speed setting, which Office apps are installed, and the **names** of open apps (for example "excel, notepad"). Window titles are left out. |
| Screenshots | When the agent takes one | A JPEG of a window, a region or the whole screen, 1280 px on the long edge unless the agent asks for more (at most 4096). Anything visible there goes too, including other windows that overlap it. |
| Text read from apps | When the agent reads controls (`ui find`, `inspect`, `read`) | Control names and values, such as a document's text, a form's fields or a list's items. |
| Window list | When the agent lists windows (`apps list`) | Titles, app names and positions of open windows. Titles can contain file names, email subjects or page titles. |
| Results of actions | After each action | Short text such as "clicked e7" or "typed 12 characters". |
| Approval cards | When an approval is asked for | What the agent wrote on the card. Your answer goes back as text (Approved, Denied, or your edited content). The card's screenshot stays on your PC. |

What each company keeps, for how long and whether it trains on it depends on your plan and settings with them. See Anthropic's and OpenAI's privacy policies. The CLIs may also send their own usage data. Claude Code and Codex document how to turn that off.

## Kept on your PC only

- **Run logs** in `<Reach home>\runs\<id>\`: every step, message and approval with timestamps (`ui.jsonl`), the agent's raw output (`events.jsonl`), the prompt used (`system.md`), and step and approval screenshots. Delete a folder to delete that run.
- **Settings**: `config.json` (Speed, port, update settings, skipped versions and the last check time), `brain.txt` (Claude or ChatGPT), `update-cache.json` (GitHub's last answer and its ETag).
- **The control-server tokens**: `mcp.json` (the app's token, new on every launch) and `token` (the `reach` command-line server's token, kept until you delete it). Neither is sent anywhere.
- **Your sign-ins**: Reach never sees your Anthropic or OpenAI password or tokens. The CLIs store them in their own folders.

`<Reach home>` is `%LOCALAPPDATA%\Reach` for the app and `%USERPROFILE%\.reach` for the `reach` command-line tool, unless `REACH_HOME` is set.

## How to send less

- Close or minimise windows you don't want in a screenshot before you start a task.
- Ask for tasks in specific apps ("in Notepad, ..."). The agent then mostly reads that app's controls instead of taking full-screen screenshots.
- Press **Stop** or **Ctrl+Alt+Esc** at any time. Nothing more is read or sent after that.
