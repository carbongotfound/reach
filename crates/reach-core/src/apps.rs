//! Windows and apps: enumerate, open (shortcut / App Paths / shell), focus, close,
//! minimize/maximize/restore, and elevation (admin) detection.

use crate::dpi::Rect;
use crate::util::wide;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use windows::core::{BOOL, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, RECT, WPARAM};
use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows::Win32::System::Threading::*;
use windows::Win32::UI::Shell::{ShellExecuteW, SHGetKnownFolderPath, FOLDERID_CommonStartMenu, FOLDERID_Desktop, FOLDERID_PublicDesktop, FOLDERID_RoamingAppData, FOLDERID_StartMenu, KF_FLAG_DEFAULT};
use windows::Win32::UI::WindowsAndMessaging::*;

#[derive(Clone, Debug)]
pub struct WinInfo {
    pub h: isize,
    pub title: String,
    pub process: String,
    pub pid: u32,
    pub bounds: Rect,
    pub focused: bool,
    pub minimized: bool,
    pub maximized: bool,
    /// The window's process runs as administrator and we don't.
    pub elevated: bool,
}

pub fn hwnd(h: isize) -> HWND { HWND(h as *mut _) }

/// The top-level window a click at this screen point would land on (click-through overlays don't count).
pub fn top_at(x: i32, y: i32) -> isize {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::{GetAncestor, WindowFromPoint, GA_ROOT};
    unsafe { GetAncestor(WindowFromPoint(POINT { x, y }), GA_ROOT).0 as isize }
}

/// Visible bounds of a window in screen px: DWM's frame, without the invisible resize border.
pub fn frame_rect(h: isize) -> Option<crate::dpi::Rect> {
    use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
    let mut r = RECT::default();
    let ok = unsafe { DwmGetWindowAttribute(hwnd(h), DWMWA_EXTENDED_FRAME_BOUNDS, &mut r as *mut _ as _, std::mem::size_of::<RECT>() as u32) }.is_ok()
        || unsafe { GetWindowRect(hwnd(h), &mut r) }.is_ok();
    if ok && r.right > r.left { Some(crate::dpi::Rect::from_ltrb(r.left, r.top, r.right, r.bottom)) } else { None }
}
pub fn fmt_hwnd(h: isize) -> String { format!("0x{:X}", h) }

/// Test fence (REACH_TEST_SCREEN set): the windows a test may see and touch. The PowerShell
/// fixtures and their dialogs, plus what the test names itself: processes in REACH_TEST_PIDS and
/// windows in REACH_TEST_HWNDS (comma lists; real apps the test opened, and UWP apps, which share
/// the ApplicationFrameHost process with every other UWP app). Off without REACH_TEST_SCREEN.
pub fn fence_on() -> bool { std::env::var("REACH_TEST_SCREEN").is_ok_and(|t| !t.is_empty()) }
/// A window the test named (REACH_TEST_PIDS / REACH_TEST_HWNDS), or a dialog it owns (Save As, menus).
pub fn test_listed(w: &WinInfo) -> bool {
    let list = |k: &str| std::env::var(k).unwrap_or_default();
    let named: Vec<i64> = list("REACH_TEST_HWNDS").split(',').map(str::trim).filter_map(|h| match h.strip_prefix("0x").or_else(|| h.strip_prefix("0X")) {
        Some(x) => i64::from_str_radix(x, 16).ok(), None => h.parse().ok() }).collect();
    let mut h = w.h;
    for _ in 0..8 {
        if named.contains(&(h as i64)) { return true; }
        h = unsafe { GetWindow(hwnd(h), GW_OWNER) }.map(|o| o.0 as isize).unwrap_or(0);
        if h == 0 { break; }
    }
    list("REACH_TEST_PIDS").split(',').any(|p| p.trim().parse::<u32>().ok() == Some(w.pid))
}

