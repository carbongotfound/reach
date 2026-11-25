//! Pause-on-user: passive low-level keyboard and mouse hooks note the last
//! physical (non-injected) input. Before an action that injects input, Reach waits
//! until the user has been idle for user_pause_idle_ms, and fails after
//! user_pause_timeout_ms. Input while inputlock.exe runs is ignored.

use crate::{config, events, Event};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering::Relaxed};
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::SystemInformation::GetTickCount64;
use windows::Win32::UI::WindowsAndMessaging::{CallNextHookEx, GetMessageW, SetWindowsHookExW, KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT, WH_KEYBOARD_LL, WH_MOUSE_LL};

static LAST_PHYSICAL: AtomicU64 = AtomicU64::new(0);
static SIMULATE_UNTIL: AtomicU64 = AtomicU64::new(0);
static INPUT_LOCK: AtomicBool = AtomicBool::new(false);
static HOOKS_OK: AtomicBool = AtomicBool::new(false);
static WAITING: AtomicI64 = AtomicI64::new(0);
static STARTED: AtomicBool = AtomicBool::new(false);

fn now() -> u64 {
    unsafe { GetTickCount64() }
}

pub fn hooks_ok() -> bool { HOOKS_OK.load(Relaxed) }
pub fn input_lock_running() -> bool { INPUT_LOCK.load(Relaxed) }

/// Optional extra filter for physical keys (the app uses it for the stop hotkey).
/// Return true to swallow the key.
pub static KEY_LISTENER: std::sync::OnceLock<fn(u32, bool) -> bool> = std::sync::OnceLock::new();
/// Tests only: also pass injected keys to KEY_LISTENER (a physical key can't be simulated).
pub static LISTEN_INJECTED: AtomicBool = AtomicBool::new(false);

unsafe extern "system" fn kb(n: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    if n >= 0 {
        let k = &*(l.0 as *const KBDLLHOOKSTRUCT);
        let injected = k.flags.0 & 0x12 != 0; // LLKHF_INJECTED | LLKHF_LOWER_IL_INJECTED
        if !injected && !INPUT_LOCK.load(Relaxed) {
            LAST_PHYSICAL.store(now(), Relaxed);
        }
        if !injected || LISTEN_INJECTED.load(Relaxed) {
            if let Some(f) = KEY_LISTENER.get() {
                if f(k.vkCode, w.0 == 0x100 || w.0 == 0x104) { // WM_KEYDOWN | WM_SYSKEYDOWN
                    return LRESULT(1);
                }
            }
        }
    }
    CallNextHookEx(None, n, w, l)
}

unsafe extern "system" fn ms(n: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    if n >= 0 && !INPUT_LOCK.load(Relaxed) {
        let m = &*(l.0 as *const MSLLHOOKSTRUCT);
        if m.flags & 0x3 == 0 { // LLMHF_INJECTED | LLMHF_LOWER_IL_INJECTED
            LAST_PHYSICAL.store(now(), Relaxed);
        }
    }
    CallNextHookEx(None, n, w, l)
}

pub fn start() {
    if STARTED.swap(true, Relaxed) {
        return;
    }
    LAST_PHYSICAL.store(now().saturating_sub(3_600_000), Relaxed);
    std::thread::Builder::new().name("reach-user-hooks".into()).spawn(|| unsafe {
        let module = GetModuleHandleW(None).ok();
        let k = SetWindowsHookExW(WH_KEYBOARD_LL, Some(kb), module.map(|m| m.into()), 0);
        let m = SetWindowsHookExW(WH_MOUSE_LL, Some(ms), module.map(|m| m.into()), 0);
        HOOKS_OK.store(k.is_ok() && m.is_ok(), Relaxed);
        config::log(&format!("pause-on-user hooks: keyboard {}, mouse {}", k.is_ok(), m.is_ok()));
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {}
    }).ok();
    std::thread::Builder::new().name("reach-inputlock-watch".into()).spawn(|| loop {
        INPUT_LOCK.store(process_running("inputlock.exe"), Relaxed);
        std::thread::sleep(std::time::Duration::from_millis(250));
    }).ok();
}

