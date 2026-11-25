//! Update check. Asks GitHub's public Releases API for the newest release of REPO (no
//! identifiers sent, an ETag cache, rate limits respected). Only when the user presses
//! Install: downloads the Windows installer from that release's own assets, checks it against
//! the release's SHA256SUMS (and its signature when it is signed), asks once, then quits and
//! runs it with /P /UPDATE /R (progress only, keep files, relaunch). Release notes are text,
//! never HTML, and nothing in them is run.
use crate::strings;
use reach_core::config;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::Emitter;

/// Where releases come from. The one place to change if the repository is renamed.
pub const REPO: &str = "carbongotfound/reach";
/// Installed version: the build's own version, never a file the user can edit.
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");
const EVERY: u64 = 6 * 3600;
/// A restart within this long reuses the cached answer instead of asking GitHub again.
const FRESH: u64 = 15 * 60;
// ponytail: unsigned installers are accepted after the SHA-256 check; set this to true once
// releases are signed through SignPath (docs/releasing.md), so an unsigned file is refused.
const REQUIRE_SIGNATURE: bool = false;

/// The API base. REACH_UPDATE_API points the tests at a local mock server; only loopback
/// addresses are accepted, so it can't send a real install anywhere else.
fn test_base() -> Option<String> {
    let b = std::env::var("REACH_UPDATE_API").ok()?.trim_end_matches('/').to_string();
    loopback(&format!("{b}/")).then_some(b)
}
fn api() -> String { test_base().unwrap_or_else(|| "https://api.github.com".into()) }

fn now() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) }

#[derive(Clone, Debug, PartialEq)]
pub struct Rel {
    pub version: semver::Version,
    pub date: String,
    pub notes: String,
    pub page: String,
    pub pre: bool,
    /// (file name, download URL, size)
    pub installer: Option<(String, String, u64)>,
    pub sums: Option<String>,
}

/// One release from the API, or None for drafts and tags that aren't versions.
pub fn parse_rel(v: &Value) -> Option<Rel> {
    if v["draft"].as_bool().unwrap_or(false) { return None; }
    let version = semver::Version::parse(v["tag_name"].as_str()?.trim_start_matches(['v', 'V'])).ok()?;
    let assets = v["assets"].as_array().cloned().unwrap_or_default();
    let asset = |pick: &dyn Fn(&str) -> bool| assets.iter().find(|a| a["name"].as_str().is_some_and(pick));
    let s = |a: &Value, k: &str| a[k].as_str().unwrap_or("").to_string();
    Some(Rel {
        pre: v["prerelease"].as_bool().unwrap_or(false) || !version.pre.is_empty(),
        version,
        date: s(v, "published_at"),
        notes: s(v, "body").chars().take(4000).collect(),
        page: s(v, "html_url"),
        installer: asset(&|n| n.ends_with("_x64-setup.exe")).map(|a| (s(a, "name"), s(a, "browser_download_url"), a["size"].as_u64().unwrap_or(0))),
        sums: asset(&|n| n == "SHA256SUMS.txt" || n == "SHA256SUMS").map(|a| s(a, "browser_download_url")),
    })
}

/// The newest usable release in an API answer (one object or a list).
pub fn pick(body: &Value, beta: bool) -> Option<Rel> {
    let list = match body { Value::Array(a) => a.clone(), v => vec![v.clone()] };
    list.iter().filter_map(parse_rel).filter(|r| beta || !r.pre).max_by(|a, b| a.version.cmp(&b.version))
}

/// The hash listed for `name` in a SHA256SUMS file ("<hex>  <name>" or "<hex> *<name>").
pub fn sum_for(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let (h, f) = l.trim().split_once(char::is_whitespace)?;
        (f.trim().trim_start_matches('*') == name && h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit())).then(|| h.to_ascii_lowercase())
    })
}

/// Only this repository's own release assets (or the test server) may be downloaded.
pub fn allowed(url: &str, test: Option<&str>) -> bool {
    match test {
        Some(b) => url.starts_with(&format!("{b}/")),
        None => url.starts_with(&format!("https://github.com/{REPO}/releases/download/")),
    }
}
/// Only this repository's pages (or the test server) may be opened from the card.
fn page_ok(url: &str) -> bool {
    match test_base() { Some(b) => url.starts_with(&format!("{b}/")), None => url.starts_with(&format!("https://github.com/{REPO}/")) }
}

