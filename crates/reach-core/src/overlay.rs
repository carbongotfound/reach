//! Agent cursor overlay: one click-through, layered, topmost window over the whole
//! virtual screen. It draws the agent's own arrow, a short trail, a click ripple and the
//! pill: a small dark (or light) capsule next to the arrow that says what the agent is doing
//! ("Clicking Save", "Thinking", "Needs your OK", "Done"). When an approval is waiting, the
//! pill grows into a compact card with Approve, Edit and Deny; only then does the window take
//! clicks (on the card's pixels), and it still never takes focus.
//! For UI Automation actions the arrow glides on its own while the real pointer stays with
//! you; for real mouse input it follows the real position.
//! Hidden from screen captures (the model never sees it) unless a capture asks for it.
use crate::{config, dpi, emit, Event, INCLUDE_OVERLAY_IN_CAPTURE};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use std::sync::{mpsc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use windows::core::{w, BOOL, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

static STARTED: AtomicBool = AtomicBool::new(false);
static REBUILD: AtomicBool = AtomicBool::new(false);
/// Keep the overlay visible to screen captures (tests and recordings). Env: REACH_OVERLAY_CAPTURE=1.
pub static FORCE_CAPTURE: AtomicBool = AtomicBool::new(false);
/// Set while the user has pressed "Take over" in the app: the pill says so instead of the idle-wait text.
pub static HELD: AtomicBool = AtomicBool::new(false);
/// The host's answer for the approval card's buttons: (approval id, "approve" | "edit" | "deny").
/// `reach serve` leaves it unset, and then no card is shown (it has no approvals).
pub static ANSWER: OnceLock<fn(u32, &str)> = OnceLock::new();

const IDLE_MS: u128 = 6000;
const FADE_MS: f64 = 400.0;
const TRAIL_MS: f64 = 220.0;
const RIPPLE_MS: f64 = 480.0;
/// An action label stays this long after the action, then the pill says "Thinking".
const ACT_MS: f64 = 1400.0;
const WIDTH_MS: f64 = 190.0;
const FADE_IN_MS: f64 = 180.0;
const SWAP_MS: f64 = 160.0;

/// Starts the overlay thread once. Safe to call from any binary.
pub fn start() {
    if STARTED.swap(true, SeqCst) { return; }
    let (tx, rx) = mpsc::channel::<Event>();
    crate::events::subscribe(move |e| {
        if matches!(e, Event::Pointer { .. } | Event::UserPause { .. } | Event::Status { .. } | Event::Doing { .. }) { let _ = tx.send(e.clone()); }
    });
    std::thread::Builder::new().name("reach-overlay".into()).spawn(move || {
        if let Err(e) = run(rx) { config::log(&format!("overlay: {e}")); }
    }).ok();
}

/// For tests: show the overlay at (x, y) as `agent` would.
pub fn demo(agent: &str, x: i32, y: i32, kind: &str) {
    emit(Event::Pointer { agent: agent.into(), x, y, kind: kind.into(), real: false });
}

/// Tells the pill what the run is doing: "thinking", "acting" (text = the label), "waiting"
/// (an approval; id and text = its action), "done", "stopped", "error" (text = the reason) or "idle".
pub fn status(agent: &str, state: &str, text: &str, id: u32) {
    emit(Event::Status { agent: agent.into(), state: state.into(), text: text.into(), id });
}

fn clip(s: &str, n: usize) -> String {
    let s = s.trim();
    if s.chars().count() > n { s.chars().take(n - 1).collect::<String>().trim_end().to_string() + "…" } else { s.to_string() }
}

/// What the pill says for a pointer action: verb first, under 28 characters.
/// Kinds: click, type, move, scroll, key:<chord>, and click:<name> / type:<name> from UI Automation.
pub fn action_label(kind: &str) -> String {
    let (k, name) = kind.split_once(':').unwrap_or((kind, ""));
    match (k, name.is_empty()) {
        ("key", false) => format!("Pressing {}", clip(name, 18)),
        ("click", false) => format!("Clicking {}", clip(name, 18)),
        ("type", false) => format!("Typing in {}", clip(name, 17)),
        ("click", true) => "Clicking".into(),
        ("type", true) => "Typing".into(),
        ("move", _) => "Moving".into(),
        ("scroll", _) => "Scrolling".into(),
        _ => clip(kind, 27),
    }
}

// ---------- the pill's states ----------

#[derive(Clone, Copy, PartialEq, Debug)]
enum Phase { Acting, Thinking, Waiting, Paused, Stopped, Done, Error }

impl Phase {
    fn label(self, act: &str, err: &str) -> String {
        match self {
            Phase::Acting => act.to_string(),
            Phase::Thinking => "Thinking".into(),
            Phase::Waiting => "Needs your OK".into(),
            Phase::Paused if HELD.load(SeqCst) => "You have control".into(),
            Phase::Paused => "Paused".into(),
            Phase::Stopped => "Stopped".into(),
            Phase::Done => "Done".into(),
            Phase::Error => if err.is_empty() { "Couldn't finish".into() } else { clip(err, 40) },
        }
    }
    /// How long the last state stays before the pill fades.
    fn hold_ms(self) -> Option<u128> {
        match self { Phase::Done => Some(1500), Phase::Stopped => Some(1500), Phase::Error => Some(6000), _ => None }
    }
}

/// Theme and motion settings, read from Windows every couple of seconds.
#[derive(Clone, Copy, Debug)]
struct Look { light: bool, reduce: bool }
impl Look {
    fn read() -> Look {
        use windows::Win32::System::Registry::*;
        let mut v = 0u32; let mut n = 4u32;
        let light = unsafe { RegGetValueW(HKEY_CURRENT_USER, w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"), w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD, None, Some(&mut v as *mut u32 as _), Some(&mut n)) }.is_ok() && v == 1;
        let mut anim = BOOL(1);
        let _ = unsafe { SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, Some(&mut anim as *mut BOOL as _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0)) };
        // REACH_PILL_THEME=light|dark: tests only, so captures don't need the user's Windows theme changed
        let light = match std::env::var("REACH_PILL_THEME").as_deref() { Ok("light") => true, Ok("dark") => false, _ => light };
        Look { light, reduce: !anim.as_bool() }
    }
}

/// Colours of one theme. Fill and text are neutral; colour is only on the state dot.
struct Palette { fill: u32, fill_a: f64, edge: u32, edge_a: f64, shadow_a: f64, ink: u32, soft_a: f64,
                 amber: u32, red: u32, grey: u32 }
fn palette(light: bool) -> Palette {
    if light {
        Palette { fill: 0xFFFFFF, fill_a: 0.97, edge: 0x000000, edge_a: 0.10, shadow_a: 0.16, ink: 0x111214, soft_a: 0.60,
                  amber: 0xC26A00, red: 0xCE2C31, grey: 0x80838D }
    } else {
        Palette { fill: 0x17181A, fill_a: 0.94, edge: 0xFFFFFF, edge_a: 0.10, shadow_a: 0.24, ink: 0xFFFFFF, soft_a: 0.58,
                  amber: 0xF5A524, red: 0xFF6369, grey: 0x9B9EA6 }
    }
}

unsafe extern "system" fn wndproc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match m {
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_DISPLAYCHANGE | WM_DPICHANGED => { REBUILD.store(true, SeqCst); LRESULT(0) }
        WM_LBUTTONUP => {
            let (x, y) = crate::input::cursor_pos();
            let hit = HITS.lock().unwrap().iter().find(|(r, _, _)| inside(r, x as f64, y as f64)).map(|(_, d, id)| (*d, *id));
            if let (Some((d, id)), Some(f)) = (hit, ANSWER.get()) { f(id, d); }
            LRESULT(0)
        }
        _ => DefWindowProcW(h, m, wp, lp),
    }
}

/// The card's buttons this frame: (l, t, r, b) in screen px, decision, approval id.
static HITS: Mutex<Vec<([f64; 4], &'static str, u32)>> = Mutex::new(Vec::new());
fn inside(r: &[f64; 4], x: f64, y: f64) -> bool { x >= r[0] && x < r[2] && y >= r[1] && y < r[3] }

struct Surface { dc: HDC, bmp: HBITMAP, bits: *mut u32, x: i32, y: i32, w: i32, h: i32 }
impl Surface {
    fn new(r: dpi::Rect) -> Result<Surface, String> {
        unsafe {
            let dc = CreateCompatibleDC(None);
            let mut bmi = BITMAPINFO::default();
            bmi.bmiHeader = BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: r.w, biHeight: -r.h, biPlanes: 1, biBitCount: 32, ..Default::default() };
            let mut bits = std::ptr::null_mut();
            let bmp = CreateDIBSection(Some(dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0).map_err(|e| e.to_string())?;
            SelectObject(dc, HGDIOBJ(bmp.0));
            Ok(Surface { dc, bmp, bits: bits as *mut u32, x: r.x, y: r.y, w: r.w, h: r.h })
        }
    }
    fn px(&mut self) -> &mut [u32] { unsafe { std::slice::from_raw_parts_mut(self.bits, (self.w * self.h) as usize) } }
}
impl Drop for Surface {
    fn drop(&mut self) { unsafe { let _ = DeleteDC(self.dc); let _ = DeleteObject(HGDIOBJ(self.bmp.0)); } }
}

/// Dirty box in screen px.
#[derive(Clone, Copy, Debug, PartialEq)]
struct BBox { l: i32, t: i32, r: i32, b: i32 }
impl BBox {
    const EMPTY: BBox = BBox { l: i32::MAX, t: i32::MAX, r: i32::MIN, b: i32::MIN };
    fn add(&mut self, l: f64, t: f64, r: f64, b: f64) {
        self.l = self.l.min(l.floor() as i32 - 1); self.t = self.t.min(t.floor() as i32 - 1);
        self.r = self.r.max(r.ceil() as i32 + 1); self.b = self.b.max(b.ceil() as i32 + 1);
    }
    fn empty(&self) -> bool { self.l >= self.r || self.t >= self.b }
    fn union(a: BBox, b: BBox) -> BBox { BBox { l: a.l.min(b.l), t: a.t.min(b.t), r: a.r.max(b.r), b: a.b.max(b.b) } }
}

/// Software painter on a premultiplied BGRA buffer, in screen coordinates.
struct Paint<'a> { px: &'a mut [u32], ox: i32, oy: i32, w: i32, h: i32, bb: BBox }
impl Paint<'_> {
    fn blend(&mut self, x: i32, y: i32, rgb: u32, a: f64) {
        if a <= 0.002 { return; }
        let (lx, ly) = (x - self.ox, y - self.oy);
        if lx < 0 || ly < 0 || lx >= self.w || ly >= self.h { return; }
        let a = a.min(1.0);
        let i = (ly * self.w + lx) as usize;
        let d = self.px[i];
        let inv = 1.0 - a;
        let ch = |s: u32, sh: u32| -> u32 {
            let sv = ((s >> sh) & 255) as f64 * a;
            let dv = ((d >> sh) & 255) as f64 * inv;
            ((sv + dv).round() as u32).min(255) << sh
        };
        let da = ((d >> 24) & 255) as f64 * inv + 255.0 * a;
        self.px[i] = ((da.round() as u32).min(255) << 24) | ch(rgb, 16) | ch(rgb, 8) | ch(rgb, 0);
    }
    /// Calls f(x+0.5, y+0.5) for every pixel of a box and blends the coverage it returns.
    fn fill(&mut self, l: f64, t: f64, r: f64, b: f64, rgb: u32, f: impl Fn(f64, f64) -> f64) {
        self.bb.add(l, t, r, b);
        for y in (t.floor() as i32)..=(b.ceil() as i32) {
            for x in (l.floor() as i32)..=(r.ceil() as i32) {
                let a = f(x as f64 + 0.5, y as f64 + 0.5);
                self.blend(x, y, rgb, a);
            }
        }
    }
    fn disc(&mut self, cx: f64, cy: f64, r: f64, rgb: u32, alpha: f64) {
        self.fill(cx - r - 1.0, cy - r - 1.0, cx + r + 1.0, cy + r + 1.0, rgb, |x, y| {
            alpha * (r - ((x - cx).powi(2) + (y - cy).powi(2)).sqrt() + 0.5).clamp(0.0, 1.0)
        });
    }
    fn ring(&mut self, cx: f64, cy: f64, r: f64, width: f64, rgb: u32, alpha: f64) {
        let o = r + width;
        self.fill(cx - o - 1.0, cy - o - 1.0, cx + o + 1.0, cy + o + 1.0, rgb, |x, y| {
            let d = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
            alpha * (width / 2.0 - (d - r).abs() + 0.5).clamp(0.0, 1.0)
        });
    }
    fn round_rect(&mut self, l: f64, t: f64, w: f64, h: f64, rad: f64, rgb: u32, alpha: f64) {
        self.fill(l - 1.0, t - 1.0, l + w + 1.0, t + h + 1.0, rgb, |x, y| {
            let qx = ((x - l - w / 2.0).abs() - (w / 2.0 - rad)).max(0.0);
            let qy = ((y - t - h / 2.0).abs() - (h / 2.0 - rad)).max(0.0);
            alpha * (rad - (qx * qx + qy * qy).sqrt() + 0.5).clamp(0.0, 1.0)
        });
    }
    /// Rounded rect whose edge fades over `blur` px (for soft shadows).
    fn soft_rect(&mut self, l: f64, t: f64, w: f64, h: f64, rad: f64, blur: f64, rgb: u32, alpha: f64) {
        self.fill(l - blur, t - blur, l + w + blur, t + h + blur, rgb, |x, y| {
            let qx = ((x - l - w / 2.0).abs() - (w / 2.0 - rad)).max(0.0);
            let qy = ((y - t - h / 2.0).abs() - (h / 2.0 - rad)).max(0.0);
            alpha * smooth(blur, -blur, (qx * qx + qy * qy).sqrt() - rad)
        });
    }
    /// 1 px outline of a rounded rect.
    fn ring_rect(&mut self, l: f64, t: f64, w: f64, h: f64, rad: f64, rgb: u32, alpha: f64) {
        self.fill(l - 1.0, t - 1.0, l + w + 1.0, t + h + 1.0, rgb, |x, y| {
            let qx = ((x - l - w / 2.0).abs() - (w / 2.0 - rad)).max(0.0);
            let qy = ((y - t - h / 2.0).abs() - (h / 2.0 - rad)).max(0.0);
            alpha * (1.0 - ((qx * qx + qy * qy).sqrt() - rad).abs()).clamp(0.0, 1.0)
        });
    }
    /// The agent's cursor with its tip at (tx, ty): a rounded arrowhead in `body`, a 1.5 px white
    /// outline and a soft shadow, drawn straight at device pixels (s = scale). `glow` (0..1) adds a
    /// soft accent halo behind it (thinking).
    fn cursor(&mut self, tx: f64, ty: f64, s: f64, body: u32, alpha: f64, glow: f64) { self.cursor_pass(tx, ty, s, body, alpha, glow, false) }
    /// body_only: just the fill, painted again over the tag so the two outlines read as one.
    fn cursor_pass(&mut self, tx: f64, ty: f64, s: f64, body: u32, alpha: f64, glow: f64, body_only: bool) {
        let pts: Vec<(f64, f64)> = CURSOR.iter().map(|&(x, y)| (tx + (x + 1.2) * s, ty + (y + 1.6) * s)).collect();
        let r = CURSOR_R * s;
        let sd = |x: f64, y: f64| -> f64 { signed_dist(&pts, x, y) - r };
        let (l, t, rr, b) = (tx - 8.0 * s, ty - 8.0 * s, tx + 26.0 * s, ty + 34.0 * s);
        let ow = 1.5 * s;
        if !body_only {
        if glow > 0.0 { self.fill(l, t, rr, b, body, |x, y| alpha * 0.30 * glow * smooth(7.0 * s, 0.0, sd(x, y) - ow)); }
        self.fill(l, t, rr, b, 0x000000, |x, y| alpha * 0.18 * smooth(3.0 * s, -1.0 * s, sd(x, y - 1.0 * s) - ow));
        self.fill(l, t, rr, b, 0xFFFFFF, |x, y| alpha * (0.5 - (sd(x, y) - ow)).clamp(0.0, 1.0));
        }
        self.fill(l, t, rr, b, body, |x, y| alpha * (0.5 - sd(x, y)).clamp(0.0, 1.0));
    }
    /// Rounded rect with its own radius per corner: [top-left, top-right, bottom-right, bottom-left].
    fn rrect4(&mut self, l: f64, t: f64, w: f64, h: f64, rad: [f64; 4], rgb: u32, alpha: f64) {
        let (cx, cy) = (l + w / 2.0, t + h / 2.0);
        self.fill(l - 1.0, t - 1.0, l + w + 1.0, t + h + 1.0, rgb, |x, y| {
            let r = match (x >= cx, y >= cy) { (false, false) => rad[0], (true, false) => rad[1], (true, true) => rad[2], (false, true) => rad[3] };
            let (qx, qy) = ((x - cx).abs() - (w / 2.0 - r), (y - cy).abs() - (h / 2.0 - r));
            let d = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r;
            alpha * (0.5 - d).clamp(0.0, 1.0)
        });
    }
    /// An open polyline stroke with round caps.
    fn stroke(&mut self, pts: &[(f64, f64)], width: f64, rgb: u32, alpha: f64) {
        let (mut l, mut t, mut r, mut b) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for &(x, y) in pts { l = l.min(x); t = t.min(y); r = r.max(x); b = b.max(y); }
        let hw = width / 2.0;
        self.fill(l - hw - 1.0, t - hw - 1.0, r + hw + 1.0, b + hw + 1.0, rgb, |x, y| {
            let d = pts.windows(2).map(|s| seg_dist(s[0], s[1], x, y)).fold(f64::MAX, f64::min);
            alpha * (hw - d + 0.5).clamp(0.0, 1.0)
        });
    }
    /// Text, cut off at x = clip_r.
    fn text(&mut self, x: f64, y: f64, t: &Text, rgb: u32, alpha: f64, clip_r: f64) {
        let (x0, y0) = (x.round() as i32, y.round() as i32);
        let light_ink = ((rgb >> 16 & 255) + (rgb >> 8 & 255) + (rgb & 255)) > 384;
        // GDI's greyscale coverage reads thin at 12-13 px; lift regular text (light ink most), leave bold as is
        let g = match (t.bold, light_ink) { (true, true) => 0.92, (true, false) => 1.0, (false, true) => 0.72, (false, false) => 0.86 };
        let wmax = ((clip_r - x0 as f64).floor() as i32).clamp(0, t.w);
        self.bb.add(x, y, x + wmax as f64, y + t.h as f64);
        for j in 0..t.h { for i in 0..wmax {
            let c = t.cov[(j * t.w + i) as usize];
            if c > 0 { self.blend(x0 + i, y0 + j, rgb, alpha * (c as f64 / 255.0).powf(g)); }
        } }
    }
}

fn smooth(e0: f64, e1: f64, x: f64) -> f64 { let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0); t * t * (3.0 - 2.0 * t) }
fn ease_out(t: f64) -> f64 { 1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3) }

