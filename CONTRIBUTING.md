# Contributing to Reach

Thanks for helping. Small, focused pull requests are easiest to review.

## Set up
See [Build from source](README.md#build-from-source). Run `cargo test -p reach-core -p reach-app` before you open a pull request. CI runs the same tests and builds the installer, but only for release tags, so pull requests rely on these local checks.

## Where things are
- `crates/reach-core`: the engine (windows, UI Automation, screenshots, mouse and keyboard, the approval guard, the local server).
- `crates/reach`: the engine as a command-line tool with a tray icon.
- `app/`: the Tauri 2 + React app. `app/src-tauri/src/brain.rs` runs the agent CLIs.
- `app/prompts/agent_system.md`: the agent's system prompt and every in-app message.
- `tests/`: test fixtures (fake windows and forms) and the scripts that drive them.

## Testing safely
Reach controls a real desktop, so tests need care:
- Test only in windows you open yourself, such as the fixtures in `tests/`. Set `REACH_TEST_SCREEN` to the fixture's title so the agent's screenshots stay inside it.
- Never send, post, pay, delete or sign in to anything real. Use the local fake forms (`tests/` has a fake mail window and form).
- Keep the stop hotkey, **Ctrl+Alt+Esc**, working.

## UI changes
Read [docs/design.md](docs/design.md). In short: use the tokens at the top of `app/src/styles.css`, Inter Variable (bundled) with Cascadia Mono for code, and `lucide-react` icons only (no emoji). Keep motion under 250 ms and off under `prefers-reduced-motion`. Every screen needs empty, loading and error states, keyboard support and visible focus, and must stay readable at 125 % and 150 % scaling.

Screenshot the screens you changed in light and dark with `tests/ui/shoot.ps1` (it uses `app/preview.html` with a mocked backend) and attach them to the pull request.

New messages the user sees go in `app/prompts/agent_system.md` and follow its voice: plain, short, says what happened and what to do next.

## Safety changes
Changes to the approval guard, the stop path or the system prompt need a test that shows the behaviour (see the tests listed in [docs/safety.md](docs/safety.md)). If a change sends anything new to the AI company, update [docs/privacy.md](docs/privacy.md).