/// Test fence: a picture of `src` (screen px) at out_w x out_h with only the test windows in it, each
/// drawn by itself (PrintWindow), on a plain dark background: the PowerShell fixture titled
/// REACH_TEST_SCREEN and its dialogs, and the windows the test named (test_listed). Other windows,
/// even ones on top, never get in. Err when no test window is in `src`.
pub fn grab_fenced(src: Rect, out_w: i32, out_h: i32) -> Result<Vec<u8>, String> {
    let mut out: Vec<u8> = std::iter::repeat([0x2a, 0x2a, 0x2a, 0xff]).take((out_w * out_h) as usize).flatten().collect();
    let (kx, ky) = (out_w as f64 / src.w as f64, out_h as f64 / src.h as f64);
    let t = std::env::var("REACH_TEST_SCREEN").unwrap_or_default().to_lowercase();
    let all = enumerate();
    let pid = all.iter().find(|x| x.process.eq_ignore_ascii_case("powershell") && !t.is_empty() && x.title.to_lowercase().contains(&t)).map(|x| x.pid);
    let mut any = false;
    // EnumWindows goes top to bottom: paint bottom first
    for win in all.iter().filter(|x| (Some(x.pid) == pid || test_listed(x)) && !x.minimized).rev() {
        let b = frame_rect(win.h).unwrap_or(win.bounds);
        let (x0, y0, x1, y1) = (b.x.max(src.x), b.y.max(src.y), b.right().min(src.right()), b.bottom().min(src.bottom()));
        if x1 <= x0 || y1 <= y0 { continue; }
        // the whole window scaled, then the part inside src
        let (fw, fh) = (((b.w as f64) * kx).round().max(1.0) as i32, ((b.h as f64) * ky).round().max(1.0) as i32);
        let Ok(px) = crate::capture::grab_window(win.h, b, fw, fh) else { continue };
        // a window that is closing (a dialog just dismissed) draws all black: leave it out
        if px.chunks_exact(4).all(|p| p[0] < 8 && p[1] < 8 && p[2] < 8) { continue; }
        let (dx, dy) = ((((x0 - src.x) as f64) * kx) as i32, (((y0 - src.y) as f64) * ky) as i32);
        let (ox, oy) = ((((x0 - b.x) as f64) * kx) as i32, (((y0 - b.y) as f64) * ky) as i32);
        let dw = ((((x1 - x0) as f64) * kx).round() as i32).min(fw - ox).min(out_w - dx);
        let dh = ((((y1 - y0) as f64) * ky).round() as i32).min(fh - oy).min(out_h - dy);
        if dw < 1 || dh < 1 { continue; }
        any = true;
        for yy in 0..dh {
            let s = (((oy + yy) * fw + ox) * 4) as usize;
            let d = (((dy + yy) * out_w + dx) * 4) as usize;
            out[d..d + dw as usize * 4].copy_from_slice(&px[s..s + dw as usize * 4]);
        }
    }
    if any { Ok(out) } else { Err("No test window here.".into()) }
}
pub fn is_fixture(w: &WinInfo) -> bool { w.process.eq_ignore_ascii_case("powershell") || test_listed(w) }

pub fn title_of(h: HWND) -> String {
    unsafe {
        let n = GetWindowTextLengthW(h);
        if n <= 0 { return String::new(); }
        let mut buf = vec![0u16; n as usize + 1];
        let got = GetWindowTextW(h, &mut buf);
        String::from_utf16_lossy(&buf[..got as usize])
    }
}

/// (exe stem, elevated). Elevated = we couldn't open it, or its token says so.
pub fn process_info(pid: u32) -> (String, bool) {
    unsafe {
        let Ok(p) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return (String::new(), !self_elevated()) };
        let mut buf = [0u16; 1024];
        let mut n = buf.len() as u32;
        let name = if QueryFullProcessImageNameW(p, PROCESS_NAME_WIN32, windows::core::PWSTR(buf.as_mut_ptr()), &mut n).is_ok() {
            Path::new(&String::from_utf16_lossy(&buf[..n as usize])).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
        } else { String::new() };
        let elev = token_elevated(p) && !self_elevated();
        let _ = CloseHandle(p);
        (name, elev)
    }
}

fn token_elevated(p: HANDLE) -> bool {
    unsafe {
        let mut tok = HANDLE::default();
        if windows::Win32::System::Threading::OpenProcessToken(p, TOKEN_QUERY, &mut tok).is_err() {
            return false;
        }
        let mut e = TOKEN_ELEVATION::default();
        let mut len = 0u32;
        let ok = GetTokenInformation(tok, TokenElevation, Some(&mut e as *mut _ as *mut _), std::mem::size_of::<TOKEN_ELEVATION>() as u32, &mut len).is_ok();
        let _ = CloseHandle(tok);
        ok && e.TokenIsElevated != 0
    }
}

