//! The agent cursor and pill on the real display, over backgrounds of our own.
//! cargo run --release -p reach-core --example cursor_stage -- <out_dir> <x> <y> [bg.bmp ...]
//! Opens a stage window of our own at physical (x, y) (put it on a monitor nobody is using),
//! paints white, black, then each BMP in turn, walks the overlay through every state and saves one
//! capture per background and state: <out_dir>\<bg>-<n>-<state>.jpg.
use reach_core::{capture, dpi, overlay, Event, INCLUDE_OVERLAY_IN_CAPTURE};
use std::sync::atomic::{AtomicIsize, Ordering::SeqCst};
use std::time::{Duration, Instant};
use windows::core::{w, HSTRING};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

/// What the stage paints: a colour (0x00BBGGRR) when no bitmap, else the bitmap stretched.
static COLOR: AtomicIsize = AtomicIsize::new(0xFFFFFF);
static BITMAP: AtomicIsize = AtomicIsize::new(0);

unsafe extern "system" fn stage_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match m {
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_ERASEBKGND => {
            let dc = HDC(wp.0 as _);
            let mut r = RECT::default();
            let _ = GetClientRect(h, &mut r);
            let bmp = BITMAP.load(SeqCst);
            if bmp != 0 {
                let src = CreateCompatibleDC(Some(dc));
                let old = SelectObject(src, HGDIOBJ(bmp as _));
                let mut bm = windows::Win32::Graphics::Gdi::BITMAP::default();
                GetObjectW(HGDIOBJ(bmp as _), std::mem::size_of::<windows::Win32::Graphics::Gdi::BITMAP>() as i32, Some(&mut bm as *mut _ as _));
                SetStretchBltMode(dc, HALFTONE);
                let _ = StretchBlt(dc, 0, 0, r.right, r.bottom, Some(src), 0, 0, bm.bmWidth, bm.bmHeight, SRCCOPY);
                SelectObject(src, old);
                let _ = DeleteDC(src);
            } else {
                let b = CreateSolidBrush(COLORREF(COLOR.load(SeqCst) as u32));
                FillRect(dc, &r, b);
                let _ = DeleteObject(HGDIOBJ(b.0));
            }
            LRESULT(1)
        }
        _ => DefWindowProcW(h, m, wp, lp),
    }
}

fn pump(ms: u64) {
    let t = Instant::now();
    while t.elapsed() < Duration::from_millis(ms) {
        unsafe { let mut msg = MSG::default(); while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() { let _ = TranslateMessage(&msg); DispatchMessageW(&msg); } }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn shot(r: dpi::Rect, path: &str) {
    INCLUDE_OVERLAY_IN_CAPTURE.store(true, SeqCst);
    pump(40);
    if let Ok(px) = capture::grab(r, r.w, r.h) { if let Ok(j) = capture::jpeg(&px, r.w, r.h, 95) { let _ = std::fs::write(path, j); } }
}

fn status(state: &str, text: &str, id: u32) { overlay::status("Claude", state, text, id); }
fn ptr(x: i32, y: i32, kind: &str) { overlay::demo("Claude", x, y, kind); }

fn main() {
    dpi::make_aware();
    let a: Vec<String> = std::env::args().skip(1).collect();
    let out = a.first().cloned().unwrap_or_else(|| "tests\\overlay\\stage".into());
    let (sx, sy): (i32, i32) = (a.get(1).and_then(|v| v.parse().ok()).unwrap_or(40), a.get(2).and_then(|v| v.parse().ok()).unwrap_or(40));
    let _ = std::fs::create_dir_all(&out);
    let _ = overlay::ANSWER.set(|_, _| {}); // the card's buttons do nothing here
    overlay::FORCE_CAPTURE.store(true, SeqCst);
    overlay::start();
    unsafe {
        let hinst = GetModuleHandleW(None).unwrap();
        let wc = WNDCLASSEXW { cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32, lpfnWndProc: Some(stage_proc), hInstance: hinst.into(), lpszClassName: w!("ReachCursorStage"), ..Default::default() };
        RegisterClassExW(&wc);
        let s = dpi::dpi_scale_at(sx + 10, sy + 10);
        let (sw, sh) = ((420.0 * s) as i32, (190.0 * s) as i32);
        let stage = CreateWindowExW(WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW, w!("ReachCursorStage"), w!("Cursor stage"), WS_POPUP | WS_VISIBLE,
            sx, sy, sw, sh, None, None, Some(hinst.into()), None).unwrap();
        let _ = ShowWindow(stage, SW_SHOWNOACTIVATE);
        let r = dpi::Rect { x: sx, y: sy, w: sw, h: sh };
        let (px, py) = (sx + (40.0 * s) as i32, sy + (30.0 * s) as i32);
        let mut grounds: Vec<(String, isize, isize)> = vec![("white".into(), 0xFFFFFF, 0), ("black".into(), 0x000000, 0)];
        for f in a.iter().skip(3) {
            let b = LoadImageW(None, &HSTRING::from(f.as_str()), IMAGE_BITMAP, 0, 0, LR_LOADFROMFILE).map(|h| h.0 as isize).unwrap_or(0);
            let name = std::path::Path::new(f).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            if b != 0 { grounds.push((name, 0, b)); } else { eprintln!("couldn't load {f}"); }
        }
        for (name, col, bmp) in &grounds {
            COLOR.store(*col, SeqCst); BITMAP.store(*bmp, SeqCst);
            let _ = InvalidateRect(Some(stage), None, true);
            let _ = UpdateWindow(stage);
            let f = |n: &str| format!("{out}\\{name}-{n}.jpg");
            ptr(px - 30, py - 10, "move"); status("thinking", "", 0); pump(1700); shot(r, &f("1-thinking"));
            ptr(px, py, "click:Save"); pump(260); shot(r, &f("2-acting"));
            ptr(px, py, "type:Search box"); pump(320); shot(r, &f("3-typing"));
            status("waiting", "Press Send in Mail (to: test@example.com)", 1); pump(450); shot(r, &f("4-card"));
            status("thinking", "", 0); reach_core::emit(Event::UserPause { agent: "Claude".into(), paused: true }); pump(400); shot(r, &f("5-paused"));
            reach_core::emit(Event::UserPause { agent: "Claude".into(), paused: false });
            status("stopped", "", 0); pump(400); shot(r, &f("6-stopped"));
            ptr(px, py, "move"); status("thinking", "", 0); pump(300); status("done", "", 0); pump(400); shot(r, &f("7-done"));
            ptr(px, py, "move"); status("thinking", "", 0); pump(300); status("error", "Notepad closed before I could save", 0); pump(400); shot(r, &f("8-error"));
            status("idle", "", 0); pump(600);
            INCLUDE_OVERLAY_IN_CAPTURE.store(false, SeqCst);
        }
        let _ = DestroyWindow(stage);
    }
    println!("saved to {out}");
}