fn seg_dist(a: (f64, f64), b: (f64, f64), x: f64, y: f64) -> f64 {
    let (ex, ey, wx, wy) = (b.0 - a.0, b.1 - a.1, x - a.0, y - a.1);
    let t = ((wx * ex + wy * ey) / (ex * ex + ey * ey).max(1e-9)).clamp(0.0, 1.0);
    ((wx - ex * t).powi(2) + (wy - ey * t).powi(2)).sqrt()
}

/// Signed distance from (x, y) to a closed polygon: negative inside.
fn signed_dist(p: &[(f64, f64)], x: f64, y: f64) -> f64 {
    let (mut d, mut inside) = (f64::MAX, false);
    let n = p.len();
    for i in 0..n {
        let (a, b) = (p[i], p[(i + 1) % n]);
        d = d.min(seg_dist(a, b, x, y));
        if (a.1 > y) != (b.1 > y) && x < a.0 + (y - a.1) / (b.1 - a.1) * (b.0 - a.0) { inside = !inside; }
    }
    if inside { -d } else { d }
}

/// Windows 11's UI face when it's installed, else Segoe UI.
fn ui_face() -> PCWSTR {
    static HAS_VARIABLE: OnceLock<bool> = OnceLock::new();
    let v = *HAS_VARIABLE.get_or_init(|| std::env::var("WINDIR").map(|w| std::path::Path::new(&w).join(r"Fonts\SegUIVar.ttf").exists()).unwrap_or(false));
    if v { w!("Segoe UI Variable Text") } else { w!("Segoe UI") }
}