pub fn self_elevated() -> bool {
    static V: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *V.get_or_init(|| unsafe { token_elevated(GetCurrentProcess()) })
}

pub fn enumerate() -> Vec<WinInfo> {
    unsafe extern "system" fn cb(h: HWND, l: LPARAM) -> BOOL {
        let list = &mut *(l.0 as *mut Vec<WinInfo>);
        if !IsWindowVisible(h).as_bool() { return BOOL(1); }
        let ex = GetWindowLongW(h, GWL_EXSTYLE) as u32;
        if ex & WS_EX_TOOLWINDOW.0 != 0 || ex & WS_EX_NOACTIVATE.0 != 0 { return BOOL(1); }
        let mut pid = 0u32;
        GetWindowThreadProcessId(h, Some(&mut pid));
        if pid == 0 { return BOOL(1); }
        let title = title_of(h);
        let (process, elevated) = process_info(pid);
        if title.trim().is_empty() && process.is_empty() { return BOOL(1); }
        let mut r = RECT::default();
        if GetWindowRect(h, &mut r).is_err() { return BOOL(1); }
        let bounds = Rect::from_ltrb(r.left, r.top, r.right, r.bottom);
        if bounds.w < 10 || bounds.h < 10 { return BOOL(1); }
        list.push(WinInfo {
            h: h.0 as isize, title, process, pid, bounds,
            focused: h == GetForegroundWindow(),
            minimized: IsIconic(h).as_bool(),
            maximized: IsZoomed(h).as_bool(),
            elevated,
        });
        BOOL(1)
    }
    let mut list = Vec::new();
    unsafe { let _ = EnumWindows(Some(cb), LPARAM(&mut list as *mut _ as isize)); }
    list
}

pub fn force_focus(h: HWND) {
    unsafe {
        if h.0.is_null() || !IsWindow(Some(h)).as_bool() { return; }
        if IsIconic(h).as_bool() { let _ = ShowWindow(h, SW_RESTORE); }
        let fg = GetWindowThreadProcessId(GetForegroundWindow(), None);
        let me = GetCurrentThreadId();
        if fg != me { let _ = windows::Win32::System::Threading::AttachThreadInput(me, fg, true); }
        let _ = BringWindowToTop(h);
        let _ = SetForegroundWindow(h);
        if fg != me { let _ = windows::Win32::System::Threading::AttachThreadInput(me, fg, false); }
    }
}

pub fn close(h: HWND) { unsafe { let _ = PostMessageW(Some(h), WM_CLOSE, WPARAM(0), LPARAM(0)); } }
pub fn show(h: HWND, cmd: SHOW_WINDOW_CMD) { unsafe { let _ = ShowWindow(h, cmd); } }
pub fn is_window(h: HWND) -> bool { unsafe { IsWindow(Some(h)).as_bool() } }

pub fn looks_like_hwnd(s: &str) -> bool {
    if s.starts_with("0x") || s.starts_with("0X") { return true; }
    s.parse::<i64>().map(|n| n > 0 && (s.len() >= 5 || n > 0x10000)).unwrap_or(false)
}

pub fn parse_hwnd(s: &str) -> Result<isize, String> {
    let s = s.trim();
    if s.is_empty() { return Err("Window handle is required.".into()); }
    let v = if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) { i64::from_str_radix(hex, 16) } else { s.parse() };
    let v = v.map_err(|_| format!("Bad window handle `{s}`."))?;
    if !is_window(hwnd(v as isize)) { return Err(format!("Not a live window: {s}")); }
    Ok(v as isize)
}

/// Handle or name. Prefers exact process, then the focused match, then the first.
pub fn resolve(w: &str) -> Result<isize, String> {
    if w.is_empty() { return Err("`window` (handle or name) is required.".into()); }
    if looks_like_hwnd(w) { return parse_hwnd(w); }
    let f = w.to_lowercase();
    let hits: Vec<WinInfo> = enumerate().into_iter().filter(|i| title_key(&i.title).contains(&f) || i.process.to_lowercase().contains(&f)).collect();
    if hits.is_empty() { return Err(format!("No open window matching `{w}`.")); }
    if let Some(h) = hits.iter().find(|h| h.process.eq_ignore_ascii_case(w)) { return Ok(h.h); }
    if let Some(h) = hits.iter().find(|h| h.focused) { return Ok(h.h); }
    Ok(hits[0].h)
}