fn loopback(url: &str) -> bool { url.starts_with("http://127.0.0.1:") || url.starts_with("http://localhost:") }

/// HTTPS only (redirects included), except to the tests' loopback server.
fn agent(url: &str) -> ureq::Agent {
    let mut b = ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(10)).timeout_read(Duration::from_secs(30))
        .user_agent("Reach-update-check").https_only(!loopback(url));
    if let Ok(tls) = native_tls::TlsConnector::new() { b = b.tls_connector(Arc::new(tls)); }
    b.build()
}

#[derive(Debug, PartialEq)]
pub enum Got { Fresh { etag: String, body: String }, NotModified, Missing, Limited(u64), Failed(u16), Offline }

/// GET a JSON URL with If-None-Match. Rate limits come back as Limited(the time to try again).
pub fn get_json(url: &str, etag: &str) -> Got {
    let mut req = agent(url).get(url).set("Accept", "application/vnd.github+json").set("X-GitHub-Api-Version", "2022-11-28");
    if !etag.is_empty() { req = req.set("If-None-Match", etag); }
    match req.call() {
        Ok(r) if r.status() == 304 => Got::NotModified,
        Ok(r) => {
            let etag = r.header("etag").unwrap_or("").to_string();
            match r.into_string() { Ok(body) => Got::Fresh { etag, body }, Err(_) => Got::Offline }
        }
        Err(ureq::Error::Status(404, _)) => Got::Missing,
        Err(ureq::Error::Status(code, r)) if code == 403 || code == 429 => {
            let num = |h: &str| r.header(h).and_then(|v| v.trim().parse::<u64>().ok());
            match (num("retry-after"), num("x-ratelimit-reset"), r.header("x-ratelimit-remaining")) {
                (Some(s), _, _) => Got::Limited(now() + s),
                (_, Some(t), Some("0")) => Got::Limited(t),
                _ if code == 429 => Got::Limited(now() + 3600),
                _ => Got::Failed(code),
            }
        }
        Err(ureq::Error::Status(code, _)) => Got::Failed(code),
        Err(ureq::Error::Transport(_)) => Got::Offline,
    }
}

// ---------- state shown in the window ----------

#[derive(Default)]
struct St {
    /// idle | checking | none | available | downloading | verifying | ready | installing | error
    phase: &'static str,
    latest: Option<Rel>,
    /// skipped or "remind me later": no card or dot until a manual check
    hidden: bool,
    got: u64,
    total: u64,
    /// the answer to a Check now press (empty for automatic checks)
    note: String,
    error: String,
    file: Option<(PathBuf, String)>,
    limited_until: u64,
}
static ST: Mutex<St> = Mutex::new(St { phase: "idle", latest: None, hidden: false, got: 0, total: 0, note: String::new(), error: String::new(), file: None, limited_until: 0 });
static CANCEL: AtomicBool = AtomicBool::new(false);
static BUSY: AtomicBool = AtomicBool::new(false);

/// Installed with the installer (it leaves uninstall.exe beside the app)? A portable copy
/// can't be updated in place, so the card offers the release page instead.
fn installed() -> bool {
    std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("uninstall.exe").exists())).unwrap_or(false)
}