pub fn process_running(exe: &str) -> bool {
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return false };
        let mut e = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut found = false;
        if Process32FirstW(snap, &mut e).is_ok() {
            loop {
                if crate::util::from_wide(&e.szExeFile).eq_ignore_ascii_case(exe) {
                    found = true;
                    break;
                }
                if Process32NextW(snap, &mut e).is_err() {
                    break;
                }
            }
        }
        let _ = windows::Win32::Foundation::CloseHandle(snap);
        found
    }
}

pub fn idle_ms() -> u64 {
    let n = now();
    if SIMULATE_UNTIL.load(Relaxed) > n {
        return 0;
    }
    n.saturating_sub(LAST_PHYSICAL.load(Relaxed))
}

pub fn simulate(ms: u64) {
    let until = now() + ms.min(120_000);
    SIMULATE_UNTIL.store(until, Relaxed);
    LAST_PHYSICAL.store(until, Relaxed);
}

/// Which actions change something on screen and so wait until the user (mouse or keyboard) stops.
/// That includes UIA actions (invoke/set/toggle/focus/click): they don't move the mouse, but
/// they change what the user is typing into. Only looking (find, inspect, read) runs meanwhile.
pub fn gated(tool: &str, action: &str) -> bool {
    let a = action.to_ascii_lowercase();
    match tool.to_ascii_lowercase().as_str() {
        "mouse" => !matches!(a.as_str(), "position" | "cursor_position" | "wait"),
        "keyboard" => true,
        "apps" => matches!(a.as_str(), "open" | "launch" | "start" | "focus" | "close" | "minimize" | "maximize" | "restore"),
        "ui" => matches!(a.as_str(), "invoke" | "click" | "set" | "set_value" | "toggle" | "focus"),
        _ => false,
    }
}

/// "Resume" in the app: the current wait for idle ends now, once.
pub static RESUME: AtomicBool = AtomicBool::new(false);

/// Blocks until the user has been idle long enough. Returns ms waited.
pub fn wait_for_idle(agent: &str) -> Result<u64, String> {
    let c = config::get();
    if !c.pause_on_user {
        return Ok(0);
    }
    let need = c.user_pause_idle_ms.max(100) as u64;
    if idle_ms() >= need {
        return Ok(0);
    }
    let timeout = c.user_pause_timeout_ms.max(1000) as u128;
    let t0 = std::time::Instant::now();
    if WAITING.fetch_add(1, Relaxed) == 0 {
        events::emit(Event::UserPause { agent: agent.to_string(), paused: true });
    }
    RESUME.store(false, Relaxed);
    let r = loop {
        if idle_ms() >= need || RESUME.swap(false, Relaxed) {
            break Ok(t0.elapsed().as_millis() as u64);
        }
        if crate::cancelled() {
            break Err(crate::STOPPED.to_string());
        }
        if t0.elapsed().as_millis() > timeout {
            break Err(format!(
                "Paused because you're using the PC: no {} s idle gap within {} s, so the command was not run. Retry when the PC is idle.",
                trim_num(need as f64 / 1000.0), trim_num(timeout as f64 / 1000.0)));
        }
        std::thread::sleep(std::time::Duration::from_millis(40));
    };
    if WAITING.fetch_sub(1, Relaxed) == 1 {
        events::emit(Event::UserPause { agent: agent.to_string(), paused: false });
    }
    r
}

fn trim_num(v: f64) -> String {
    let s = format!("{v:.1}");
    s.trim_end_matches(".0").to_string()
}

#[cfg(test)]
mod tests {
    #[test]
    fn typing_or_mouse_pauses_changes_not_looking() {
        for (t, a) in [("keyboard", "type"), ("mouse", "click"), ("ui", "set"), ("ui", "invoke"), ("ui", "toggle"), ("apps", "focus")] { assert!(super::gated(t, a), "{t} {a}"); }
        for (t, a) in [("ui", "find"), ("ui", "inspect"), ("ui", "read"), ("screenshot", ""), ("mouse", "wait")] { assert!(!super::gated(t, a), "{t} {a}"); }
    }
}