pub fn open(name: &str, fresh: bool) -> Result<String, String> {
    let name = name.trim();
    if !fresh {
        let f = name.to_lowercase();
        let hits: Vec<WinInfo> = enumerate().into_iter().filter(|i| title_key(&i.title).contains(&f)
            || i.process.to_lowercase() == f || i.process.to_lowercase().replace(' ', "") == f.replace(' ', "")).collect();
        if !hits.is_empty() {
            let pick = hits.iter().find(|h| h.process.eq_ignore_ascii_case(name)).unwrap_or(&hits[0]);
            force_focus(hwnd(pick.h));
            return Ok(format!("already open, focused {}  {}  [{}]", fmt_hwnd(pick.h), pick.title, pick.process));
        }
    }
    let lower = name.to_lowercase();
    let target = if lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("file:") || Path::new(name).exists() {
        name.to_string()
    } else if let Some(l) = find_shortcut(name) {
        l.to_string_lossy().into_owned()
    } else if let Some(p) = find_app_path(name) {
        p
    } else if let Some(id) = find_start_app(name) {
        format!(r"shell:AppsFolder\{id}")
    } else {
        name.to_string()
    };
    launch(&target).map_err(|e| format!("Could not open `{name}`. {e}"))?;
    Ok(wait_for_new(name))
}

pub fn launch(file: &str) -> Result<(), String> {
    let dir = std::env::var("USERPROFILE").unwrap_or_default();
    let (f, d, op) = (wide(file), wide(&dir), wide("open"));
    let r = unsafe { ShellExecuteW(None, PCWSTR(op.as_ptr()), PCWSTR(f.as_ptr()), PCWSTR::null(), PCWSTR(d.as_ptr()), SW_SHOWNORMAL) };
    if r.0 as isize <= 32 { Err(format!("The system cannot find `{file}`.")) } else { Ok(()) }
}

fn wait_for_new(name: &str) -> String {
    let f = Path::new(name).file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_else(|| name.to_lowercase());
    for _ in 0..12 {
        std::thread::sleep(Duration::from_millis(250));
        if let Some(i) = enumerate().into_iter().find(|i| title_key(&i.title).contains(&f) || i.process.to_lowercase().contains(&f)) {
            force_focus(hwnd(i.h));
            return format!("opened {}  {}  [{}]", fmt_hwnd(i.h), i.title, i.process);
        }
    }
    format!("launched `{name}`, but no window showed up yet. Run `reach apps` to check.")
}

fn known(id: &windows::core::GUID) -> Option<PathBuf> {
    unsafe {
        let p = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None).ok()?;
        let s = p.to_string().ok();
        windows::Win32::System::Com::CoTaskMemFree(Some(p.0 as _));
        s.map(PathBuf::from)
    }
}

fn scan_shortcuts() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = [&FOLDERID_StartMenu, &FOLDERID_CommonStartMenu, &FOLDERID_Desktop, &FOLDERID_PublicDesktop].iter().filter_map(|g| known(g)).collect();
    if let Some(a) = known(&FOLDERID_RoamingAppData) {
        dirs.push(a.join(r"Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar"));
    }
    let mut out = Vec::new();
    fn walk(d: &Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(d) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() { walk(&p, out); } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("lnk")) { out.push(p); }
        }
    }
    for d in dirs { walk(&d, &mut out); }
    out
}

fn find_shortcut(name: &str) -> Option<PathBuf> {
    static CACHE: Mutex<Option<(Instant, Vec<PathBuf>)>> = Mutex::new(None);
    let mut c = CACHE.lock().unwrap();
    if c.as_ref().is_none_or(|(t, _)| t.elapsed() > Duration::from_secs(120)) {
        *c = Some((Instant::now(), scan_shortcuts()));
    }
    let needle = name.to_lowercase();
    let list = &c.as_ref().unwrap().1;
    let stem = |p: &PathBuf| p.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
    list.iter().find(|p| stem(p) == needle)
        .or_else(|| list.iter().find(|p| stem(p).starts_with(&needle)))
        .or_else(|| list.iter().find(|p| stem(p).contains(&needle)))
        .cloned()
}

