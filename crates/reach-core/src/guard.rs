//! Approval guard: right before Reach presses a control whose name says it sends, posts,
//! submits, pays, deletes, installs, shares or signs in, it asks the host to approve.
//! The app sets HOOK (it shows the approval card); `reach serve` leaves it unset.
//! Coverage: UIA invoke, left clicks (the control under the point) and Enter/Space on the
//! focused control. Not covered: shortcuts like Ctrl+Enter or Alt+S, and controls with no name.

use std::sync::OnceLock;
use windows::Win32::Foundation::POINT;
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::UI::Accessibility::*;

/// (control type, control name, title of its window) -> Ok to go ahead, Err(message) to refuse.
pub static HOOK: OnceLock<fn(&str, &str, &str) -> Result<(), String>> = OnceLock::new();
/// Screen rectangle (left, top, right, bottom) of the control the hook is being asked about, so the
/// host can outline it on the approval card. Taken (and cleared) by `take_target`.
static TARGET: std::sync::Mutex<Option<(i32, i32, i32, i32)>> = std::sync::Mutex::new(None);
pub fn take_target() -> Option<(i32, i32, i32, i32)> { TARGET.lock().unwrap().take() }

const WORDS: &[&str] = &["send", "post", "publish", "submit", "pay", "buy", "purchase", "checkout", "transfer", "delete", "install", "upload", "share", "signin", "login"];
const PAIRS: &[&str] = &["sign in", "log in", "place order", "check out", "send now", "pay now"];
/// Typing targets: their name is a label, not what pressing them does.
const SKIP_TYPES: &[&str] = &["edit", "document"];

pub fn risky(ty: &str, name: &str) -> bool {
    if SKIP_TYPES.contains(&ty.to_lowercase().as_str()) { return false; }
    let words: Vec<String> = name.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(String::from).collect();
    let joined = words.join(" ");
    words.iter().any(|w| WORDS.contains(&w.as_str())) || PAIRS.iter().any(|p| format!(" {joined} ").contains(&format!(" {p} ")))
}

/// Asks the host when the control looks risky.
pub fn check_el(el: &IUIAutomationElement) -> Result<(), String> {
    let (ty, name) = unsafe { (el.CurrentLocalizedControlType().map(|s| s.to_string()).unwrap_or_default(), el.CurrentName().map(|s| s.to_string()).unwrap_or_default()) };
    if !risky(&ty, &name) { return Ok(()); }
    let Some(f) = HOOK.get() else { return Ok(()) };
    *TARGET.lock().unwrap() = unsafe { el.CurrentBoundingRectangle() }.ok().filter(|r| r.right > r.left && r.bottom > r.top).map(|r| (r.left, r.top, r.right, r.bottom));
    f(&ty, &name, &window_title(el))
}

/// Title of the top-level window the element is in.
fn window_title(el: &IUIAutomationElement) -> String {
    use windows::Win32::UI::WindowsAndMessaging::{GetAncestor, GA_ROOT};
    unsafe {
        let Some(a) = auto() else { return String::new() };
        let Ok(walker) = a.ControlViewWalker() else { return String::new() };
        let mut cur = el.clone();
        for _ in 0..50 {
            let h = cur.CurrentNativeWindowHandle().unwrap_or_default();
            if !h.0.is_null() { return crate::apps::title_of(GetAncestor(h, GA_ROOT)); }
            match walker.GetParentElement(&cur) { Ok(p) => cur = p, Err(_) => break }
        }
        String::new()
    }
}

fn auto() -> Option<IUIAutomation> {
    if HOOK.get().is_none() { return None; }
    unsafe {
        let a: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
        if let Ok(a2) = windows::core::Interface::cast::<IUIAutomation2>(&a) { let _ = a2.SetConnectionTimeout(2000); }
        Some(a)
    }
}

/// The control under a screen point (before a left click).
pub fn check_point(x: i32, y: i32) -> Result<(), String> {
    let Some(a) = auto() else { return Ok(()) };
    match unsafe { a.ElementFromPoint(POINT { x, y }) } { Ok(el) => check_el(&el), Err(_) => Ok(()) }
}

/// The focused control (before Enter or Space).
pub fn check_focused() -> Result<(), String> {
    let Some(a) = auto() else { return Ok(()) };
    match unsafe { a.GetFocusedElement() } { Ok(el) => check_el(&el), Err(_) => Ok(()) }
}

/// Enter / Space press the focused control.
pub fn presses(chord: &str) -> bool {
    let k = chord.to_lowercase();
    let last = k.rsplit('+').next().unwrap_or("");
    matches!(last, "return" | "enter" | "space" | "kp_enter") && !k.contains("alt+") && !k.contains("win+")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn words() {
        assert!(risky("button", "Send"));
        assert!(risky("button", "Send now"));
        assert!(risky("hyperlink", "Sign in"));
        assert!(risky("button", "Place order"));
        assert!(risky("menu item", "Delete"));
        assert!(!risky("button", "Save"));
        assert!(!risky("button", "Sender"));
        assert!(!risky("edit", "Send to"));
        assert!(!risky("button", "Sign"));
        assert!(presses("Return") && presses("ctrl+Return") && presses("space"));
        assert!(!presses("ctrl+s") && !presses("alt+Return"));
    }
}
