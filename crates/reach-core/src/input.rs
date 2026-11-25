//! SendInput mouse and keyboard, chords, and motion (instant / fast / smooth).
//! Every loop checks the stop flag so Stop halts a glide or a long typing run.

use crate::dpi::{round, ScreenMap};
use crate::util::rand;
use crate::{cancelled, ctx, emit_pointer, yield_, STOPPED};
use std::thread::sleep;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::POINT;
use windows::Win32::Media::{timeBeginPeriod, timeEndPeriod};
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, SetCursorPos};

pub type R<T> = Result<T, String>;

fn ms(n: u64) { sleep(Duration::from_millis(n)) }

pub fn send(inputs: &[INPUT]) -> R<()> {
    if inputs.is_empty() {
        return Ok(());
    }
    let n = unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
    if n == 0 {
        return Err(format!("SendInput failed ({})", unsafe { windows::Win32::Foundation::GetLastError().0 }));
    }
    Ok(())
}

fn mouse_input(dx: i32, dy: i32, data: i32, flags: MOUSE_EVENT_FLAGS) -> INPUT {
    INPUT { r#type: INPUT_MOUSE, Anonymous: INPUT_0 { mi: MOUSEINPUT { dx, dy, mouseData: data as u32, dwFlags: flags, time: 0, dwExtraInfo: 0 } } }
}

fn key_input(vk: u16, scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VIRTUAL_KEY(vk), wScan: scan, dwFlags: flags, time: 0, dwExtraInfo: 0 } } }
}

pub fn cursor_pos() -> (i32, i32) {
    let mut p = POINT::default();
    unsafe { let _ = GetCursorPos(&mut p); }
    (p.x, p.y)
}

/// "smooth" (Human), "fast", or "instant"; --fast turns smooth into fast for one call.
pub fn motion_mode() -> &'static str {
    let m = crate::config::get().motion.to_ascii_lowercase();
    match m.as_str() {
        "instant" => "instant",
        "fast" => "fast",
        _ if ctx().fast => "fast",
        _ => "smooth",
    }
}

struct Timer;
impl Timer {
    fn new() -> Timer { unsafe { timeBeginPeriod(1); } Timer }
}
impl Drop for Timer {
    fn drop(&mut self) { unsafe { timeEndPeriod(1); } }
}

fn send_abs(x: i32, y: i32) -> R<()> {
    let vs = ScreenMap::virtual_screen();
    let nx = round((x - vs.x) as f64 * 65535.0 / (vs.w - 1).max(1) as f64).clamp(0, 65535);
    let ny = round((y - vs.y) as f64 * 65535.0 / (vs.h - 1).max(1) as f64).clamp(0, 65535);
    send(&[mouse_input(nx, ny, 0, MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK)])
}

/// Glide the real cursor to `target` (screen px). smooth: cubic Bezier with random
/// control points, ease-in-out, overshoot + correction on long moves, light jitter,
/// ~150-300 ms. fast: short direct glide. instant: teleport. If you grab the mouse
/// mid-glide, it stops, waits for you, and resumes.
pub fn move_to(target: (i32, i32), dur_scale: f64) -> R<()> {
    let mode = motion_mode();
    let (cx, cy) = cursor_pos();
    let (dx, dy) = ((target.0 - cx) as f64, (target.1 - cy) as f64);
    let d = (dx * dx + dy * dy).sqrt();
    if mode != "instant" && d >= 3.0 {
        let path = if mode == "fast" { Path::fast((cx as f64, cy as f64), (target.0 as f64, target.1 as f64)) }
                   else { Path::human((cx as f64, cy as f64), (target.0 as f64, target.1 as f64)) };
        let total = path.ms * dur_scale;
        let _t = Timer::new();
        let t0 = Instant::now();
        let (mut lx, mut ly) = (i32::MIN, i32::MIN);
        loop {
            if cancelled() {
                return Err(STOPPED.into());
            }
            let t = (t0.elapsed().as_secs_f64() * 1000.0 / total).min(1.0);
            let (px, py) = path.at(t);
            let (ix, iy) = (px.round() as i32, py.round() as i32);
            if ix != lx || iy != ly {
                send_abs(ix, iy)?;
                emit_pointer(ix, iy, "move", true);
                lx = ix;
                ly = iy;
            }
            if t >= 1.0 {
                break;
            }
            if !ctx().no_yield && crate::config::get().pause_on_user && yield_::idle_ms() < 120 {
                yield_::wait_for_idle(&ctx().agent)?;
                return move_to(target, dur_scale);
            }
            ms(8);
        }
    }
    send_abs(target.0, target.1)?;
    let (fx, fy) = cursor_pos();
    if (fx, fy) != target {
        unsafe { let _ = SetCursorPos(target.0, target.1); }
    }
    Ok(())
}