fn snapshot() -> Value {
    let s = ST.lock().unwrap();
    let c = config::get();
    json!({
        "phase": s.phase, "installed": CURRENT, "hidden": s.hidden, "got": s.got, "total": s.total,
        "note": s.note, "error": s.error, "portable": !installed(),
        "auto": c.update_auto, "beta": c.update_beta, "lastCheck": c.update_last_check,
        "latest": s.latest.as_ref().map(|r| json!({ "version": r.version.to_string(), "date": r.date, "notes": r.notes, "page": r.page, "pre": r.pre })),
    })
}
fn emit() {
    if let Some(a) = crate::APP.get() { let _ = a.emit("update", snapshot()); }
}
fn set(f: impl FnOnce(&mut St)) { f(&mut ST.lock().unwrap()); emit(); }

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Cache { url: String, etag: String, body: String }
fn cache_path() -> PathBuf { config::path("update-cache.json") }
fn load_cache() -> Cache { std::fs::read_to_string(cache_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default() }

/// Ask GitHub (or reuse the cached answer) and update the card. `manual` is a Check now press:
/// it always asks, shows skipped versions again, and is the only kind that reports problems.
pub fn check(manual: bool, cache_only: bool) {
    let c = config::get();
    if !manual && !c.update_auto { return; }
    if !matches!(ST.lock().unwrap().phase, "idle" | "none" | "available" | "error") { return; } // mid-download
    if BUSY.swap(true, SeqCst) { return; }
    let limited = ST.lock().unwrap().limited_until;
    if limited > now() {
        if manual { set(|s| s.note = strings::update_limited((limited - now()).div_ceil(60))); }
        BUSY.store(false, SeqCst);
        return;
    }
    if manual { set(|s| { s.phase = "checking"; s.note.clear(); }); }
    let base = api();
    let url = if c.update_beta { format!("{base}/repos/{REPO}/releases?per_page=20") } else { format!("{base}/repos/{REPO}/releases/latest") };
    let cache = load_cache();
    let etag = if cache.url == url { cache.etag.as_str() } else { "" };
    let got = if cache_only && cache.url == url && !cache.body.is_empty() { Got::NotModified } else { get_json(&url, etag) };
    let body = match got {
        Got::Fresh { etag, body } => {
            let _ = std::fs::write(cache_path(), serde_json::to_string(&Cache { url: url.clone(), etag, body: body.clone() }).unwrap_or_default());
            Some(body)
        }
        Got::NotModified if cache.url == url => Some(cache.body),
        Got::NotModified => None,
        Got::Missing => Some("[]".into()),
        Got::Limited(t) => { set(|s| { s.limited_until = t; if manual { s.note = strings::update_limited(t.saturating_sub(now()).div_ceil(60)); } }); None }
        Got::Failed(code) => { if manual { set(|s| s.note = strings::update_server_error(code)); } None }
        Got::Offline => { if manual { set(|s| s.note = strings::UPDATE_OFFLINE.into()); } None }
    };
    if let Some(body) = body {
        if !cache_only { config::update(|c| c.update_last_check = now()); }
        let current = semver::Version::parse(CURRENT).expect("build version is semver");
        let rel = serde_json::from_str::<Value>(&body).ok().and_then(|v| pick(&v, c.update_beta)).filter(|r| r.version > current);
        let c = config::get();
        set(|s| match rel {
            Some(r) => {
                s.hidden = quiet(manual, &c.update_skipped, c.update_later_until, &r.version.to_string(), now());
                if s.latest.as_ref().map(|l| l.version != r.version).unwrap_or(true) { s.file = None; }
                s.latest = Some(r);
                s.phase = "available";
                s.error.clear();
                if manual { s.note.clear(); }
            }
            None => { s.latest = None; s.phase = "none"; if manual { s.note = strings::up_to_date(CURRENT); } }
        });
    } else if manual {
        set(|s| if s.phase == "checking" { s.phase = if s.latest.is_some() { "available" } else { "idle" }; });
    }
    BUSY.store(false, SeqCst);
}

/// No card and no dot: the version was skipped or "Remind me later" is still running. Check now shows it anyway.
fn quiet(manual: bool, skipped: &[String], later_until: u64, version: &str, now: u64) -> bool {
    !manual && (skipped.iter().any(|v| v == version) || later_until > now)
}

/// After startup (never blocking it), then every 6 hours.
pub fn start() {
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(5));
        let recent = now().saturating_sub(config::get().update_last_check) < FRESH;
        check(false, recent);
        loop {
            std::thread::sleep(Duration::from_secs(600));
            if now().saturating_sub(config::get().update_last_check) >= EVERY { check(false, false); }
        }
    });
}

// ---------- download and verify ----------

pub enum Sig { Valid, Unsigned, Bad }

