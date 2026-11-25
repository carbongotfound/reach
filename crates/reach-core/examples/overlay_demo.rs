//! Overlay check on a stage window of our own (never over your other windows).
//! cargo run --release -p reach-core --example overlay_demo -- <out_dir>
//! Captures frames of the stage with the overlay made visible to capture, then checks:
//! window styles, hit-testing passes through, a real click lands on the window below,
//! the foreground window never changes, and the real cursor is not moved by glides.
use reach_core::{capture, dpi, input, overlay, INCLUDE_OVERLAY_IN_CAPTURE};
use std::sync::atomic::{AtomicU32, Ordering::SeqCst};
use std::time::{Duration, Instant};
use windows::core::w;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

static CLICKS: AtomicU32 = AtomicU32::new(0);
static MOUSE: AtomicU32 = AtomicU32::new(0);
unsafe extern "system" fn stage_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match m {
        WM_LBUTTONDOWN => { CLICKS.fetch_add(1, SeqCst); LRESULT(0) }
        WM_MOUSEMOVE | WM_LBUTTONUP | WM_NCLBUTTONDOWN | WM_POINTERDOWN => { MOUSE.fetch_add(1, SeqCst); DefWindowProcW(h, m, wp, lp) }
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_ERASEBKGND => {
            let mut r = RECT::default();
            let _ = GetClientRect(h, &mut r);
            let b = CreateSolidBrush(COLORREF(0x00F4F1EE));
            FillRect(HDC(wp.0 as _), &r, b);
            let _ = DeleteObject(HGDIOBJ(b.0));
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

fn shot(r: dpi::Rect, out: &str, name: &str) {
    INCLUDE_OVERLAY_IN_CAPTURE.store(true, SeqCst);
    pump(60);
    if let Ok(px) = capture::grab(r, r.w, r.h) { if let Ok(j) = capture::jpeg(&px, r.w, r.h, 90) { let _ = std::fs::write(format!("{out}\\{name}.jpg"), j); } }
    INCLUDE_OVERLAY_IN_CAPTURE.store(false, SeqCst);
}

fn main() {
    dpi::make_aware();
    let out = std::env::args().nth(1).unwrap_or_else(|| "tests\\overlay".into());
    let _ = std::fs::create_dir_all(&out);
    overlay::FORCE_CAPTURE.store(true, SeqCst);
    overlay::start();
    let fg0 = unsafe { GetForegroundWindow() };
    let cur0 = input::cursor_pos();
    let mut ok = true;
    let mut check = |name: &str, pass: bool, detail: String| { println!("{} {name}: {detail}", if pass { "PASS" } else { "FAIL" }); ok &= pass; };
    unsafe {
        let hinst = GetModuleHandleW(None).unwrap();
        let wc = WNDCLASSEXW { cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32, lpfnWndProc: Some(stage_proc), hInstance: hinst.into(), lpszClassName: w!("ReachOverlayStage"), ..Default::default() };
        RegisterClassExW(&wc);
        let s = dpi::dpi_scale_at(cur0.0, cur0.1);
        let (sw, sh) = ((560.0 * s) as i32, (300.0 * s) as i32);
        // top-left of the primary monitor's work area, topmost so nothing of yours sits in the shot
        let mut wa = RECT::default();
        let _ = SystemParametersInfoW(SPI_GETWORKAREA, 0, Some(&mut wa as *mut _ as _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));
        let (sx, sy) = (wa.left + 40, wa.top + 40);
        let stage = CreateWindowExW(WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW, w!("ReachOverlayStage"), w!("Overlay stage"), WS_POPUP | WS_VISIBLE,
            sx, sy, sw, sh, None, None, Some(hinst.into()), None).unwrap();
        let _ = ShowWindow(stage, SW_SHOWNOACTIVATE);
        pump(200);
        let r = dpi::Rect { x: sx, y: sy, w: sw, h: sh };
        let px = |fx: f64, fy: f64| (sx + (sw as f64 * fx) as i32, sy + (sh as f64 * fy) as i32);

        // 1. UIA-style glide: appears, glides, clicks (ripple), types, key chord, pause
        let a = px(0.15, 0.25);
        overlay::demo("Claude", a.0, a.1, "move");
        pump(250); shot(r, &out, "1-appear");
        let b = px(0.55, 0.45);
        overlay::demo("Claude", b.0, b.1, "click");
        pump(150); shot(r, &out, "2-glide-trail");
        pump(300); shot(r, &out, "3-click-ripple");
        overlay::demo("Claude", b.0, b.1, "type");
        pump(200); shot(r, &out, "4-typing");
        overlay::demo("Claude", b.0, b.1, "key:Ctrl + S");
        pump(200); shot(r, &out, "5-chord");
        reach_core::emit(reach_core::Event::UserPause { agent: "Claude".into(), paused: true });
        pump(200); shot(r, &out, "6-paused");
        reach_core::emit(reach_core::Event::UserPause { agent: "Claude".into(), paused: false });
        let c = px(0.3, 0.6);
        overlay::demo("ChatGPT", c.0, c.1, "click");
        pump(700); shot(r, &out, "7-chatgpt");

        let cur1 = input::cursor_pos();
        // you may move your own mouse meanwhile; what matters is that it never went to the agent's targets
        let near = |p: (i32, i32)| [a, b, c].iter().any(|t| (t.0 - p.0).abs() < 3 && (t.1 - p.1).abs() < 3);
        check("real cursor not moved by glides", !near(cur1), format!("{cur0:?} -> {cur1:?}, targets {a:?} {b:?} {c:?}"));
        check("focus never moved", GetForegroundWindow() == fg0, format!("{:?} -> {:?}", fg0.0, GetForegroundWindow().0));

        let ov = FindWindowW(w!("ReachAgentOverlay"), None).unwrap_or_default();
        let ex = GetWindowLongW(ov, GWL_EXSTYLE) as u32;
        let want = (WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW).0;
        check("overlay window styles", !ov.0.is_null() && ex & want == want, format!("exstyle {ex:#x}"));
        let mut wr = RECT::default();
        let _ = GetWindowRect(ov, &mut wr);
        let vs = dpi::ScreenMap::virtual_screen();
        check("covers all monitors", wr.left == vs.x && wr.top == vs.y && wr.right - wr.left == vs.w && wr.bottom - wr.top == vs.h,
            format!("{},{} {}x{} vs virtual {},{} {}x{}", wr.left, wr.top, wr.right - wr.left, wr.bottom - wr.top, vs.x, vs.y, vs.w, vs.h));
        // the arrow body sits a few px below-right of the tip at c
        let s2 = dpi::dpi_scale_at(c.0, c.1) as f64 * 1.15;
        let on_arrow = POINT { x: c.0 + (3.0 * s2) as i32, y: c.1 + (9.0 * s2) as i32 };
        let hit = WindowFromPoint(on_arrow);
        check("hit test passes through the arrow", hit == stage, format!("window at arrow = {:?} (stage {:?})", hit.0, stage.0));

        // a real click on the arrow lands on the window below (moves your pointer briefly, then puts it back)
        if std::env::args().any(|a| a == "--click") {
        let n0 = CLICKS.load(SeqCst);
        let _ = SetCursorPos(on_arrow.x, on_arrow.y);
        let at = input::cursor_pos();
        if WindowFromPoint(POINT { x: at.0, y: at.1 }) != stage { println!("ABORT: point not on stage"); std::process::exit(2); }
        let i = |f| INPUT { r#type: INPUT_MOUSE, Anonymous: INPUT_0 { mi: MOUSEINPUT { dwFlags: f, ..Default::default() } } };
        SendInput(&[i(MOUSEEVENTF_LEFTDOWN), i(MOUSEEVENTF_LEFTUP)], std::mem::size_of::<INPUT>() as i32);
        pump(200);
        let _ = SetCursorPos(cur0.0, cur0.1);
        check("real click goes through to the window below", CLICKS.load(SeqCst) == n0 + 1, format!("stage clicks {} -> {}", n0, CLICKS.load(SeqCst)));
        let fg = GetForegroundWindow();
        let mut cls = [0u16; 64]; let n = GetClassNameW(fg, &mut cls);
        let mut pid = 0u32; GetWindowThreadProcessId(fg, Some(&mut pid));
        check("focus never moved (after click)", fg == fg0, format!("fg {:?} class {} pid {} (me {}), stage mouse msgs {}", fg.0, String::from_utf16_lossy(&cls[..n as usize]), pid, std::process::id(), MOUSE.load(SeqCst)));
        }

        // capture exclusion: a normal capture must not contain the overlay
        overlay::FORCE_CAPTURE.store(false, SeqCst);
        overlay::demo("Claude", b.0, b.1, "click");
        pump(600);
        let without = capture::grab(r, r.w, r.h).unwrap();
        let with = { INCLUDE_OVERLAY_IN_CAPTURE.store(true, SeqCst); pump(150); let p = capture::grab(r, r.w, r.h).unwrap(); INCLUDE_OVERLAY_IN_CAPTURE.store(false, SeqCst); p };
        let bg = |p: &[u8]| p.chunks(4).filter(|c| c[0] != 0xF4 || c[1] != 0xF1 || c[2] != 0xEE).count();
        check("hidden from normal captures", bg(&without) == 0 && bg(&with) > 200, format!("non-background px: normal {}, debug {}", bg(&without), bg(&with)));

        // stop fades it out
        reach_core::stop();
        pump(700);
        let ex_vis = IsWindowVisible(ov).as_bool();
        check("hides after Stop", !ex_vis, format!("visible={ex_vis}"));
        let _ = DestroyWindow(stage);
    }
    println!("{}", if ok { "ALL PASS" } else { "SOME FAILED" });
    std::process::exit(if ok { 0 } else { 1 });
}