/// A motion path from a to b. `at(t)` takes linear time t in [0,1] and applies easing.
pub struct Path {
    pub a: (f64, f64),
    pub b: (f64, f64),
    pub c1: (f64, f64),
    pub c2: (f64, f64),
    pub overshoot: Option<(f64, f64)>,
    pub ms: f64,
    pub jitter: f64,
}

pub fn ease(t: f64) -> f64 {
    if t < 0.5 { 4.0 * t * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(3) / 2.0 }
}

fn bez(p0: (f64, f64), p1: (f64, f64), p2: (f64, f64), p3: (f64, f64), t: f64) -> (f64, f64) {
    let u = 1.0 - t;
    let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    (a * p0.0 + b * p1.0 + c * p2.0 + d * p3.0, a * p0.1 + b * p1.1 + c * p2.1 + d * p3.1)
}

impl Path {
    pub fn human(a: (f64, f64), b: (f64, f64)) -> Path {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let d = (dx * dx + dy * dy).sqrt().max(1.0);
        let (nx, ny) = (-dy / d, dx / d); // unit normal
        let bend = |f: f64| (0.3 + rand() * 0.7) * (d * 0.15).min(80.0) * f * if rand() < 0.5 { -1.0 } else { 1.0 };
        let (o1, o2) = (bend(1.0), bend(0.6));
        let c1 = (a.0 + dx * (0.2 + rand() * 0.15) + nx * o1, a.1 + dy * (0.2 + rand() * 0.15) + ny * o1);
        let c2 = (a.0 + dx * (0.65 + rand() * 0.15) + nx * o2, a.1 + dy * (0.65 + rand() * 0.15) + ny * o2);
        // long moves overshoot a few px past the target, then correct
        let overshoot = (d > 300.0).then(|| {
            let k = (d * 0.012).min(10.0) * (0.6 + rand() * 0.4);
            (b.0 + dx / d * k, b.1 + dy / d * k)
        });
        Path { a, b, c1, c2, overshoot, ms: (150.0 + d * 0.08).min(300.0), jitter: 0.6 }
    }

    pub fn fast(a: (f64, f64), b: (f64, f64)) -> Path {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let d = (dx * dx + dy * dy).sqrt();
        let c1 = (a.0 + dx / 3.0, a.1 + dy / 3.0);
        let c2 = (a.0 + dx * 2.0 / 3.0, a.1 + dy * 2.0 / 3.0);
        Path { a, b, c1, c2, overshoot: None, ms: (50.0 + d * 0.06).min(120.0), jitter: 0.0 }
    }

    pub fn at(&self, t: f64) -> (f64, f64) {
        if t >= 1.0 {
            return self.b;
        }
        let e = ease(t);
        match self.overshoot {
            // first 88% of eased progress goes to the overshoot point, the rest corrects back
            Some(o) if e < 0.88 => {
                let p = bez(self.a, self.c1, self.c2, o, e / 0.88);
                self.jit(p, e)
            }
            Some(o) => {
                let k = (e - 0.88) / 0.12;
                (o.0 + (self.b.0 - o.0) * k, o.1 + (self.b.1 - o.1) * k)
            }
            None => self.jit(bez(self.a, self.c1, self.c2, self.b, e), e),
        }
    }

    fn jit(&self, p: (f64, f64), e: f64) -> (f64, f64) {
        if self.jitter == 0.0 || e > 0.85 {
            return p;
        }
        (p.0 + (rand() - 0.5) * 2.0 * self.jitter, p.1 + (rand() - 0.5) * 2.0 * self.jitter)
    }
}