/// Authenticode check of a downloaded file (no revocation lookup, no UI).
pub fn signature(p: &Path) -> Sig {
    use windows::core::{GUID, HSTRING, PCWSTR};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Security::WinTrust::*;
    let path = HSTRING::from(p.as_os_str());
    let mut file = WINTRUST_FILE_INFO { cbStruct: std::mem::size_of::<WINTRUST_FILE_INFO>() as u32, pcwszFilePath: PCWSTR(path.as_ptr()), ..Default::default() };
    let mut data = WINTRUST_DATA {
        cbStruct: std::mem::size_of::<WINTRUST_DATA>() as u32, dwUIChoice: WTD_UI_NONE, fdwRevocationChecks: WTD_REVOKE_NONE,
        dwUnionChoice: WTD_CHOICE_FILE, dwStateAction: WTD_STATEACTION_VERIFY, Anonymous: WINTRUST_DATA_0 { pFile: &mut file }, ..Default::default()
    };
    let mut action: GUID = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    let r = unsafe { WinVerifyTrust(HWND::default(), &mut action, &mut data as *mut _ as _) };
    data.dwStateAction = WTD_STATEACTION_CLOSE;
    unsafe { WinVerifyTrust(HWND::default(), &mut action, &mut data as *mut _ as _) };
    match r as u32 { 0 => Sig::Valid, 0x800B_0100 => Sig::Unsigned, _ => Sig::Bad }
}

fn sha256_file(p: &Path) -> Option<String> {
    let mut f = std::fs::File::open(p).ok()?;
    let mut h = Sha256::new();
    std::io::copy(&mut f, &mut h).ok()?;
    Some(format!("{:x}", h.finalize()))
}

pub const CANCELLED: &str = "cancelled";

/// Download the release's installer into `dir`, checking it against SHA256SUMS and its
/// signature. On any failure the partial or bad file is deleted. Returns (path, sha256).
pub fn download(rel: &Rel, dir: &Path, test: Option<&str>, cancel: &AtomicBool, mut progress: impl FnMut(u64, u64)) -> Result<(PathBuf, String), String> {
    let (name, url, size) = rel.installer.clone().ok_or(strings::UPDATE_NO_INSTALLER)?;
    let sums_url = rel.sums.clone().ok_or(strings::UPDATE_NO_SUMS)?;
    if !allowed(&url, test) || !allowed(&sums_url, test) || name.contains(['/', '\\', ':']) || name.contains("..") {
        return Err(strings::UPDATE_BAD_SOURCE.into());
    }
    let sums = agent(&sums_url).get(&sums_url).call().map_err(|e| strings::update_download_failed(e))?.into_string().map_err(|e| strings::update_download_failed(e))?;
    let want = sum_for(&sums, &name).ok_or(strings::UPDATE_NOT_IN_SUMS)?;
    std::fs::create_dir_all(dir).map_err(|e| strings::update_download_failed(e))?;
    let part = dir.join(format!("{name}.part"));
    let fin = dir.join(&name);
    let fail = |e: String| { let _ = std::fs::remove_file(&part); e };
    let resp = agent(&url).get(&url).call().map_err(|e| strings::update_download_failed(e))?;
    let total = resp.header("content-length").and_then(|v| v.parse().ok()).unwrap_or(size);
    let mut r = resp.into_reader();
    let mut f = std::fs::File::create(&part).map_err(|e| strings::update_download_failed(e))?;
    let (mut h, mut got, mut buf) = (Sha256::new(), 0u64, vec![0u8; 64 * 1024]);
    loop {
        if cancel.load(SeqCst) { drop(f); return Err(fail(CANCELLED.into())); }
        let n = match r.read(&mut buf) { Ok(n) => n, Err(e) => { drop(f); return Err(fail(strings::update_download_failed(e))); } };
        if n == 0 { break; }
        if let Err(e) = f.write_all(&buf[..n]) { drop(f); return Err(fail(strings::update_download_failed(e))); }
        h.update(&buf[..n]);
        got += n as u64;
        progress(got, total);
    }
    drop(f);
    let hex = format!("{:x}", h.finalize());
    if hex != want { return Err(fail(strings::UPDATE_BAD_CHECKSUM.into())); }
    let _ = std::fs::remove_file(&fin);
    std::fs::rename(&part, &fin).map_err(|e| fail(strings::update_download_failed(e)))?;
    match signature(&fin) {
        Sig::Valid => {}
        Sig::Unsigned if !REQUIRE_SIGNATURE => {}
        _ => { let _ = std::fs::remove_file(&fin); return Err(strings::UPDATE_BAD_SIGNATURE.into()); }
    }
    Ok((fin, hex))
}

// ---------- commands ----------

#[tauri::command]
pub fn update_state() -> Value { snapshot() }

