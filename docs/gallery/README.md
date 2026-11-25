# Reach 0.3.0 gallery

Pictures of every part of Reach, made on 2026-11-25 from the current build. Light and dark follow the Windows setting. Every task in these pictures is a test run; no real account, file or window of the user's appears.

## 1. The agent's pointer and its tag (`1-pointer-and-pill/`)

The teal arrow is the agent's own pointer, separate from your mouse, with a solid tag on it. Rendered by the overlay itself at 150 %, light and dark:

| File | State |
|---|---|
| `1-thinking-*`, `2-acting-*` | Working: the agent's name |
| `4-waiting-*` | Needs your OK, with the time it has waited (amber) |
| `5-paused-*` | Paused while you use the mouse or keyboard (grey) |
| `6-stopped-*` | Stopped by the Stop button or Ctrl+Alt+Esc (grey) |
| `7-done-*` | Done, shown for 3 seconds |
| `8-error-*` | What went wrong, in red, shown for 6 seconds |
| `9-card-*` | The approval card that grows from the tag: Approve, Edit (opens the app), Deny |

## 2. A real run (`2-real-run/`)

Frames of the real app window during a real task in the Test Pad, a stand-in for Notepad: "Write hello, ask me before saving, then save it as hello". One run in dark, one in light. `1-working` (the steps, the screen with the agent's pointer, the window thumbnail), `2-needs-your-ok` (the approval card above the composer), `3-save-box` (after Approve, the agent fills in the Save box), `4-done`. The README's screenshots are a later VS Code run.

## 3. The app (`3-app/`)

Every screen from the design preview, with sample tasks, light and dark:

- `start`, `start-empty`, `start-loading`: the task box, with and without past tasks, and while tasks load
- `signed-out`: the agent CLI isn't signed in
- `running-early`, `running`, `paused`, `held`, `waiting`: before the first screenshot; steps and the screen panel; paused for you; you took over; the agent asks a question
- `approval`, `approval-engine`, `approval-denied`: the approval card (asked by the agent, by Reach's own check, and after Deny)
- `done`, `long`, `failed`: the end of a task (with a table in the summary), and a failure
- `settings`, `settings-loading`, `settings-error`: Settings, and its loading and error states

The update card isn't shown: until a real release exists, it could only show invented version numbers. Screens with ChatGPT aren't shown either, because OpenAI asks that an app's screenshots be free of its brands (see `docs/branding.md`).

## 4. Logo (`4-logo/`)

A hand reaching for a dot: the app icon at each size (a simpler hand at 16 to 24 px), the wordmarks for light and dark pages, and the social preview.

## 5. The real window (`5-real-window/`)

The actual app window, not the preview, with five sample tasks: the start screen at 900, 1200 and 1600 px wide in light and dark (`start-<theme>-<width>.png`), and the finished run at the same widths (`done-dark-<width>.png`). Two at higher display scaling: `start-light-900-150pct.png` (150 %) and `start-dark-1200-125pct.png` (125 %).