pub fn click_at(map: &ScreenMap, x: i64, y: i64, button: &str, times: u32) -> R<()> {
    let smooth = motion_mode() == "smooth";
    let target = map.to_screen(x, y);
    if button == "left" { crate::guard::check_point(target.0 as i32, target.1 as i32)?; }
    move_to(target, 1.0)?;
    emit_pointer(target.0, target.1, "click", true);
    ms(if smooth { 60 + (rand() * 60.0) as u64 } else { 15 }); // short pause before the click
    let (down, up) = match button {
        "right" => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
        "middle" => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP),
        _ => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
    };
    for i in 0..times {
        send(&[mouse_input(0, 0, 0, down)])?;
        ms(if smooth { 45 + (rand() * 30.0) as u64 } else { 8 });
        send(&[mouse_input(0, 0, 0, up)])?;
        if i + 1 < times {
            ms(if smooth { 70 } else { 40 });
        }
    }
    Ok(())
}

pub fn drag(map: &ScreenMap, x1: i64, y1: i64, x2: i64, y2: i64) -> R<()> {
    let smooth = motion_mode() == "smooth";
    let a = map.to_screen(x1, y1);
    move_to(a, 1.0)?;
    emit_pointer(a.0, a.1, "click", true);
    ms(if smooth { 60 } else { 30 });
    send(&[mouse_input(0, 0, 0, MOUSEEVENTF_LEFTDOWN)])?;
    ms(if smooth { 70 } else { 30 });
    let r = move_to(map.to_screen(x2, y2), 1.5);
    ms(if smooth { 70 } else { 30 });
    send(&[mouse_input(0, 0, 0, MOUSEEVENTF_LEFTUP)])?; // always release
    r
}

pub fn scroll(dir: &str, amount: i64) -> R<()> {
    let mut delta = 120 * amount.max(1) as i32;
    match dir {
        "down" | "right" => delta = -delta,
        "up" | "left" => {}
        _ => return Err("direction must be up, down, left or right.".into()),
    }
    let flags = if dir == "left" || dir == "right" { MOUSEEVENTF_HWHEEL } else { MOUSEEVENTF_WHEEL };
    send(&[mouse_input(0, 0, delta, flags)])
}

pub fn button(down: bool) -> R<()> {
    send(&[mouse_input(0, 0, 0, if down { MOUSEEVENTF_LEFTDOWN } else { MOUSEEVENTF_LEFTUP })])
}

// ---------- keyboard ----------

/// "ctrl+shift+t", "ctrl+c, ctrl+v", "ctrl+a Delete", "ctrl + s"
pub fn split_chords(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for seg in text.split(',') {
        let mut t = String::new();
        // collapse spaces around '+'
        let seg = seg.trim();
        let chars: Vec<char> = seg.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if chars[i] == '+' {
                while t.ends_with([' ', '\t']) { t.pop(); }
                t.push('+');
                i += 1;
                while i < chars.len() && (chars[i] == ' ' || chars[i] == '\t') { i += 1; }
                continue;
            }
            t.push(chars[i]);
            i += 1;
        }
        out.extend(t.split([' ', '\t']).filter(|p| !p.is_empty()).map(String::from));
    }
    out
}

pub fn chord_label(chord: &str) -> String {
    chord.split('+').filter(|p| !p.is_empty()).map(|p| {
        let p = p.trim().to_ascii_lowercase();
        let p = match p.as_str() {
            "control" => "ctrl".to_string(),
            "return" => "enter".to_string(),
            "escape" => "esc".to_string(),
            "super" | "meta" | "cmd" | "windows" => "win".to_string(),
            _ => p,
        };
        if p.chars().count() <= 1 { p.to_uppercase() } else { p[..1].to_uppercase() + &p[1..] }
    }).collect::<Vec<_>>().join(" + ")
}