#[tauri::command]
pub async fn update_check() -> Value {
    let _ = tauri::async_runtime::spawn_blocking(|| check(true, false)).await;
    snapshot()
}

/// Install pressed: download and verify in the background; the card shows progress.
#[tauri::command]
pub fn update_install() -> Result<(), String> {
    if crate::brain::busy() { return Err(strings::UPDATE_TASK_RUNNING.into()); }
    let rel = {
        let mut s = ST.lock().unwrap();
        if !matches!(s.phase, "available" | "error") { return Ok(()); }
        let rel = s.latest.clone().ok_or(strings::UPDATE_NOT_READY)?;
        s.phase = "downloading"; s.got = 0; s.total = rel.installer.as_ref().map(|i| i.2).unwrap_or(0); s.error.clear(); s.note.clear();
        rel
    };
    emit();
    CANCEL.store(false, SeqCst);
    std::thread::spawn(move || {
        let dir = std::env::temp_dir().join("reach-update");
        let mut last = 0u64;
        let r = download(&rel, &dir, test_base().as_deref(), &CANCEL, |got, total| {
            if got - last >= 256 * 1024 || got == total { last = got; set(|s| { s.got = got; s.total = total; if got == total { s.phase = "verifying"; } }); }
        });
        match r {
            Ok(f) => { config::log(&format!("update: {} downloaded and verified", rel.version)); set(|s| { s.file = Some(f); s.phase = "ready"; }); }
            Err(e) if e == CANCELLED => set(|s| { s.phase = "available"; s.got = 0; }),
            Err(e) => { config::log(&format!("update: {e}")); set(|s| { s.phase = "error"; s.error = e; }); }
        }
    });
    Ok(())
}

#[tauri::command]
pub fn update_cancel() {
    CANCEL.store(true, SeqCst);
    set(|s| if s.phase == "ready" { if let Some((f, _)) = s.file.take() { let _ = std::fs::remove_file(f); } s.phase = "available"; });
}

/// Continue pressed after "Reach will close and install version X": check the file once more,
/// start the installer, and quit so it can replace the app. It relaunches Reach when done.
#[tauri::command]
pub fn update_run(app: tauri::AppHandle) -> Result<(), String> {
    if crate::brain::busy() { return Err(strings::UPDATE_TASK_RUNNING.into()); }
    let (file, hex) = ST.lock().unwrap().file.clone().ok_or(strings::UPDATE_NOT_READY)?;
    if sha256_file(&file).as_deref() != Some(hex.as_str()) {
        let _ = std::fs::remove_file(&file);
        set(|s| { s.file = None; s.phase = "error"; s.error = strings::UPDATE_BAD_CHECKSUM.into(); });
        return Err(strings::UPDATE_BAD_CHECKSUM.into());
    }
    set(|s| s.phase = "installing");
    match std::process::Command::new(&file).args(["/P", "/UPDATE", "/R"]).spawn() {
        Ok(p) => { config::log(&format!("update: installer started (pid {}), closing", p.id())); app.exit(0); Ok(()) }
        Err(e) => { let m = strings::update_run_failed(e); set(|s| { s.phase = "error"; s.error = m.clone(); }); Err(m) }
    }
}

#[tauri::command]
pub fn update_skip() {
    let v = ST.lock().unwrap().latest.as_ref().map(|r| r.version.to_string());
    if let Some(v) = v { config::update(|c| if !c.update_skipped.contains(&v) { c.update_skipped.push(v) }); }
    set(|s| { s.hidden = true; s.note.clear(); });
}

/// Remind me later: quiet for a day.
#[tauri::command]
pub fn update_later() {
    config::update(|c| c.update_later_until = now() + 24 * 3600);
    set(|s| { s.hidden = true; s.note.clear(); if s.phase == "error" { s.phase = "available"; s.error.clear(); } });
}