/// Grayscale text coverage rendered with GDI (antialiased, not ClearType).
struct Text { w: i32, h: i32, cov: Vec<u8>, bold: bool }
fn render_text(s: &str, px: i32, weight: i32) -> Text {
    let wide: Vec<u16> = s.encode_utf16().collect();
    unsafe {
        let dc = CreateCompatibleDC(None);
        let font = CreateFontW(-px, 0, 0, 0, weight, 0, 0, 0, DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS,
            ANTIALIASED_QUALITY, (DEFAULT_PITCH.0 | FF_SWISS.0) as u32, ui_face());
        let old = SelectObject(dc, HGDIOBJ(font.0));
        let mut sz = SIZE::default();
        let _ = GetTextExtentPoint32W(dc, &wide, &mut sz);
        let (w, h) = (sz.cx.max(1), sz.cy.max(1));
        let mut bmi = BITMAPINFO::default();
        bmi.bmiHeader = BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: w, biHeight: -h, biPlanes: 1, biBitCount: 32, ..Default::default() };
        let mut bits = std::ptr::null_mut();
        let mut cov = vec![0u8; (w * h) as usize];
        if let Ok(bmp) = CreateDIBSection(Some(dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
            let ob = SelectObject(dc, HGDIOBJ(bmp.0));
            SetBkMode(dc, TRANSPARENT);
            SetTextColor(dc, COLORREF(0xFFFFFF));
            let _ = TextOutW(dc, 0, 0, &wide);
            let _ = GdiFlush();
            let p = std::slice::from_raw_parts(bits as *const u32, (w * h) as usize);
            for (c, v) in cov.iter_mut().zip(p) { *c = ((v >> 8) & 255) as u8; }
            SelectObject(dc, ob);
            let _ = DeleteObject(HGDIOBJ(bmp.0));
        }
        SelectObject(dc, old);
        let _ = DeleteObject(HGDIOBJ(font.0));
        let _ = DeleteDC(dc);
        Text { w, h, cov, bold: weight >= 600 }
    }
}

/// Rendered strings, keyed by size, weight, width limit and text.
#[derive(Default)]
struct Texts(HashMap<String, std::rc::Rc<Text>>);
impl Texts {
    /// The string at px/weight, shortened with an ellipsis to fit max_w.
    fn get(&mut self, s: &str, px: i32, weight: i32, max_w: f64) -> std::rc::Rc<Text> {
        let key = format!("{px}:{weight}:{}:{s}", max_w.min(1e6) as i32);
        if let Some(t) = self.0.get(&key) { return t.clone(); }
        if self.0.len() > 64 { self.0.clear(); }
        let mut t = render_text(s, px, weight);
        if t.w as f64 > max_w {
            let chars: Vec<char> = s.chars().collect();
            let cut = |n: usize| chars[..n].iter().collect::<String>().trim_end().to_string() + "…";
            let (mut lo, mut hi) = (0usize, chars.len());
            while lo < hi {
                let mid = (lo + hi + 1) / 2;
                if render_text(&cut(mid), px, weight).w as f64 <= max_w { lo = mid } else { hi = mid - 1 }
            }
            t = render_text(&cut(lo), px, weight);
        }
        let t = std::rc::Rc::new(t);
        self.0.insert(key, t.clone());
        t
    }
}

// ---------- drawing the pill and the card ----------

/// What one frame of the pill shows.
struct Spec<'a> { phase: Phase, label: &'a str, trail: &'a str, card: Option<(&'a str, u32)> }
/// Animation inputs: the previous colour fading out, the previous label fading out, the appear rise.
#[derive(Default)]
struct Fx { dot_prev: Option<(Phase, f64)>, label_prev: Option<(String, f64)>, rise: f64 }