pub fn lookup_key(p: &str) -> Option<(u16, bool)> {
    let p = p.to_ascii_lowercase();
    if p.len() == 1 {
        let c = p.as_bytes()[0];
        if c.is_ascii_lowercase() { return Some(((c - b'a' + 0x41) as u16, false)); }
        if c.is_ascii_digit() { return Some((c as u16, false)); }
    }
    let r = match p.as_str() {
        "return" | "enter" => (0x0D, false),
        "tab" => (0x09, false),
        "esc" | "escape" => (0x1B, false),
        "space" | "spacebar" => (0x20, false),
        "backspace" | "bksp" => (0x08, false),
        "delete" | "del" => (0x2E, true),
        "insert" | "ins" => (0x2D, true),
        "home" => (0x24, true),
        "end" => (0x23, true),
        "pageup" | "pgup" => (0x21, true),
        "pagedown" | "pgdn" => (0x22, true),
        "up" | "uparrow" => (0x26, true),
        "down" | "downarrow" => (0x28, true),
        "left" | "leftarrow" => (0x25, true),
        "right" | "rightarrow" => (0x27, true),
        "win" | "super" | "meta" | "cmd" => (0x5B, false),
        "apps" | "menu" => (0x5D, false),
        "ctrl" | "control" => (0x11, false),
        "alt" => (0x12, false),
        "shift" => (0x10, false),
        "capslock" => (0x14, false),
        "printscreen" | "prtsc" => (0x2C, false),
        _ => {
            if p.len() >= 2 && p.starts_with('f') {
                if let Ok(n) = p[1..].parse::<u16>() {
                    if (1..=24).contains(&n) { return Some((0x70 + n - 1, false)); }
                }
            }
            return None;
        }
    };
    Some(r)
}

pub struct Chord { pub mods: Vec<u16>, pub key: u16, pub ext: bool }

pub fn parse_chord(chord: &str) -> R<Chord> {
    let parts: Vec<&str> = if chord.contains('+') || chord.len() < 3 { chord.split('+').filter(|s| !s.is_empty()).collect() }
                           else { chord.split(['+', '-']).filter(|s| !s.is_empty()).collect() };
    let mut mods = Vec::new();
    let (mut key, mut ext) = (0u16, false);
    for (i, raw) in parts.iter().enumerate() {
        let p = raw.trim().to_ascii_lowercase();
        let is_mod = matches!(p.as_str(), "ctrl" | "control" | "alt" | "shift" | "win" | "super" | "meta" | "cmd" | "windows");
        if is_mod && i < parts.len() - 1 {
            mods.push(match p.as_str() { "ctrl" | "control" => 0x11, "alt" => 0x12, "shift" => 0x10, _ => 0x5B });
        } else {
            let (vk, e) = lookup_key(&p).ok_or_else(|| format!("Unknown key `{raw}`."))?;
            key = vk;
            ext = e;
        }
    }
    if key == 0 {
        return Err(format!("No key in chord `{chord}`."));
    }
    if mods.contains(&0x5B) && key == 0x4C {
        return Err("Win+L is blocked so Reach cannot lock the PC.".into());
    }
    // The app's stop hotkey must stay the user's: the agent can't press it.
    if mods.contains(&0x11) && mods.contains(&0x12) && key == 0x1B {
        return Err("Ctrl+Alt+Esc is the stop hotkey, so Reach won't press it.".into());
    }
    Ok(Chord { mods, key, ext })
}

/// Modifiers down, key, release in reverse order. Everything that went down is released.
pub fn press_chord(chord: &str, hold_ms: u64) -> R<()> {
    let c = parse_chord(chord)?;
    let smooth = motion_mode() == "smooth";
    let gap = if smooth { 25 } else { 0 };
    let mut downed: Vec<(u16, KEYBD_EVENT_FLAGS)> = Vec::new();
    let mut r = Ok(());
    for m in &c.mods {
        let f = if *m == 0x5B { KEYEVENTF_EXTENDEDKEY } else { KEYBD_EVENT_FLAGS(0) };
        if let Err(e) = send(&[key_input(*m, 0, f)]) { r = Err(e); break; }
        downed.push((*m, f));
        if gap > 0 { ms(gap); }
    }
    if r.is_ok() {
        let f = if c.ext { KEYEVENTF_EXTENDEDKEY } else { KEYBD_EVENT_FLAGS(0) };
        match send(&[key_input(c.key, 0, f)]) {
            Ok(()) => {
                downed.push((c.key, f));
                let hold = if hold_ms > 0 { hold_ms } else if smooth { 45 + (rand() * 25.0) as u64 } else { 0 };
                if hold > 0 { ms(hold); }
            }
            Err(e) => r = Err(e),
        }
    }
    for (i, (vk, f)) in downed.iter().enumerate().rev() {
        let _ = send(&[key_input(*vk, 0, *f | KEYEVENTF_KEYUP)]);
        if gap > 0 && i > 0 { ms(gap / 2); }
    }
    ms(if smooth { 30 } else { 15 });
    r
}