#[tauri::command]
pub fn update_open() -> Result<(), String> {
    let page = ST.lock().unwrap().latest.as_ref().map(|r| r.page.clone()).unwrap_or_else(|| format!("https://github.com/{REPO}/releases"));
    if !page_ok(&page) { return Err(strings::UPDATE_BAD_SOURCE.into()); }
    std::process::Command::new("explorer.exe").arg(&page).spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufRead;
    use std::net::TcpListener;

    /// A tiny HTTP server: `routes(path, if_none_match)` -> (status, headers, body).
    fn serve(routes: impl Fn(&str, &str) -> (u16, Vec<(&'static str, String)>, Vec<u8>) + Send + 'static) -> String {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://127.0.0.1:{}", l.local_addr().unwrap().port());
        std::thread::spawn(move || for s in l.incoming().flatten() {
            let mut r = std::io::BufReader::new(s.try_clone().unwrap());
            let mut line = String::new(); let _ = r.read_line(&mut line);
            let path = line.split_whitespace().nth(1).unwrap_or("").to_string();
            let mut inm = String::new();
            loop { let mut h = String::new(); if r.read_line(&mut h).unwrap_or(0) <= 2 { break; } if let Some(v) = h.to_ascii_lowercase().strip_prefix("if-none-match:") { inm = v.trim().to_string(); } }
            let (code, hs, body) = routes(&path, &inm);
            let mut w = s;
            let mut head = format!("HTTP/1.1 {code} X\r\nContent-Length: {}\r\nConnection: close\r\n", body.len());
            for (k, v) in hs { head.push_str(&format!("{k}: {v}\r\n")); }
            let _ = w.write_all(head.as_bytes()); let _ = w.write_all(b"\r\n"); let _ = w.write_all(&body);
        });
        base
    }

    fn rel_json(base: &str, tag: &str, pre: bool, draft: bool) -> Value {
        json!({ "tag_name": tag, "draft": draft, "prerelease": pre, "published_at": "2026-12-01T10:00:00Z", "body": "<b>Fixes</b>", "html_url": format!("{base}/rel"),
            "assets": [ { "name": format!("Reach_{}_x64-setup.exe", tag.trim_start_matches('v')), "browser_download_url": format!("{base}/dl/setup.exe"), "size": 5 },
                        { "name": "SHA256SUMS.txt", "browser_download_url": format!("{base}/dl/SHA256SUMS.txt"), "size": 1 } ] })
    }

    #[test]
    fn picks_newest_and_skips_drafts_and_betas() {
        let b = "http://127.0.0.1:1";
        let list = json!([rel_json(b, "v0.3.1", false, false), rel_json(b, "v0.4.0-beta.1", true, false), rel_json(b, "v0.9.0", false, true), rel_json(b, "nightly", false, false)]);
        assert_eq!(pick(&list, false).unwrap().version.to_string(), "0.3.1");
        assert_eq!(pick(&list, true).unwrap().version.to_string(), "0.4.0-beta.1");
        // semver, not string order: 0.10.0 > 0.9.0, and 0.4.0 > 0.4.0-beta.1
        assert!(semver::Version::parse("0.10.0").unwrap() > semver::Version::parse("0.9.0").unwrap());
        assert!(semver::Version::parse("0.4.0").unwrap() > semver::Version::parse("0.4.0-beta.1").unwrap());
        // a prerelease flag on a plain version still counts as a beta
        assert!(pick(&json!(rel_json(b, "v1.0.0", true, false)), false).is_none());
    }

    #[test]
    fn skipped_and_later() {
        let sk = vec!["0.3.1".to_string()];
        assert!(quiet(false, &sk, 0, "0.3.1", 100));          // skipped
        assert!(!quiet(false, &sk, 0, "0.3.2", 100));         // a newer one shows again
        assert!(quiet(false, &[], 200, "0.3.2", 100));        // remind me later, still within the day
        assert!(!quiet(false, &[], 200, "0.3.2", 300));       // the day is over
        assert!(!quiet(true, &sk, 200, "0.3.1", 100));        // Check now always shows it
    }

    #[test]
    fn sums_and_sources() {
        let h = "a".repeat(64);
        assert_eq!(sum_for(&format!("{h}  Reach_0.4.0_x64-setup.exe\n{}  other.exe", "b".repeat(64)), "Reach_0.4.0_x64-setup.exe"), Some(h.clone()));
        assert_eq!(sum_for(&format!("{h} *x.exe"), "x.exe"), Some(h));
        assert_eq!(sum_for("zz  x.exe", "x.exe"), None);
        assert!(allowed(&format!("https://github.com/{REPO}/releases/download/v1/x.exe"), None));
        assert!(!allowed("https://github.com/someone/else/releases/download/v1/x.exe", None));
        assert!(!allowed(&format!("http://github.com/{REPO}/releases/download/v1/x.exe"), None));
        assert!(!allowed("https://evil.example/x.exe", Some("http://127.0.0.1:9")));
    }

    #[test]
    fn api_answers() {
        let base = serve(|p, inm| match p {
            "/latest" if inm == "\"e1\"" => (304, vec![], vec![]),
            "/latest" => (200, vec![("ETag", "\"e1\"".into())], b"{}".to_vec()),
            "/limited" => (403, vec![("X-RateLimit-Remaining", "0".into()), ("X-RateLimit-Reset", "4000000000".into())], vec![]),
            "/slow-down" => (429, vec![("Retry-After", "60".into())], vec![]),
            "/none" => (404, vec![], vec![]),
            _ => (500, vec![], vec![]),
        });
        assert_eq!(get_json(&format!("{base}/latest"), ""), Got::Fresh { etag: "\"e1\"".into(), body: "{}".into() });
        assert_eq!(get_json(&format!("{base}/latest"), "\"e1\""), Got::NotModified);
        assert_eq!(get_json(&format!("{base}/limited"), ""), Got::Limited(4_000_000_000));
        assert!(matches!(get_json(&format!("{base}/slow-down"), ""), Got::Limited(t) if t >= now() + 59));
        assert_eq!(get_json(&format!("{base}/none"), ""), Got::Missing);
        assert_eq!(get_json(&format!("{base}/boom"), ""), Got::Failed(500));
        // nothing listening: offline
        let dead = { let l = TcpListener::bind("127.0.0.1:0").unwrap(); l.local_addr().unwrap().port() };
        assert_eq!(get_json(&format!("http://127.0.0.1:{dead}/x"), ""), Got::Offline);
    }

    #[test]
    fn download_checks_the_hash_and_cancels() {
        let payload = std::fs::read(std::env::current_exe().unwrap()).unwrap(); // a real, unsigned program
        let good = format!("{:x}", Sha256::digest(&payload));
        let base = serve(move |p, _| match p {
            "/dl/setup.exe" => (200, vec![], payload.clone()),
            "/dl/SHA256SUMS.txt" => (200, vec![], format!("{good}  Reach_0.4.0_x64-setup.exe\n").into_bytes()),
            "/bad/SHA256SUMS.txt" => (200, vec![], format!("{}  Reach_0.4.0_x64-setup.exe\n", "0".repeat(64)).into_bytes()),
            "/big/setup.exe" => (200, vec![], vec![0u8; 4 * 1024 * 1024]),
            _ => (404, vec![], vec![]),
        });
        let dir = std::env::temp_dir().join(format!("reach-update-test-{}", std::process::id()));
        let rel = parse_rel(&rel_json(&base, "v0.4.0", false, false)).unwrap();
        let no = AtomicBool::new(false);
        let (f, _) = download(&rel, &dir, Some(&base), &no, |_, _| {}).unwrap();
        assert!(f.exists());

        let mut bad = rel.clone(); bad.sums = Some(format!("{base}/bad/SHA256SUMS.txt"));
        assert_eq!(download(&bad, &dir, Some(&base), &no, |_, _| {}).unwrap_err(), strings::UPDATE_BAD_CHECKSUM);
        assert!(!dir.join("Reach_0.4.0_x64-setup.exe.part").exists());

        let mut big = rel.clone(); big.installer = Some(("Reach_0.4.0_x64-setup.exe".into(), format!("{base}/big/setup.exe"), 0));
        let stop = AtomicBool::new(false);
        let r = download(&big, &dir, Some(&base), &stop, |got, _| if got > 512 * 1024 { stop.store(true, SeqCst) });
        assert_eq!(r.unwrap_err(), CANCELLED);
        assert!(!dir.join("Reach_0.4.0_x64-setup.exe.part").exists());

        let mut elsewhere = rel.clone(); elsewhere.installer = Some(("x.exe".into(), "https://evil.example/x.exe".into(), 0));
        assert_eq!(download(&elsewhere, &dir, Some(&base), &no, |_, _| {}).unwrap_err(), strings::UPDATE_BAD_SOURCE);
        let mut nosums = rel.clone(); nosums.sums = None;
        assert_eq!(download(&nosums, &dir, Some(&base), &no, |_, _| {}).unwrap_err(), strings::UPDATE_NO_SUMS);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