/// The cursor's silhouette before rounding (1 unit = 1 px at 100 %), tip at (0, 0): a slim classic
/// arrow with a tail, about 24 px tall once rounded and outlined. The live view uses the same points
/// (app/src/ui/AgentCursor.tsx).
const CURSOR: [(f64, f64); 7] = [(0.0, 0.0), (0.0, 18.5), (4.6, 14.3), (7.6, 21.2), (10.6, 19.9), (7.6, 13.1), (13.6, 13.1)];
const CURSOR_R: f64 = 0.7;
/// The one accent (cobalt): cursor, tag, ripple. 4.9:1 with white text.
pub const ACCENT: u32 = 0x2563FF;
/// "Needs your OK" (the tag, the card's glyph): amber, 5:1 with white text.
const AMBER: u32 = 0xB45309;
const MAX_W: f64 = 260.0;
const CARD_W: f64 = 300.0;
const CARD_H: f64 = 104.0;
const EXPAND_MS: f64 = 200.0;

/// The tag's sizes at 100 %: pill height, paddings, glyph box, gap, text px and weight, offset from the tip.
/// (Design round: a tag in the accent attached at the cursor's tail won over a dark chip beside it and a
/// halo ring with a compact pill.)
struct Geo { h: f64, pad_l: f64, pad_r: f64, mark: f64, gap: f64, px: f64, weight: i32, at: (f64, f64) }
/// (Redesign 2026-11: a name tag like a collaborator's: bold label, no state glyph; the step detail is in the app's timeline.)
/// (Round 2: a slim arrow and a tag in the one accent, white semibold text, no dot or timer.)
const G: Geo = Geo { h: 22.0, pad_l: 8.0, pad_r: 8.0, mark: 0.0, gap: 0.0, px: 12.5, weight: 600, at: (11.0, 16.5) };
/// The object's colour for a state: the accent, amber when it needs you, dimmed while paused or stopped, red on error.
fn obj_col(phase: Phase) -> u32 {
    let a = ACCENT;
    match phase { Phase::Paused | Phase::Stopped => mix(a, 0x777B84, 0.45), Phase::Error => 0xD93036, Phase::Waiting => AMBER, _ => a }
}
fn mix(a: u32, b: u32, k: f64) -> u32 {
    let ch = |sh: u32| ((((a >> sh) & 255) as f64 * (1.0 - k) + ((b >> sh) & 255) as f64 * k).round() as u32) << sh;
    ch(16) | ch(8) | ch(0)
}

/// Width the pill wants for this spec (before animation), and the label's room.
fn pill_width(spec: &Spec, g: &Geo, s: f64, texts: &mut Texts) -> (f64, f64) {
    let px = (g.px * s).round() as i32;
    let trail_w = if spec.trail.is_empty() { 0.0 } else { texts.get(spec.trail, px, 400, 1e9).w as f64 + 8.0 * s };
    let mark = if spec.phase == Phase::Done { 14.0 } else { g.mark + g.gap };
    let fixed = (g.pad_l + mark + g.pad_r) * s + trail_w;
    let room = (MAX_W * s - fixed).max(40.0 * s);
    let lw = texts.get(spec.label, px, g.weight, room).w as f64;
    ((fixed + lw).ceil(), room)
}

/// The state glyph centred on (cx, cy). On the tag it's drawn in the tag's ink (white); in a neutral
/// pill it carries the colour: an accent dot (breathing halo while thinking), amber while waiting,
/// pause bars, a stop square, a check, an exclamation mark.
fn glyph(p: &mut Paint, cx: f64, cy: f64, s: f64, phase: Phase, pal: &Palette, on_tag: bool, alpha: f64, pulse: f64) {
    let ink = if on_tag { 0xFFFFFF } else { 0 };
    let col = |c: u32| if on_tag { ink } else { c };
    match phase {
        Phase::Done => {
            if !on_tag { p.disc(cx, cy, 5.5 * s, ACCENT, alpha); }
            p.stroke(&[(cx - 2.5 * s, cy + 0.1 * s), (cx - 0.7 * s, cy + 1.9 * s), (cx + 2.6 * s, cy - 1.7 * s)], 1.6 * s, 0xFFFFFF, alpha);
        }
        Phase::Error => {
            if !on_tag { p.disc(cx, cy, 5.5 * s, pal.red, alpha); }
            p.stroke(&[(cx, cy - 2.6 * s), (cx, cy + 0.6 * s)], 1.6 * s, 0xFFFFFF, alpha);
            p.disc(cx, cy + 2.6 * s, 0.9 * s, 0xFFFFFF, alpha);
        }
        Phase::Stopped => p.round_rect(cx - 3.5 * s, cy - 3.5 * s, 7.0 * s, 7.0 * s, 1.6 * s, col(pal.grey), alpha),
        Phase::Paused => {
            p.round_rect(cx - 3.5 * s, cy - 4.0 * s, 2.4 * s, 8.0 * s, 1.0 * s, col(pal.grey), alpha);
            p.round_rect(cx + 1.1 * s, cy - 4.0 * s, 2.4 * s, 8.0 * s, 1.0 * s, col(pal.grey), alpha);
        }
        _ => {
            let c = col(if phase == Phase::Waiting { pal.amber } else { ACCENT });
            let halo = match phase { Phase::Thinking => 0.12 + 0.26 * pulse, _ => 0.22 };
            p.disc(cx, cy, (4.6 + if phase == Phase::Thinking { 1.4 * pulse } else { 0.8 }) * s, c, alpha * halo);
            p.disc(cx, cy, 3.0 * s, c, alpha);
        }
    }
}

/// The card's body: like the capsule, with one small corner where it meets the cursor (`corner` as in rrect4).
fn card_body(p: &mut Paint, l: f64, t: f64, w: f64, h: f64, r: f64, corner: usize, s: f64, pal: &Palette, fill: u32, alpha: f64) {
    let mut rad = [r; 4];
    rad[corner] = (4.0 * s).min(r);
    p.soft_rect(l, t + 4.0 * s, w, h, r, 8.0 * s, 0x000000, alpha * pal.shadow_a);
    p.rrect4(l - 1.0, t - 1.0, w + 2.0, h + 2.0, rad.map(|v| v + 1.0), pal.edge, alpha * pal.edge_a);
    p.rrect4(l, t, w, h, rad, fill, alpha * pal.fill_a.max(if fill == pal.fill { 0.0 } else { 1.0 }));
}

/// Paints the pill at (l, t), width w. `corner` is the tag's corner that touches the cursor (0..3 as in rrect4).
fn paint_pill(p: &mut Paint, l: f64, t: f64, w: f64, s: f64, corner: usize, spec: &Spec, fx: &Fx, _pal: &Palette, alpha: f64, texts: &mut Texts) {
    let g = &G;
    let (l, t) = (l.round(), (t + fx.rise).round());
    let h = (g.h * s).round();
    let on_tag = true;
    let ink = 0xFFFFFF;
    {
        let mut rad = [h / 2.0; 4];
        rad[corner] = 4.0 * s;
        let fill = match fx.dot_prev { Some((old, k)) => mix(obj_col(spec.phase), obj_col(old), k), None => obj_col(spec.phase) };
        let ow = 1.5 * s;
        p.soft_rect(l, t + 1.0 * s, w, h, h / 2.0, 3.0 * s, 0x000000, alpha * 0.16);
        p.rrect4(l - ow, t - ow, w + 2.0 * ow, h + 2.0 * ow, rad.map(|r| r + ow), 0xFFFFFF, alpha);
        p.rrect4(l, t, w, h, rad, fill, alpha);
    }
    let mx = l + g.pad_l * s;
    let _ = on_tag; // the tag has no state glyph any more; glyph() stays for the card
    let px = (g.px * s).round() as i32;
    let done = spec.phase == Phase::Done;
    if done { p.stroke(&[(mx + 0.5 * s, t + h / 2.0), (mx + 3.5 * s, t + h / 2.0 + 3.0 * s), (mx + 9.0 * s, t + h / 2.0 - 3.0 * s)], 1.8 * s, ink, alpha); }
    let tx = mx + if done { 14.0 } else { g.mark + g.gap } * s;
    let clip_r = l + w - g.pad_r * s + 1.0;
    let (_, room) = pill_width(spec, g, s, texts);
    let trail = if spec.trail.is_empty() { None } else { Some(texts.get(spec.trail, px, 400, 1e9)) };
    let trail_w = trail.as_ref().map_or(0.0, |t| t.w as f64);
    let label_clip = if trail.is_some() { clip_r - trail_w - 8.0 * s } else { clip_r };
    let lt = texts.get(spec.label, px, g.weight, room);
    let ty = t + (h - lt.h as f64) / 2.0 - 0.5 * s;
    // a new label slides up 4 px into place while the old one slides out above
    let slide = 4.0 * s;
    if let Some((old, k)) = &fx.label_prev {
        let ot = texts.get(old, px, g.weight, room);
        p.text(tx, ty - slide * (1.0 - k), &ot, ink, alpha * k, label_clip);
    }
    let k = fx.label_prev.as_ref().map_or(0.0, |(_, k)| *k);
    p.text(tx, ty + slide * k, &lt, ink, alpha * (1.0 - k), label_clip);
    if let Some(tr) = trail { p.text(clip_r - trail_w - 1.0, ty, &tr, ink, alpha * 0.78, clip_r + 1.0); }
}