/// smooth: per-character key events with a variable human delay (~25-45 ms, a bit
/// longer after spaces and punctuation). fast/instant: bulk chunks of 20 characters.
pub fn type_text(text: &str) -> R<()> {
    let smooth = motion_mode() == "smooth";
    let mut base = 35.0f64;
    let n_chars = text.chars().count() as f64;
    if smooth && n_chars * base > 12000.0 {
        base = (12000.0 / n_chars).max(4.0);
    }
    let _t = smooth.then(Timer::new);
    let mut list: Vec<INPUT> = Vec::new();
    let mut n = 0u64;
    for ch in text.chars() {
        if cancelled() {
            return Err(STOPPED.into());
        }
        if ch == '\r' { continue; }
        if ch == '\n' {
            list.push(key_input(0x0D, 0, KEYBD_EVENT_FLAGS(0)));
            list.push(key_input(0x0D, 0, KEYEVENTF_KEYUP));
        } else {
            let mut buf = [0u16; 2];
            for u in ch.encode_utf16(&mut buf) {
                list.push(key_input(0, *u, KEYEVENTF_UNICODE));
                list.push(key_input(0, *u, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP));
            }
        }
        n += 1;
        if smooth {
            send(&list)?;
            list.clear();
            if n % 20 == 0 {
                let (x, y) = cursor_pos();
                emit_pointer(x, y, "type", false);
            }
            if !ctx().no_yield && n % 4 == 0 {
                yield_::wait_for_idle(&ctx().agent)?;
            }
            let pause = if " .,;:!?".contains(ch) { 1.5 } else { 1.0 };
            ms((base * (0.72 + rand() * 0.56) * pause).round() as u64);
        } else if list.len() >= 40 {
            send(&list)?;
            list.clear();
            if !ctx().no_yield {
                yield_::wait_for_idle(&ctx().agent)?;
            }
        }
    }
    send(&list)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chords() {
        assert_eq!(split_chords("ctrl+c, ctrl+v").join("|"), "ctrl+c|ctrl+v");
        assert_eq!(split_chords("ctrl + shift + t").join("|"), "ctrl+shift+t");
        assert_eq!(split_chords("ctrl+a Delete").join("|"), "ctrl+a|Delete");
        let c = parse_chord("ctrl+shift+t").unwrap();
        assert_eq!((c.mods.clone(), c.key), (vec![0x11, 0x10], 0x54));
        let c = parse_chord("win+r").unwrap();
        assert_eq!((c.mods.clone(), c.key), (vec![0x5B], 0x52));
        let c = parse_chord("shift").unwrap();
        assert!(c.mods.is_empty() && c.key == 0x10);
        let c = parse_chord("alt+F4").unwrap();
        assert_eq!((c.mods.clone(), c.key), (vec![0x12], 0x73));
        assert!(parse_chord("win+l").is_err());
        assert!(parse_chord("ctrl+alt+esc").is_err());
        assert_eq!(chord_label("ctrl+shift+t"), "Ctrl + Shift + T");
        assert_eq!(chord_label("win+r"), "Win + R");
        assert_eq!(chord_label("control+Return"), "Ctrl + Enter");
    }
    #[test]
    fn paths_land_and_stay_in_time() {
        for i in 0..2000 {
            let a = ((i * 37 % 3840) as f64 - 1920.0, (i * 11 % 1080) as f64);
            let b = ((i * 53 % 3840) as f64 - 1920.0, (i * 29 % 1080) as f64);
            for p in [Path::human(a, b), Path::fast(a, b)] {
                assert_eq!(p.at(1.0), b);
                assert!(p.ms <= 300.0);
                let d = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
                // human path never strays far outside the a-b corridor
                let (mx, my) = p.at(0.5);
                let off = ((mx - (a.0 + b.0) / 2.0).powi(2) + (my - (a.1 + b.1) / 2.0).powi(2)).sqrt();
                assert!(off <= d * 0.75 + 90.0, "path strays: {off} for d {d}");
            }
        }
        let mut prev = -1.0;
        for k in 0..=100 {
            let e = ease(k as f64 / 100.0);
            assert!(e >= prev);
            prev = e;
        }
    }
}
