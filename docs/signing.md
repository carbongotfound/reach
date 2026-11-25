# SmartScreen and code signing

## Why Windows warns you

Reach's builds are **not code-signed yet**. When you run the installer or the portable exe, Windows SmartScreen may show "Windows protected your PC". The warning means Windows doesn't know who published the file. It doesn't mean a virus was found.

To run it anyway:
1. Click **More info**.
2. Check that the file is one you downloaded from this project's GitHub Releases page.
3. Click **Run anyway**.

To check that the file wasn't changed, compare its SHA-256 hash with `SHA256SUMS.txt` from the same release:

```powershell
Get-FileHash .\Reach_0.3.0_x64-setup.exe -Algorithm SHA256
```

Each CI build writes `SHA256SUMS.txt` next to the files (see `.github/workflows/build.yml`).

## Plan to sign

Goal: every release signs `reach-app.exe`, `reach.exe` and the installer in CI, with the key kept out of the repository.

1. **Get a signing identity.** These options were current when this plan was written. Check prices and eligibility before choosing.
   - **Azure Artifact Signing** (formerly Trusted Signing). A monthly subscription, and Microsoft holds the key. Identity validation is open to organisations in the USA, Canada, the EU and the UK, and to individual developers in the USA and Canada only. Some 2026 reports say new onboarding is paused, so check the Azure portal first.
   - **SignPath Foundation**. Free signing for open-source projects that meet its rules. Builds are signed from CI after review.
   - **An OV code-signing certificate** from a certificate authority. Since 2023 the private key must live on a hardware token or a cloud HSM, so it suits a cloud HSM better than a laptop.
2. **Sign in CI.** Tauri can call a signing tool for every file it builds (`bundle.windows.signCommand` in `tauri.conf.json`). The `reach` CLI exe is signed in a separate step with the same tool. Credentials live in GitHub Actions secrets, and only tag builds can use them.
3. **Timestamp every signature**, so files stay valid after the certificate expires.
4. **Build reputation.** A new certificate, EV included, still starts with no SmartScreen reputation. The warning fades as more people run signed builds. Until then, keep the "Run anyway" steps above in the README.

**Chosen: SignPath Foundation** (2026-11-24). The application hasn't been sent yet. Next steps once it is accepted:
1. SignPath gives an organisation ID, a project slug and a signing-policy slug for `carbongotfound/reach`.
2. Add the API token as the GitHub secret `SIGNPATH_API_TOKEN`, and the IDs as repository variables.
3. In `.github/workflows/build.yml`, upload the unsigned exes and installer as an artifact. Then submit them with SignPath's `submit-signing-request` action (wait for completion), and publish the signed files and `SHA256SUMS.txt` from its output. Only tag builds get the token.
4. Switch the updater to require a valid signature: set `REQUIRE_SIGNATURE = true` in `app/src-tauri/src/update.rs`, so an unsigned or wrongly signed installer is refused even when its checksum matches. Ship that in the first signed release, not before.

Until then builds stay unsigned, and the steps above apply.

## SignPath Foundation application (prepared, not sent)

Everything the application form asks for, ready to paste. Check SignPath's current form and rules before sending; nothing has been submitted.

| Field | Answer |
|---|---|
| Project name | Reach |
| Repository | https://github.com/carbongotfound/reach |
| Publisher | Carbon (GitHub: carbongotfound) |
| Licence | MIT (OSI-approved), see `LICENSE` |
| Homepage / downloads | https://github.com/carbongotfound/reach/releases |
| Build system | GitHub Actions (`.github/workflows/build.yml`), from source in the repository |
| Files to sign | `reach-app.exe`, `reach.exe`, `Reach_<version>_x64-setup.exe` |
| Platform | Windows 10 and 11, x64 |

**Short description:** Reach is a free, open-source Windows desktop app that lets an AI agent (Claude Code or Codex, signed in with the user's own account) carry out tasks in the apps on the user's PC, while the user watches every step.

**What the app does:**
- Reads app controls through Windows UI Automation, takes screenshots, and moves the mouse and types when the user asks it to do a task.
- Shows every step live, and stops instantly with the Stop button or Ctrl+Alt+Esc.
- Pauses when the user moves the mouse or types, and lets the user take over and give control back.
- Asks the user to approve before anything that sends, pays, deletes or signs in. No answer in 2 minutes counts as Deny.
- Runs a local server on 127.0.0.1 only, protected by a random token, for the agent CLI to call.

**What it doesn't do:**
- No telemetry, no accounts, no update checks, no ads. Reach itself sends nothing over the network; the agent CLI the user chose talks to its own company.
- It doesn't run as administrator, doesn't install drivers or services, and doesn't change system settings.
- It doesn't collect, store or upload passwords or files. Run logs and screenshots stay on the user's PC.
- It doesn't hide from the user: a visible pointer and label show while the agent acts, and the app window lists every action.

**Why signing matters for this project:** unsigned builds trigger SmartScreen warnings, and an app that controls the desktop should prove where it came from, so users can check that a download wasn't tampered with.