/// The approval card: "Needs your OK", what the agent wants to do, Approve / Edit / Deny. It grows
/// out of the pill: `k` (0..1) runs the box from the pill's size (from_w x from_h) to the card's, and the
/// content fades in over the second half. Returns the buttons' hit boxes in screen px.
fn paint_card(p: &mut Paint, l: f64, t: f64, s: f64, k: f64, from: (f64, f64), corner: usize, spec: &Spec, fx: &Fx, pal: &Palette, alpha: f64, hover: Option<(f64, f64)>, texts: &mut Texts) -> Vec<([f64; 4], &'static str, u32)> {
    let (action, id) = spec.card.unwrap_or(("", 0));
    let (l, t) = (l.round(), (t + fx.rise).round());
    let (cw, ch) = ((CARD_W * s).round(), (CARD_H * s).round());
    let (w, h) = ((from.0 + (cw - from.0) * k).round(), (from.1 + (ch - from.1) * k).round());
    let r = from.1 / 2.0 + (16.0 * s - from.1 / 2.0) * k;
    // the accent tag turns into the neutral card as it grows, keeping its small corner at the cursor
    card_body(p, l, t, w, h, r, corner, s, pal, mix(ACCENT, pal.fill, k), alpha);
    let a = alpha * smooth(0.5, 1.0, k);
    if a <= 0.0 { return vec![]; }
    let px = (12.5 * s).round() as i32;
    let pad = 12.0 * s;
    let row1 = t + pad;
    let th = texts.get(spec.label, px, 600, cw).h as f64;
    glyph(p, l + pad + 6.0 * s, row1 + th / 2.0, s, Phase::Waiting, pal, false, a, 0.0);
    let tx = l + pad + 17.0 * s;
    let trail = if spec.trail.is_empty() { None } else { Some(texts.get(spec.trail, px, 400, 1e9)) };
    let trail_w = trail.as_ref().map_or(0.0, |t| t.w as f64);
    let tt = texts.get(spec.label, px, 600, cw);
    p.text(tx, row1, &tt, pal.ink, a, l + w - pad - trail_w - 6.0 * s);
    if let Some(tr) = trail { p.text(l + w - pad - trail_w, row1, &tr, pal.ink, a * pal.soft_a, l + w); }
    let at = texts.get(action, px, 400, cw - 2.0 * pad);
    p.text(l + pad, row1 + 24.0 * s, &at, pal.ink, a * 0.76, l + w - pad);
    // buttons, left to right; Approve is the filled one
    let mut hits = vec![];
    let bh = (28.0 * s).round();
    let by = t + h - pad - bh;
    let mut bx = l + pad;
    for (label, decision) in [("Approve", "approve"), ("Edit", "edit"), ("Deny", "deny")] {
        let lt = texts.get(label, px, 600, 1e9);
        let (lw, lh) = (lt.w as f64, lt.h as f64);
        let bw = (lw + 24.0 * s).round().max(64.0 * s);
        let over = hover.is_some_and(|(x, y)| inside(&[bx, by, bx + bw, by + bh], x, y));
        let primary = decision == "approve";
        if primary {
            p.round_rect(bx, by, bw, bh, bh / 2.0, ACCENT, a * if over { 0.86 } else { 1.0 });
        } else {
            if over { p.round_rect(bx, by, bw, bh, bh / 2.0, pal.ink, a * 0.08); }
            p.ring_rect(bx + 0.5, by + 0.5, bw - 1.0, bh - 1.0, bh / 2.0 - 0.5, pal.ink, a * 0.22);
        }
        p.text(bx + (bw - lw) / 2.0, by + (bh - lh) / 2.0 - 0.5 * s, &lt, if primary { 0xFFFFFF } else { pal.ink }, a, bx + bw);
        if k >= 1.0 { hits.push(([bx, by, bx + bw, by + bh], decision, id)); }
        bx += bw + 6.0 * s;
    }
    hits
}

// ---------- state ----------

struct State {
    pos: (f64, f64),
    glide: Option<((f64, f64), (f64, f64), Instant, f64)>,
    trail: VecDeque<(f64, f64, Instant)>,
    ripples: Vec<(f64, f64, Instant)>,
    agent: String,
    paused: bool,
    last: Instant,
    shown: bool,
    born: Instant,
    fade_from: Option<Instant>,
    gen: u64,
    /// The last action label and when it happened.
    act: (String, Instant),
    /// Set by the host: the run is going (no idle fade), and an approval may be waiting.
    run_active: bool,
    waiting: Option<(String, u32, Instant)>,
    /// The run's last state (done, stopped, error) and its reason, and when it came.
    fin: Option<(Phase, String, Instant)>,
    paused_at: Instant,
    // animation
    width: (f64, f64, Instant),
    dot: (Phase, Option<(Phase, Instant)>),
    label: (String, Option<(String, Instant)>),
    off: Option<(f64, f64)>,
    frame: Instant,
    /// When the approval card started growing out of the pill.
    card_from: Option<Instant>,
    /// The pill's smoothed spot (it trails the cursor by a few px at most).
    pill_at: Option<(f64, f64)>,
    /// Agent names seen; the pill names the agent only when there's more than one.
    agents: std::collections::HashSet<String>,
}

fn ease(t: f64) -> f64 { if t < 0.5 { 4.0 * t * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(3) / 2.0 } }

