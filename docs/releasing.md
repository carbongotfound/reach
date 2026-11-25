# Releasing Reach

The app's update check reads GitHub Releases for the repository named in `REPO` in `app/src-tauri/src/update.rs` (now `carbongotfound/reach`). A release is offered to users only when all of this is true:

- it is **published**: drafts are ignored;
- its tag is a semver version with a `v` in front (`v0.3.1`, `v0.4.0-beta.1`); other tags are ignored;
- it is not marked **pre-release**, and its version has no `-beta` style suffix. Pre-releases are offered only to people who turned on **Include beta versions**;
- its version is newer than the installed build, compared as semver (so `0.10.0` is newer than `0.9.0`);
- it has the NSIS installer as an asset (a name ending in `_x64-setup.exe`) **and** a `SHA256SUMS.txt` asset that lists it. Without the sums file the app refuses to install.

## 1. Set the version

One version everywhere, with no `v`:

- `Cargo.toml` (workspace `version`), which the crates inherit;
- `app/src-tauri/tauri.conf.json` (`version`);
- `app/package.json` (`version`);
- `CHANGELOG.md`: a `## <version>` section. The release notes are taken from it, and CI fails if it's missing.

The installed version the updater compares against comes from the build (`CARGO_PKG_VERSION`), not from any file a user can edit.

## 2. Tag

```powershell
git commit -am "Release 0.3.1"
git tag -a v0.3.1 -m "Reach 0.3.1"
git push origin main v0.3.1
```

Use `v0.4.0-beta.1` style tags for betas. CI marks a tag with a `-` in it as a pre-release.

## 3. Build (CI does this)

`.github/workflows/build.yml` runs on every tag `v*`:

1. runs the tests;
2. builds the CLI (`cargo build --release -p reach`) and the app with its NSIS installer (`npx tauri build`);
3. copies the installer, the portable exe and the CLI into `dist-release/`;
4. writes `dist-release/SHA256SUMS.txt`: one `<lowercase sha256>  <file name>` line per file;
5. takes the `## <version>` section of `CHANGELOG.md` as the notes and creates a **draft** release with all the files.

To build by hand instead:

```powershell
cargo build --release -p reach
cd app; npx tauri build; cd ..
Get-FileHash dist-release\*.exe -Algorithm SHA256 |
  ForEach-Object { "$($_.Hash.ToLower())  $(Split-Path $_.Path -Leaf)" } |
  Set-Content dist-release\SHA256SUMS.txt
```

## 4. Sign (when SignPath is ready)

Builds are unsigned for now (see `docs/signing.md`). Once a signing identity exists:

- sign `reach-app.exe`, `reach.exe` and the installer in CI with a timestamp (`bundle.windows.signCommand` in `tauri.conf.json` for the app and installer, a separate step for the CLI);
- write `SHA256SUMS.txt` **after** signing, since signing changes the hash.

The updater checks the Authenticode signature when the installer has one. A broken signature makes it refuse the file even when the hash matches. An unsigned installer is accepted on the hash alone.

## 5. Publish

1. Open the draft on GitHub and check the notes and the files: the installer, the portable exe, the CLI and `SHA256SUMS.txt`.
2. Untick **Set as a pre-release** unless it is a beta.
3. Press **Publish release**.

The app's update check finds the new release within 6 hours, or straight away with **Settings > Check now**. Release notes appear in the card as plain text; HTML in them is shown as text, never rendered.

## Checking a release before users see it

Run the app against a local copy of the release JSON instead of GitHub:

```powershell
python -I tests\updater\mock.py 7499   # serves fake releases on 127.0.0.1:7499
$env:REACH_UPDATE_API = "http://127.0.0.1:7499"
.\target\release\reach-app.exe
```

`REACH_UPDATE_API` is honoured only for `127.0.0.1` and `localhost`, so it can't point a real install at another server.