/// Store and packaged apps (Calculator, Paint, Snipping Tool...) have no .lnk; they live in the
/// shell's Apps folder. Returns the app's AppUserModelID for `shell:AppsFolder\<id>`.
fn find_start_app(name: &str) -> Option<String> {
    static CACHE: Mutex<Option<(Instant, Vec<(String, String)>)>> = Mutex::new(None);
    let mut c = CACHE.lock().unwrap();
    if c.as_ref().is_none_or(|(t, _)| t.elapsed() > Duration::from_secs(120)) {
        *c = Some((Instant::now(), scan_start_apps()));
    }
    let needle = name.to_lowercase();
    let list = &c.as_ref().unwrap().1;
    list.iter().find(|(n, _)| *n == needle)
        .or_else(|| list.iter().find(|(n, _)| n.starts_with(&needle)))
        .or_else(|| list.iter().find(|(n, _)| n.contains(&needle)))
        .map(|(_, id)| id.clone())
}

/// (lowercase display name, AppUserModelID) for every entry in shell:AppsFolder.
fn scan_start_apps() -> Vec<(String, String)> {
    use windows::Win32::UI::Shell::{BHID_EnumItems, IEnumShellItems, IShellItem, SHCreateItemFromParsingName, SIGDN_NORMALDISPLAY, SIGDN_PARENTRELATIVEPARSING};
    let mut out = Vec::new();
    unsafe {
        let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
        let path = wide("shell:AppsFolder");
        let Ok(folder) = SHCreateItemFromParsingName::<_, _, IShellItem>(PCWSTR(path.as_ptr()), None) else { return out };
        let Ok(items) = folder.BindToHandler::<_, IEnumShellItems>(None, &BHID_EnumItems) else { return out };
        let name_of = |i: &IShellItem, k| i.GetDisplayName(k).ok().and_then(|p| {
            let s = p.to_string().ok();
            windows::Win32::System::Com::CoTaskMemFree(Some(p.0 as _));
            s
        });
        loop {
            let mut got = [None];
            if items.Next(&mut got, None).is_err() { break }
            let Some(item) = got[0].take() else { break };
            if let (Some(n), Some(id)) = (name_of(&item, SIGDN_NORMALDISPLAY), name_of(&item, SIGDN_PARENTRELATIVEPARSING)) {
                out.push((n.to_lowercase(), id));
            }
        }
    }
    out
}

fn find_app_path(name: &str) -> Option<String> {
    let mut key = format!(r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\{name}");
    if !name.to_lowercase().ends_with(".exe") { key.push_str(".exe"); }
    for root in [crate::reg::HKLM, crate::reg::HKCU] {
        if let Some((v, _)) = crate::reg::read(root, &key, "") {
            let v = v.trim_matches('"').to_string();
            if Path::new(&v).exists() { return Some(v); }
        }
    }
    None
}


/// Lowercase title for matching. Edge writes "Microsoft\u{200b} Edge" with a zero-width space.
fn title_key(t: &str) -> String { t.to_lowercase().replace('\u{200b}', "") }

#[cfg(test)]
mod tests {
    #[test]
    fn edge_title_matches() { assert!(super::title_key("New tab - Microsoft\u{200b} Edge").contains("microsoft edge")); }

    /// The test fence's own lists: a named PID or HWND is a fixture, anything else isn't (unless PowerShell).
    #[test]
    fn fence_lists() {
        let w = |h: isize, pid: u32, p: &str| super::WinInfo { h, pid, title: String::new(), process: p.into(),
            bounds: crate::dpi::Rect { x: 0, y: 0, w: 1, h: 1 }, focused: false, minimized: false, maximized: false, elevated: false };
        std::env::set_var("REACH_TEST_PIDS", "11, 22");
        std::env::set_var("REACH_TEST_HWNDS", "0x1A2B,");
        assert!(super::is_fixture(&w(1, 22, "notepad")));
        assert!(super::is_fixture(&w(0x1A2B, 5, "ApplicationFrameHost")));
        assert!(!super::is_fixture(&w(0x1A2C, 5, "ApplicationFrameHost")));
        assert!(super::is_fixture(&w(9, 9, "powershell")));
        std::env::remove_var("REACH_TEST_PIDS");
        std::env::remove_var("REACH_TEST_HWNDS");
        assert!(!super::is_fixture(&w(1, 22, "notepad")));
    }
}