impl State {
    fn new() -> State {
        let now = Instant::now();
        State { pos: (0.0, 0.0), glide: None, trail: VecDeque::new(), ripples: vec![], agent: "Agent".into(), paused: false, last: now, shown: false,
            born: now, fade_from: None, gen: crate::stop_gen(), act: (String::new(), now.checked_sub(Duration::from_secs(60)).unwrap_or(now)), run_active: false, waiting: None,
            fin: None, paused_at: now, width: (0.0, 0.0, now), dot: (Phase::Thinking, None), label: (String::new(), None), off: None, frame: now,
            card_from: None, pill_at: None, agents: Default::default() }
    }
    fn appear(&mut self, now: Instant) {
        if !self.shown { self.trail.clear(); self.born = now; self.off = None; self.pill_at = None; self.card_from = None; self.width = (0.0, 0.0, now); self.dot.1 = None; self.label = (String::new(), None); }
        self.shown = true; self.fade_from = None;
    }
    fn on(&mut self, e: Event) {
        let now = Instant::now();
        if let Event::Pointer { agent, .. } | Event::Doing { agent, .. } | Event::Status { agent, .. } = &e {
            if !agent.is_empty() && !(self.run_active && !matches!(e, Event::Status { .. })) && self.agents.len() < 8 { self.agents.insert(agent.clone()); }
        }
        match e {
            Event::Pointer { agent, x, y, kind, real } => {
                let to = (x as f64, y as f64);
                let from = self.at(now);
                if !self.shown || self.fade_from.is_some() || real {
                    self.glide = None;
                    self.pos = to;
                } else {
                    let d = ((to.0 - from.0).powi(2) + (to.1 - from.1).powi(2)).sqrt();
                    let ms = (120.0 + d * 0.35).clamp(140.0, 420.0);
                    self.glide = Some((from, to, now, ms));
                }
                if kind.starts_with("click") && config::get().cursor_ripple {
                    let delay = self.glide.map(|g| g.3).unwrap_or(0.0);
                    self.ripples.push((to.0, to.1, now + Duration::from_millis(delay as u64)));
                }
                if !self.run_active { self.agent = agent; }
                self.act = (action_label(&kind), now);
                self.fin = None;
                self.appear(now); self.last = now; self.paused = false;
            }
            Event::UserPause { agent, paused } => {
                if paused && !self.paused { self.paused_at = now; }
                self.paused = paused; self.last = now;
                // only where the agent's pointer already is: never next to the user's own cursor
                if paused && self.shown { if !self.run_active { self.agent = agent; } self.appear(now); }
            }
            Event::Doing { agent, kind } => {
                // label only; shown only if the agent's pointer is already on screen
                if !self.shown { return; }
                if !self.run_active { self.agent = agent; }
                self.act = (action_label(&kind), now); self.fin = None; self.last = now; self.appear(now);
            }
            Event::Status { agent, state, text, id } => {
                if !config::get().agent_cursor && state != "idle" { return; }
                self.agent = agent; self.last = now;
                match state.as_str() {
                    "thinking" | "acting" => {
                        self.run_active = true; self.fin = None; self.waiting = None;
                        if state == "acting" && !text.is_empty() { self.act = (clip(&text, 27), now); }
                        if self.shown { self.appear(now); } // otherwise it waits for the agent's first pointer move
                    }
                    "waiting" => {
                        self.run_active = true; self.fin = None;
                        if ANSWER.get().is_some() { self.waiting = Some((clip(&text, 60), id, now)); }
                        if self.shown { self.appear(now); } // otherwise it waits for the agent's first pointer move
                    }
                    "done" | "stopped" | "error" => {
                        self.run_active = false; self.waiting = None;
                        let ph = match state.as_str() { "done" => Phase::Done, "stopped" => Phase::Stopped, _ => Phase::Error };
                        if self.shown && self.fade_from.is_none() { self.fin = Some((ph, text, now)); }
                    }
                    _ => { self.run_active = false; self.waiting = None; self.fin = None; if self.shown && self.fade_from.is_none() { self.fade_from = Some(now); } }
                }
            }
            _ => {}
        }
    }
    fn at(&self, now: Instant) -> (f64, f64) {
        match self.glide {
            Some((a, b, t0, ms)) => {
                let t = ease((now.duration_since(t0).as_secs_f64() * 1000.0 / ms).min(1.0));
                (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
            }
            None => self.pos,
        }
    }
    fn phase(&self, now: Instant) -> Phase {
        if let Some((p, _, _)) = &self.fin { return *p; }
        if self.paused { return Phase::Paused; }
        if self.waiting.is_some() { return Phase::Waiting; }
        if now.duration_since(self.act.1).as_secs_f64() * 1000.0 < ACT_MS { Phase::Acting } else { Phase::Thinking }
    }
    /// Advances time. Returns the overall opacity (0 = hidden).
    fn tick(&mut self, now: Instant) -> f64 {
        if let Some((_, b, t0, ms)) = self.glide {
            if now.duration_since(t0).as_secs_f64() * 1000.0 >= ms { self.pos = b; self.glide = None; }
        }
        let p = self.at(now);
        if self.trail.back().map_or(true, |&(x, y, _)| (x - p.0).abs() + (y - p.1).abs() > 0.5) { self.trail.push_back((p.0, p.1, now)); }
        while self.trail.front().is_some_and(|t| now.duration_since(t.2).as_secs_f64() * 1000.0 > TRAIL_MS) { self.trail.pop_front(); }
        self.ripples.retain(|r| now < r.2 || now.duration_since(r.2).as_secs_f64() * 1000.0 < RIPPLE_MS);
        let g = crate::stop_gen();
        if g != self.gen {
            self.gen = g; self.run_active = false; self.waiting = None;
            if self.shown && self.fade_from.is_none() && self.fin.is_none() { self.fin = Some((Phase::Stopped, String::new(), now)); }
        }
        if !self.shown { return 0.0; }
        if self.fade_from.is_none() {
            let hold_over = self.fin.as_ref().is_some_and(|(ph, _, t)| ph.hold_ms().is_some_and(|h| now.duration_since(*t).as_millis() > h));
            let idle = self.fin.is_none() && !self.paused && !self.run_active && self.waiting.is_none() && now.duration_since(self.last).as_millis() > IDLE_MS;
            if hold_over || idle { self.fade_from = Some(now); }
        }
        match self.fade_from {
            Some(f) => {
                let a = 1.0 - now.duration_since(f).as_secs_f64() * 1000.0 / FADE_MS;
                if a <= 0.0 { self.shown = false; self.fade_from = None; self.glide = None; self.ripples.clear(); self.trail.clear(); self.fin = None; 0.0 } else { a }
            }
            None => 1.0,
        }
    }
    fn busy(&self) -> bool { self.shown || !self.ripples.is_empty() }
}

trait IntoF { fn into_f(self) -> (f64, f64); }
impl IntoF for (i32, i32) { fn into_f(self) -> (f64, f64) { (self.0 as f64, self.1 as f64) } }

/// The tag's short form of an action label: the verb ("Clicking Save" -> "Clicking"), keeping a key chord.
fn short_act(label: &str) -> String {
    if label.starts_with("Pressing") { return clip(label, 22); }
    label.split(' ').next().unwrap_or(label).to_string()
}

/// Draws one frame. Returns the card's buttons when the approval card is up.
fn draw(p: &mut Paint, st: &mut State, now: Instant, alpha: f64, look: Look, mons: &[dpi::Display], texts: &mut Texts) -> Vec<([f64; 4], &'static str, u32)> {
    let g = &G;
    let (x, y) = st.at(now);
    let s = dpi::dpi_scale_at(x as i32, y as i32) as f64;
    let sa = s;
    let ms = |t: Instant| now.duration_since(t).as_secs_f64() * 1000.0;
    let phase = st.phase(now);
    let col = obj_col(phase);
    // trail: fading dots between the last positions
    let pts: Vec<_> = st.trail.iter().copied().collect();
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
        let steps = (len / (2.0 * sa)).ceil().max(1.0) as i32;
        for k in 0..steps {
            let f = k as f64 / steps as f64;
            let age = ms(a.2) * (1.0 - f) + ms(b.2) * f;
            let life = (1.0 - age / TRAIL_MS).clamp(0.0, 1.0);
            if life <= 0.0 { continue; }
            p.disc(a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f, (1.0 + 2.5 * life) * sa, col, alpha * 0.22 * life);
        }
    }
    // click ripple: a small accent ring at the click point
    for &(rx, ry, t0) in &st.ripples {
        if now < t0 { continue; }
        let t = ms(t0) / RIPPLE_MS;
        let e = 1.0 - (1.0 - t).powi(3);
        p.ring(rx, ry, (3.0 + 15.0 * e) * sa, 2.0 * sa, col, alpha.max(0.6) * 0.85 * (1.0 - t).powf(1.4));
        p.disc(rx, ry, (2.5 + 4.0 * e) * sa, col, alpha.max(0.6) * 0.28 * (1.0 - t));
    }
    if !st.shown { return vec![]; }
    let pulse = if look.reduce { 0.6 } else { 0.5 - 0.5 * (ms(st.born) / 1800.0 * std::f64::consts::TAU).cos() };
    let _ = pulse;
    p.cursor(x, y, sa, col, alpha, 0.0);

    // what the tag says: the short action ("Typing") while acting, the agent's name while thinking, else
    // the state. No timer and no step detail: those are in the app's timeline.
    let err = st.fin.as_ref().map(|f| f.1.clone()).unwrap_or_default();
    let label = match phase { Phase::Acting => short_act(&st.act.0), Phase::Thinking => st.agent.clone(), _ => phase.label(&st.act.0, &err) };
    let trail = String::new();
    let waiting = st.waiting.clone();
    let card = if phase == Phase::Waiting { waiting.as_ref().map(|w| (w.0.as_str(), w.1)) } else { None };
    let spec = Spec { phase, label: &label, trail: &trail, card };

