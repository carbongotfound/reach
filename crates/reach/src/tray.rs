//! Tray icon: Pause/Resume, Open usage log, Exit. Runs the message loop on the main thread.

use reach_core::{config, events, util::wide, Event};
use std::sync::atomic::{AtomicIsize, Ordering::SeqCst};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const WM_TRAY: u32 = WM_APP + 1;
const WM_REFRESH: u32 = WM_APP + 2;
const ID_PAUSE: usize = 1;
const ID_LOG: usize = 2;
const ID_EXIT: usize = 3;

static HWND_TRAY: AtomicIsize = AtomicIsize::new(0);
static USER_PAUSED: AtomicIsize = AtomicIsize::new(0);
static TASKBAR_CREATED: AtomicIsize = AtomicIsize::new(0);
static ICON: AtomicIsize = AtomicIsize::new(0);

fn tip() -> String {
    if USER_PAUSED.load(SeqCst) != 0 { "Reach: paused, you're using the PC".into() }
    else if config::get().paused { "Reach: paused".into() }
    else { "Reach: desktop control".into() }
}

fn nid(h: HWND) -> NOTIFYICONDATAW {
    let mut n = NOTIFYICONDATAW { cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32, hWnd: h, uID: 1, ..Default::default() };
    n.uFlags = NIF_ICON | NIF_TIP | NIF_MESSAGE;
    n.uCallbackMessage = WM_TRAY;
    n.hIcon = HICON(ICON.load(SeqCst) as *mut _);
    let t = wide(&tip());
    let len = t.len().min(n.szTip.len());
    n.szTip[..len].copy_from_slice(&t[..len]);
    n.szTip[n.szTip.len() - 1] = 0;
    n
}

/// Ask the tray to redraw its tooltip (safe from any thread).
pub fn refresh() {
    let h = HWND_TRAY.load(SeqCst);
    if h != 0 { unsafe { let _ = PostMessageW(Some(HWND(h as *mut _)), WM_REFRESH, WPARAM(0), LPARAM(0)); } }
}

unsafe extern "system" fn proc_(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match m {
        WM_TRAY if matches!(l.0 as u32, WM_RBUTTONUP | WM_LBUTTONUP) => { menu(h); LRESULT(0) }
        WM_REFRESH => { let _ = Shell_NotifyIconW(NIM_MODIFY, &nid(h)); LRESULT(0) }
        WM_COMMAND => {
            match w.0 & 0xFFFF {
                ID_PAUSE => { config::update(|c| c.paused = !c.paused); let _ = Shell_NotifyIconW(NIM_MODIFY, &nid(h)); }
                ID_LOG => {
                    let home = config::home();
                    let u = config::path("usage.jsonl");
                    if !u.exists() { let _ = std::fs::write(&u, ""); }
                    let _ = std::process::Command::new("explorer.exe").arg(home).spawn();
                }
                ID_EXIT => { let _ = Shell_NotifyIconW(NIM_DELETE, &nid(h)); std::process::exit(0); }
                _ => {}
            }
            LRESULT(0)
        }
        _ if m != 0 && m as isize == TASKBAR_CREATED.load(SeqCst) => { let _ = Shell_NotifyIconW(NIM_ADD, &nid(h)); LRESULT(0) }
        _ => DefWindowProcW(h, m, w, l),
    }
}

unsafe fn menu(h: HWND) {
    let Ok(m) = CreatePopupMenu() else { return };
    let paused = config::get().paused;
    let status = wide(if paused { "Reach is paused" } else { "Reach is running" });
    let pause = wide(if paused { "Resume" } else { "Pause" });
    let log = wide("Open usage log");
    let exit = wide("Exit");
    let _ = AppendMenuW(m, MF_STRING | MF_GRAYED, 0, PCWSTR(status.as_ptr()));
    let _ = AppendMenuW(m, MF_SEPARATOR, 0, PCWSTR::null());
    let _ = AppendMenuW(m, MF_STRING, ID_PAUSE, PCWSTR(pause.as_ptr()));
    let _ = AppendMenuW(m, MF_STRING, ID_LOG, PCWSTR(log.as_ptr()));
    let _ = AppendMenuW(m, MF_SEPARATOR, 0, PCWSTR::null());
    let _ = AppendMenuW(m, MF_STRING, ID_EXIT, PCWSTR(exit.as_ptr()));
    let mut p = POINT::default();
    let _ = GetCursorPos(&mut p);
    let _ = SetForegroundWindow(h);
    let _ = TrackPopupMenu(m, TPM_RIGHTBUTTON, p.x, p.y, Some(0), h, None);
    let _ = PostMessageW(Some(h), WM_NULL, WPARAM(0), LPARAM(0));
    let _ = DestroyMenu(m);
}


pub fn run() {
    unsafe {
        let class = wide("ReachTray");
        let wc = WNDCLASSW { lpfnWndProc: Some(proc_), lpszClassName: PCWSTR(class.as_ptr()), ..Default::default() };
        RegisterClassW(&wc);
        let title = wide("Reach");
        let Ok(h) = CreateWindowExW(WINDOW_EX_STYLE(0), PCWSTR(class.as_ptr()), PCWSTR(title.as_ptr()), WS_OVERLAPPED, 0, 0, 0, 0, None, None, None, None) else { return };
        HWND_TRAY.store(h.0 as isize, SeqCst);
        let tc = wide("TaskbarCreated");
        TASKBAR_CREATED.store(RegisterWindowMessageW(PCWSTR(tc.as_ptr())) as isize, SeqCst);
        // the icon follows the taskbar theme (#111 on light, white on dark); WM_REFRESH re-sends it
        let size = GetSystemMetrics(SM_CXSMICON);
        ICON.store(reach_core::brand::icon(reach_core::reg::taskbar_light(), size).0 as isize, SeqCst);
        let _ = Shell_NotifyIconW(NIM_ADD, &nid(h));
        reach_core::brand::watch(move |light| { ICON.store(reach_core::brand::icon(light, size).0 as isize, SeqCst); refresh(); });
        events::subscribe(|e| if let Event::UserPause { paused, .. } = e { USER_PAUSED.store(*paused as isize, SeqCst); refresh(); });
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