    // animation state: glyph and label swaps, width, card growth, appear
    if st.dot.0 != phase { st.dot = (phase, Some((st.dot.0, now))); }
    if st.label.0 != label { let old = std::mem::replace(&mut st.label.0, label.clone()); st.label.1 = if old.is_empty() { None } else { Some((old, now)) }; }
    let (target_w, _) = pill_width(&spec, &g, s, texts);
    let cur_w = { let (from, to, t0) = st.width; if look.reduce { to } else { from + (to - from) * ease_out(ms(t0) / WIDTH_MS) } };
    if card.is_none() && (st.width.1 - target_w).abs() > 0.5 { st.width = (if cur_w <= 0.0 { target_w } else { cur_w }, target_w, now); }
    let w = if cur_w <= 0.0 { target_w } else { cur_w }.round();
    let pill_h = (g.h * s).round();
    match (card.is_some(), st.card_from) { (true, None) => st.card_from = Some(now), (false, Some(_)) => st.card_from = None, _ => {} }
    let grow = st.card_from.map_or(0.0, |t0| if look.reduce { 1.0 } else { ease_out(ms(t0) / EXPAND_MS) });
    let (pw, ph) = if card.is_some() { (w + ((CARD_W * s).round() - w) * grow, pill_h + ((CARD_H * s).round() - pill_h) * grow) } else { (w, pill_h) };
    let appear = if look.reduce { 1.0 } else { ease_out(ms(st.born) / FADE_IN_MS) };
    let fx = Fx {
        dot_prev: st.dot.1.filter(|(_, t)| !look.reduce && ms(*t) < SWAP_MS).map(|(p, t)| (p, 1.0 - ms(t) / SWAP_MS)),
        label_prev: st.label.1.as_ref().filter(|(_, t)| !look.reduce && ms(*t) < SWAP_MS).map(|(l, t)| (l.clone(), 1.0 - ms(*t) / SWAP_MS)),
        rise: (1.0 - appear) * 4.0 * s,
    };

    // place it snug at the cursor's tail; flip near the monitor's edges, never off screen
    let mon = mons.iter().find(|m| inside(&[m.bounds.x as f64, m.bounds.y as f64, m.bounds.right() as f64, m.bounds.bottom() as f64], x, y))
        .map(|m| m.work).unwrap_or(dpi::Rect { x: -100_000, y: -100_000, w: 200_000, h: 200_000 });
    let (mut ox, mut oy) = (g.at.0 * sa, g.at.1 * sa);
    let (left, up) = (x + ox + pw > mon.right() as f64 - 4.0 * s, y + oy + ph > mon.bottom() as f64 - 4.0 * s);
    if left { ox = -pw + 1.5 * sa; }
    if up { oy = -ph + 1.0 * sa; }
    let corner = match (left, up) { (false, false) => 0, (true, false) => 1, (false, true) => 3, (true, true) => 2 };
    let dt = now.duration_since(st.frame).as_secs_f64();
    let k = if look.reduce { 1.0 } else { 1.0 - (-dt / 0.05).exp() }; // flips: critically damped, ~150 ms, no overshoot
    let off = match st.off { Some((ax, ay)) => (ax + (ox - ax) * k, ay + (oy - ay) * k), None => (ox, oy) };
    st.off = Some(off); st.frame = now;
    let target = (x + off.0, y + off.1);
    // a light spring on the whole pill: it trails the cursor by at most 4 px, never overshoots
    let at = match st.pill_at {
        Some((ax, ay)) if !look.reduce => {
            let k = 1.0 - (-dt / 0.016).exp();
            let (mut nx, mut ny) = (ax + (target.0 - ax) * k, ay + (target.1 - ay) * k);
            let (dx, dy) = (target.0 - nx, target.1 - ny);
            let d = (dx * dx + dy * dy).sqrt();
            let lim = 4.0 * s;
            if d > lim { nx = target.0 - dx * lim / d; ny = target.1 - dy * lim / d; }
            (nx, ny)
        }
        _ => target,
    };
    st.pill_at = Some(at);
    let l = at.0.clamp(mon.x as f64 + 4.0, (mon.right() as f64 - pw - 4.0).max(mon.x as f64));
    let t = at.1.clamp(mon.y as f64 + 4.0, (mon.bottom() as f64 - ph - 4.0).max(mon.y as f64));
    let pal = palette(look.light);
    let a = alpha * appear;
    if card.is_some() {
        let hover = Some(crate::input::cursor_pos().into_f());
        let hits = paint_card(p, l, t, s, grow, (w, pill_h), corner, &spec, &fx, &pal, a, hover, texts);
        p.cursor_pass(x, y, sa, col, alpha, 0.0, true);
        hits
    } else {
        paint_pill(p, l, t, w, s, corner, &spec, &fx, &pal, a, texts);
        p.cursor_pass(x, y, sa, col, alpha, 0.0, true);
        vec![]
    }
}

fn run(rx: mpsc::Receiver<Event>) -> Result<(), String> {
    if std::env::var("REACH_OVERLAY_CAPTURE").is_ok_and(|v| v == "1") { FORCE_CAPTURE.store(true, SeqCst); }
    unsafe {
        let hinst = GetModuleHandleW(None).map_err(|e| e.to_string())?;
        let cls = w!("ReachAgentOverlay");
        let wc = WNDCLASSEXW { cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32, lpfnWndProc: Some(wndproc), hInstance: hinst.into(), lpszClassName: cls,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(), ..Default::default() };
        RegisterClassExW(&wc);
        let vs = dpi::ScreenMap::virtual_screen();
        let ex = WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW;
        let hwnd = CreateWindowExW(ex, cls, PCWSTR::null(), WS_POPUP, vs.x, vs.y, vs.w, vs.h, None, None, Some(hinst.into()), None).map_err(|e| e.to_string())?;
        let mut surf = Surface::new(vs)?;
        let mut affinity: Option<bool> = None;
        let mut visible = false;
        let mut clickable = false;
        let mut prev = BBox::EMPTY;
        let mut texts = Texts::default();
        let mut last_top = Instant::now();
        let mut look = Look::read();
        let mut look_at = Instant::now();
        let mut mons = dpi::displays();
        let mut st = State::new();
        loop {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() { let _ = TranslateMessage(&msg); DispatchMessageW(&msg); }
            // animating: one frame per display refresh; idle: wake for events
            let wait = if st.busy() { let _ = windows::Win32::Graphics::Dwm::DwmFlush(); 0 } else { 100 };
            match rx.recv_timeout(Duration::from_millis(wait)) {
                Ok(e) => { st.on(e); while let Ok(e) = rx.try_recv() { st.on(e); } }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => return Ok(()),
            }
            let cap = FORCE_CAPTURE.load(SeqCst) || INCLUDE_OVERLAY_IN_CAPTURE.load(SeqCst);
            if affinity != Some(cap) {
                let _ = SetWindowDisplayAffinity(hwnd, if cap { WDA_NONE } else { WDA_EXCLUDEFROMCAPTURE });
                affinity = Some(cap);
            }
            if REBUILD.swap(false, SeqCst) {
                let vs = dpi::ScreenMap::virtual_screen();
                mons = dpi::displays();
                if let Ok(s) = Surface::new(vs) { surf = s; prev = BBox::EMPTY;
                    let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), vs.x, vs.y, vs.w, vs.h, SWP_NOACTIVATE); }
            }
            if !st.busy() && !visible { continue; }
            let now = Instant::now();
            if look_at.elapsed() > Duration::from_secs(2) { look = Look::read(); look_at = now; }
            let alpha = st.tick(now);
            let (ox, oy, w, h) = (surf.x, surf.y, surf.w, surf.h);
            let px = surf.px();
            // clear last frame's box, draw the new one
            if !prev.empty() {
                for y in prev.t.max(oy)..prev.b.min(oy + h) {
                    let row = ((y - oy) * w) as usize;
                    let (a, b) = ((prev.l.max(ox) - ox) as usize, (prev.r.min(ox + w) - ox) as usize);
                    if a < b { px[row + a..row + b].fill(0); }
                }
            }
            let mut p = Paint { px, ox, oy, w, h, bb: BBox::EMPTY };
            let hits = if st.busy() { draw(&mut p, &mut st, now, alpha, look, &mons, &mut texts) } else { vec![] };
            // the card takes clicks on its own pixels (transparent pixels still pass through); otherwise click-through
            let want = !hits.is_empty();
            *HITS.lock().unwrap() = hits;
            if want != clickable {
                let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                let t = WS_EX_TRANSPARENT.0 as isize;
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, if want { ex & !t } else { ex | t });
                clickable = want;
            }
            let cur = p.bb;
            let dirty = BBox::union(prev, cur);
            prev = cur;
            if !visible && st.busy() {
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
                visible = true;
            }
            if !dirty.empty() {
                let r = RECT { left: (dirty.l - ox).max(0), top: (dirty.t - oy).max(0), right: (dirty.r - ox).min(w), bottom: (dirty.b - oy).min(h) };
                let (pt, src, size) = (POINT { x: ox, y: oy }, POINT::default(), SIZE { cx: w, cy: h });
                let blend = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA as u8 };
                let info = UPDATELAYEREDWINDOWINFO { cbSize: std::mem::size_of::<UPDATELAYEREDWINDOWINFO>() as u32, hdcDst: HDC::default(), pptDst: &pt,
                    psize: &size, hdcSrc: surf.dc, pptSrc: &src, crKey: COLORREF(0), pblend: &blend, dwFlags: ULW_ALPHA, prcDirty: &r };
                if !UpdateLayeredWindowIndirect(hwnd, &info).as_bool() {
                    // first update (or after a resize) must cover the whole surface
                    let info = UPDATELAYEREDWINDOWINFO { prcDirty: std::ptr::null(), ..info };
                    let _ = UpdateLayeredWindowIndirect(hwnd, &info);
                }
            }
            if visible && !st.busy() { let _ = ShowWindow(hwnd, SW_HIDE); visible = false; }
            if visible && last_top.elapsed() > Duration::from_secs(2) {
                let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER);
                last_top = Instant::now();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn arrow_inside_outside() {
        let p = CURSOR;
        assert!(signed_dist(&p, 2.0, 10.0) < 0.0);
        assert!(signed_dist(&p, 10.0, 2.0) > 0.0);
        assert!((signed_dist(&p, -3.0, 5.0) - 3.0).abs() < 1e-9);
    }
    #[test]
    fn labels() {
        assert_eq!(action_label("click"), "Clicking");
        assert_eq!(action_label("click:Save"), "Clicking Save");
        assert_eq!(action_label("key:Ctrl + S"), "Pressing Ctrl + S");
        assert_eq!(action_label("type:Search box"), "Typing in Search box");
        for k in ["click:A very long button name that goes on", "type:Another very long field name here", "key:Ctrl + Shift + Alt + F12 + something"] {
            assert!(action_label(k).chars().count() < 28, "{}", action_label(k));
        }
        assert_eq!(Phase::Waiting.label("", ""), "Needs your OK");
        assert_eq!(Phase::Error.label("", ""), "Couldn't finish");
    }
    #[test]
    fn never_appears_at_the_users_cursor() {
        let mut st = State::new();
        st.on(Event::UserPause { agent: "Claude".into(), paused: true });
        assert!(!st.shown, "a pause before the agent has moved shows nothing");
        st.on(Event::Status { agent: "Claude".into(), state: "thinking".into(), text: String::new(), id: 0 });
        assert!(!st.shown, "a run starting shows nothing until the agent's pointer moves");
        st.on(Event::Doing { agent: "Claude".into(), kind: "key:Ctrl+S".into() });
        assert!(!st.shown, "a key press shows nothing until the agent's pointer moves");
        st.on(Event::Pointer { agent: "Claude Code".into(), x: 500, y: 300, kind: "click".into(), real: false });
        st.on(Event::Doing { agent: "Claude".into(), kind: "key:Ctrl+S".into() });
        assert_eq!(st.pos, (500.0, 300.0), "a key press leaves the pointer where it was");
        assert_eq!(st.act.0, "Pressing Ctrl+S");
    }

    #[test]
    fn phases_and_holds() {
        let mut st = State::new();
        st.shown = true;
        let now = Instant::now();
        assert_eq!(st.phase(now), Phase::Thinking);
        st.act = ("Clicking".into(), now);
        assert_eq!(st.phase(now), Phase::Acting);
        st.waiting = Some(("Press Send".into(), 3, now));
        assert_eq!(st.phase(now), Phase::Waiting);
        st.paused = true;
        assert_eq!(st.phase(now), Phase::Paused);
        st.fin = Some((Phase::Done, String::new(), now));
        assert_eq!(st.phase(now), Phase::Done);
        // a running task never fades on its own; a finished one fades after its hold
        st.fin = None; st.paused = false; st.waiting = None; st.run_active = true;
        st.last = now - Duration::from_secs(60);
        assert_eq!(st.tick(now), 1.0);
        st.run_active = false;
        st.fin = Some((Phase::Done, String::new(), now - Duration::from_millis(3100)));
        st.tick(now);
        assert!(st.fade_from.is_some());
    }
    /// Writes one raw frame (u32 w, u32 h, then premultiplied BGRA) to tests/overlay/pill/<name>.rgba,
    /// for tests/overlay/pill_sheet.py to put on backgrounds.
    fn save_frame(name: &str, w: i32, h: i32, f: impl FnOnce(&mut Paint)) {
        let mut buf = vec![0u32; (w * h) as usize];
        let mut p = Paint { px: &mut buf, ox: 0, oy: 0, w, h, bb: BBox::EMPTY };
        f(&mut p);
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/overlay/pill");
        std::fs::create_dir_all(&dir).unwrap();
        let mut out = Vec::with_capacity(8 + buf.len() * 4);
        out.extend_from_slice(&(w as u32).to_le_bytes()); out.extend_from_slice(&(h as u32).to_le_bytes());
        for v in buf { out.extend_from_slice(&v.to_le_bytes()); }
        std::fs::write(dir.join(format!("{name}.rgba")), out).unwrap();
    }
    /// Every state of the pill and the approval card, light and dark, at 100, 125 and 150 %.
    /// REACH_PILL_ROUND=<n> names the files r<n>-...
    #[test]
    #[ignore]
    fn pill_frames() {
        let round = std::env::var("REACH_PILL_ROUND").unwrap_or_else(|_| "x".into());
        let rows: [(&str, Phase, &str, &str); 8] = [
            ("1-thinking", Phase::Thinking, "Claude", ""),
            ("2-acting", Phase::Acting, "Claude", ""),
            ("3-typing", Phase::Acting, "Typing", ""),
            ("4-waiting", Phase::Waiting, "Needs your OK", ""),
            ("5-paused", Phase::Paused, "Paused", ""),
            ("6-stopped", Phase::Stopped, "Stopped", ""),
            ("7-done", Phase::Done, "Done", ""),
            ("8-error", Phase::Error, "Notepad closed before I could save", ""),
        ];
        let mut texts = Texts::default();
        {
            let sname = "cobalt";
            let g = &G;
            for light in [false, true] {
                let pal = palette(light);
                for k in [1.0, 1.25, 1.5] {
                    let tag = format!("{sname}-{}-{}", if light { "light" } else { "dark" }, (k * 100.0) as i32);
                    let sa = k;
                    for (name, phase, label, trail) in rows {
                        let spec = Spec { phase, label, trail, card: None };
                        let (w, _) = pill_width(&spec, g, k, &mut texts);
                        let pulse = 1.0;
                        save_frame(&format!("r{round}-{tag}-{name}"), (330.0 * k) as i32, (64.0 * k) as i32, |p| {
                            let glow = if phase == Phase::Thinking { pulse } else { 0.0 };
                            p.cursor(14.0, 10.0, sa, obj_col(phase), 1.0, glow);
                            paint_pill(p, 14.0 + g.at.0 * sa, 10.0 + g.at.1 * sa, w, k, 0, &spec, &Fx::default(), &pal, 1.0, &mut texts);
                            p.cursor_pass(14.0, 10.0, sa, obj_col(phase), 1.0, 0.0, true);
                        });
                    }
                    let spec = Spec { phase: Phase::Waiting, label: "Needs your OK", trail: "", card: Some(("Press Send in Mail (to: test@example.com)", 1)) };
                    save_frame(&format!("r{round}-{tag}-9-card"), (330.0 * k) as i32, (150.0 * k) as i32, |p| {
                        p.cursor(14.0, 10.0, sa, ACCENT, 1.0, 0.0);
                        let _ = paint_card(p, 14.0 + g.at.0 * sa, 10.0 + g.at.1 * sa, k, 1.0, (100.0, g.h * k), 0, &spec, &Fx::default(), &pal, 1.0, None, &mut texts);
                        p.cursor_pass(14.0, 10.0, sa, ACCENT, 1.0, 0.0, true);
                    });
                }
            }
        }
    }
    #[test]
    fn paints_premultiplied() {
        let mut buf = vec![0u32; 100];
        let mut p = Paint { px: &mut buf, ox: 0, oy: 0, w: 10, h: 10, bb: BBox::EMPTY };
        p.disc(5.0, 5.0, 3.0, 0xFF0000, 1.0);
        let c = buf[55];
        assert_eq!(c >> 24, 255);
        assert_eq!((c >> 16) & 255, 255);
        assert_eq!(buf[0], 0);
    }
}
