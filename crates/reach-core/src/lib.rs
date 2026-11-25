//! reach-core: Windows desktop control for AI agents. Capture, mouse, keyboard,
//! UI Automation and window management behind one `Engine::run(tool, args)`.
//! Windows 10/11 only.

pub mod apps;
pub mod brand;
pub mod capture;
pub mod config;
pub mod dpi;
pub mod guard;
pub mod input;
pub mod overlay;
pub mod reg;
#[cfg(feature = "server")]
pub mod server;
pub mod uia;
pub mod util;
pub mod yield_;

use dpi::{Rect, ScreenMap};
use serde_json::Value;
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::SeqCst};
use std::sync::{mpsc, Mutex, RwLock};
use std::time::Instant;
use util::{bool_, int, ints, num, s, s_any, short, Args};

pub const STOPPED: &str = "Stopped.";

// ---------- stop ----------
static STOP_GEN: AtomicU64 = AtomicU64::new(0);

/// Halt the action in progress and everything queued before this call.
pub fn stop() { STOP_GEN.fetch_add(1, SeqCst); }
pub fn stop_gen() -> u64 { STOP_GEN.load(SeqCst) }
pub fn cancelled() -> bool { ctx().gen != stop_gen() }

// ---------- per-call context (engine thread) ----------
#[derive(Clone, Default)]
pub struct Ctx {
    pub agent: String,
    pub no_cursor: bool,
    pub no_yield: bool,
    pub fast: bool,
    pub gen: u64,
}
thread_local!(static CTX: RefCell<Ctx> = RefCell::new(Ctx { gen: u64::MAX, ..Default::default() }));
pub fn ctx() -> Ctx {
    CTX.with(|c| {
        let mut c = c.borrow().clone();
        if c.gen == u64::MAX { c.gen = stop_gen(); } // outside a call: never cancelled
        c
    })
}

// ---------- events (overlay cursor, tray, app step view) ----------
#[derive(Clone, Debug)]
pub enum Event {
    /// The agent is pointing at (x, y) screen px. real = the real mouse is there too.
    Pointer { agent: String, x: i32, y: i32, kind: String, real: bool },
    UserPause { agent: String, paused: bool },
    Action { tool: String, action: String, args: Args, ok: bool, text: String, ms: u64 },
    /// What a run is doing, for the pill: thinking, acting, waiting (an approval, `id`), done, stopped, error, idle.
    Status { agent: String, state: String, text: String, id: u32 },
    /// A step with no place on screen (a key press, typing into the focused control): the pill
    /// says what's happening and the agent's pointer stays where it last was.
    Doing { agent: String, kind: String },
}
type Sink = Box<dyn Fn(&Event) + Send + Sync>;
static SINKS: RwLock<Vec<Sink>> = RwLock::new(Vec::new());
pub mod events {
    pub fn subscribe(f: impl Fn(&super::Event) + Send + Sync + 'static) { super::SINKS.write().unwrap().push(Box::new(f)); }
    pub fn emit(e: super::Event) { for f in super::SINKS.read().unwrap().iter() { f(&e); } }
}
pub use events::emit;

/// Update the pill's action label without moving the agent's pointer (keyboard steps).
pub fn emit_action(kind: &str) {
    let c = ctx();
    if c.no_cursor || !config::get().agent_cursor { return; }
    emit(Event::Doing { agent: c.agent, kind: kind.into() });
}

/// Show the agent cursor at (x, y) unless disabled for this call or globally.
pub fn emit_pointer(x: i32, y: i32, kind: &str, real: bool) {
    let c = ctx();
    if c.no_cursor || !config::get().agent_cursor { return; }
    emit(Event::Pointer { agent: c.agent, x, y, kind: kind.into(), real });
}

/// Debug: let screen captures see the overlay.
pub static INCLUDE_OVERLAY_IN_CAPTURE: AtomicBool = AtomicBool::new(false);

// ---------- results ----------
#[derive(Clone, Debug)]
pub struct ActResult {
    pub ok: bool,
    pub text: String,
    pub image: Option<Vec<u8>>,
    pub mime: &'static str,
    pub image_path: Option<String>,
}
impl ActResult {
    pub fn say(t: impl Into<String>) -> Self { ActResult { ok: true, text: t.into(), image: None, mime: "image/jpeg", image_path: None } }
    pub fn fail(t: impl Into<String>) -> Self { ActResult { ok: false, ..Self::say(t) } }
}
type R<T> = Result<T, String>;

// ---------- engine ----------
struct Job { tool: String, args: Args, gen: u64, reply: mpsc::Sender<ActResult> }

/// Runs every tool call on one dedicated COM (MTA) thread, in order.
#[derive(Clone)]
pub struct Engine { tx: mpsc::Sender<Job> }

impl Engine {
    pub fn start() -> Engine {
        let (tx, rx) = mpsc::channel::<Job>();
        std::thread::Builder::new().name("reach-engine".into()).spawn(move || {
            unsafe { let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED); }
            let mut w = Worker { uia: None, last_hash: 0 };
            for job in rx {
                let r = if job.gen != stop_gen() { ActResult::fail(STOPPED) } else { w.run(&job.tool, &job.args, job.gen) };
                let _ = job.reply.send(r);
            }
        }).expect("engine thread");
        Engine { tx }
    }

    /// Run one tool call. Blocks until done.
    pub fn run(&self, tool: &str, args: Args) -> ActResult {
        let gen = stop_gen();
        let t0 = Instant::now();
        let tool_l = tool.to_ascii_lowercase();
        let action = s(&args, "action", "");
        if tool_l == "apps" && matches!(action.as_str(), "open" | "launch" | "start") && config::get().agent_cursor && !ctx().no_cursor {
            let name = s_any(&args, &["name", "app", "text", "query"], "");
            if !name.is_empty() { emit(Event::Status { agent: agent_name(&args), state: "acting".into(), text: format!("Opening {}", nice_app_name(&name)), id: 0 }); }
        }
        let r = self.run_inner(&tool_l, &action, args.clone(), gen);
        let ms = t0.elapsed().as_millis();
        config::record_usage(tool, &action, ms, r.ok, &short(&r.text));
        emit(Event::Action { tool: tool_l, action, args, ok: r.ok, text: r.text.clone(), ms: ms as u64 });
        r
    }

    fn run_inner(&self, tool: &str, action: &str, args: Args, gen: u64) -> ActResult {
        if config::get().paused && !pause_exempt(tool, action) {
            return ActResult::fail("Reach is paused. Resume it from the tray icon, or run `reach resume`.");
        }
        // Light tools answer here so status stays responsive while the engine is busy.
        if let Some(r) = light(tool, action, &args) { return r; }
        let mut waited = 0;
        if !bool_(&args, "no_yield", false) && yield_::gated(tool, action) {
            match yield_::wait_for_idle(&agent_name(&args)) {
                Ok(ms) => waited = ms,
                Err(e) => return ActResult::fail(e),
            }
        }
        let (tx, rx) = mpsc::channel();
        if self.tx.send(Job { tool: tool.into(), args, gen, reply: tx }).is_err() {
            return ActResult::fail("engine stopped");
        }
        let mut r = rx.recv().unwrap_or_else(|_| ActResult::fail("no result"));
        if waited > 0 && r.ok {
            r.text = format!("{}  (waited {:.1} s for you to stop using the PC)", r.text, waited as f64 / 1000.0);
        }
        r
    }
}

fn pause_exempt(tool: &str, action: &str) -> bool {
    // "resume" as a tool: the C# build blocked `reach resume` while paused (its own hint said to run it).
    matches!(tool, "status" | "usage" | "pause_on_user" | "motion" | "resume" | "pause")
        || (tool == "apps" && matches!(action, "resume" | "status"))
        || matches!(action, "resume" | "status" | "usage")
}

/// "notepad.exe" -> "Notepad", for the pill.
fn nice_app_name(n: &str) -> String {
    let n = n.trim().trim_end_matches(".exe").trim_end_matches(".EXE");
    let mut c = n.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

pub fn agent_name(a: &Args) -> String {
    let mut n = s(a, "agent", "");
    if n.trim().is_empty() { n = "Agent".into(); }
    let n = n.trim().to_string();
    if n.chars().count() > 28 { n.chars().take(27).collect::<String>() + "…" } else { n }
}

fn light(tool: &str, action: &str, a: &Args) -> Option<ActResult> {
    let r = match tool {
        "status" => status(bool_(a, "json", false)),
        "usage" => ActResult::say(config::usage_tail(int(a, "limit", 20).max(0) as usize)),
        "pause" => { config::update(|c| c.paused = true); ActResult::say("paused") }
        "resume" => { config::update(|c| c.paused = false); ActResult::say("resumed") }
        "motion" => {
            let v = s_any(a, &["value", "action"], "").to_lowercase();
            if matches!(v.as_str(), "smooth" | "fast" | "instant") { config::update(|c| c.motion = v.clone()); }
            else if !v.is_empty() && v != "status" { return Some(ActResult::fail("motion is smooth, fast or instant")); }
            ActResult::say(format!("motion {}", input::motion_mode()))
        }
        "pause_on_user" => pause_on_user(action, a).unwrap_or_else(ActResult::fail),
        "cursor" => cursor_cmd(action, a),
        _ => return None,
    };
    Some(r)
}

pub fn status(json: bool) -> ActResult {
    let c = config::get();
    let map = ScreenMap::build(c.max_dim);
    let n = dpi::displays().len();
    let pid = std::process::id();
    let mb = mem_bytes() / (1024 * 1024);
    if json {
        return ActResult::say(serde_json::json!({
            "version": config::VERSION, "paused": c.paused, "port": c.port, "cursor": c.agent_cursor,
            "pauseOnUser": c.pause_on_user, "image": format!("{}x{}", map.out_w, map.out_h), "displays": n,
            "pid": pid, "ramKB": mem_bytes() / 1024,
        }).to_string());
    }
    ActResult::say(format!("reach {} {} port {} cursor {} yield {} image {}x{} displays {} pid {} {}MB",
        config::VERSION, if c.paused { "paused" } else { "running" }, c.port, onoff(c.agent_cursor), onoff(c.pause_on_user),
        map.out_w, map.out_h, n, pid, mb))
}

fn onoff(b: bool) -> &'static str { if b { "on" } else { "off" } }

fn mem_bytes() -> u64 {
    use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    let mut pmc = PROCESS_MEMORY_COUNTERS::default();
    unsafe {
        let _ = GetProcessMemoryInfo(windows::Win32::System::Threading::GetCurrentProcess(), &mut pmc, std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32);
    }
    pmc.WorkingSetSize as u64
}

fn pause_on_user(action: &str, a: &Args) -> R<ActResult> {
    let v = int(a, "value", 0);
    match if action.is_empty() { "status".to_string() } else { action.to_lowercase() }.as_str() {
        "on" => config::update(|c| c.pause_on_user = true),
        "off" => config::update(|c| c.pause_on_user = false),
        "idle" => { if v < 100 { return Err("usage: reach pause-on-user idle <ms>".into()); } config::update(|c| c.user_pause_idle_ms = v) }
        "timeout" => { if v < 1000 { return Err("usage: reach pause-on-user timeout <ms> (min 1000)".into()); } config::update(|c| c.user_pause_timeout_ms = v) }
        "simulate" => { let ms = if v > 0 { v } else { 1000 }; yield_::simulate(ms as u64); return Ok(ActResult::say(format!("simulating your input for {ms} ms"))); }
        "status" => {}
        _ => return Err("usage: reach pause-on-user on|off|status|idle <ms>|timeout <ms>".into()),
    }
    let c = config::get();
    let idle = yield_::idle_ms();
    Ok(ActResult::say(format!("pause-on-user {}  idle {} ms  timeout {} ms  hooks {}  input-lock {}  you were last active {} ago",
        onoff(c.pause_on_user), c.user_pause_idle_ms, c.user_pause_timeout_ms, if yield_::hooks_ok() { "ok" } else { "MISSING" },
        if yield_::input_lock_running() { "running (ignored)" } else { "off" },
        if idle > 3_600_000 { "over an hour".to_string() } else { format!("{:.1} s", idle as f64 / 1000.0) })))
}

fn cursor_cmd(action: &str, a: &Args) -> ActResult {
    match action.to_lowercase().as_str() {
        "ripple" => {
            let v = s(a, "value", "").to_lowercase();
            if v == "on" || v == "off" { config::update(|c| c.cursor_ripple = v == "on"); }
            ActResult::say(format!("click ripple {}", onoff(config::get().cursor_ripple)))
        }
        "on" | "enable" => { config::update(|c| c.agent_cursor = true); ActResult::say("agent cursor on") }
        "off" | "disable" => { config::update(|c| c.agent_cursor = false); ActResult::say("agent cursor off") }
        "capture" => {
            let on = s(a, "value", "").eq_ignore_ascii_case("on");
            INCLUDE_OVERLAY_IN_CAPTURE.store(on, SeqCst);
            ActResult::say(format!("pill visible to capture: {}", if on { "yes" } else { "no" }))
        }
        "show" | "test" => {
            let k = s(a, "value", "click").to_lowercase();
            let k = if k == "type" || k == "move" { k } else { "click".into() };
            let agent = agent_name(a);
            let (x, y) = input::cursor_pos();
            if config::get().agent_cursor { emit(Event::Pointer { agent: agent.clone(), x, y, kind: k, real: true }); }
            ActResult::say(format!("showing agent cursor for {agent}"))
        }
        _ => { let c = config::get(); ActResult::say(format!("agent cursor {}, ripple {}", onoff(c.agent_cursor), onoff(c.cursor_ripple))) }
    }
}

struct Worker { uia: Option<uia::Uia>, last_hash: u64 }

impl Worker {
    fn run(&mut self, tool: &str, a: &Args, gen: u64) -> ActResult {
        CTX.with(|c| *c.borrow_mut() = Ctx {
            agent: agent_name(a), no_cursor: bool_(a, "no_cursor", false), no_yield: bool_(a, "no_yield", false), fast: bool_(a, "fast", false), gen,
        });
        let action = s(a, "action", "");
        let r = match tool {
            "screenshot" => self.shot(a),
            "mouse" => mouse(&action, a).map(ActResult::say),
            "keyboard" => keyboard(&action, a).map(ActResult::say),
            "apps" => apps_cmd(&action, a).map(ActResult::say),
            "ui" => self.ui(&action, a),
            _ => Err(format!("Unknown tool `{tool}`. Use screenshot, mouse, keyboard, apps, ui, status, usage, pause, resume.")),
        };
        CTX.with(|c| c.borrow_mut().gen = u64::MAX);
        r.unwrap_or_else(ActResult::fail)
    }

    fn shot(&mut self, a: &Args) -> R<ActResult> {
        let c = config::get();
        let force = bool_(a, "force", false);
        let region = ints(a, "region").filter(|r| r.len() == 4);
        let window = s(a, "window", "");
        let max_dim = (int(a, "max", c.max_dim as i64) as i32).clamp(200, 4096);
        let map = ScreenMap::build(c.max_dim);
        let out_map = if max_dim == c.max_dim { map } else { ScreenMap::build(max_dim) };
        let mut src = if let Some(r) = &region {
            let (x, y) = map.to_screen(r[0], r[1]);
            Rect { x, y, w: 1.max(dpi::round(r[2] as f64 / map.scale)), h: 1.max(dpi::round(r[3] as f64 / map.scale)) }
        } else if !window.is_empty() {
            let h = apps::parse_hwnd(&window)?;
            apps::frame_rect(h).ok_or_else(|| "Could not get window bounds.".to_string())?
        } else { map.virt };
        let v = map.virt;
        src = src.intersect(&Rect::from_ltrb(v.x - 10000, v.y - 10000, v.right() + 10000, v.bottom() + 10000));
        if src.w < 1 || src.h < 1 { return Err("Empty capture region.".into()); }
        let full = src == map.virt && region.is_none();
        let (out_w, out_h) = if full { (out_map.out_w, out_map.out_h) }
            else if src.w.max(src.h) > c.max_dim {
                let s2 = c.max_dim as f64 / src.w.max(src.h) as f64;
                (1.max(dpi::round(src.w as f64 * s2)), 1.max(dpi::round(src.h as f64 * s2)))
            } else { (src.w, src.h) };
        let with_overlay = bool_(a, "include_overlay", false);
        if with_overlay { INCLUDE_OVERLAY_IN_CAPTURE.store(true, SeqCst); std::thread::sleep(std::time::Duration::from_millis(120)); }
        // fenced (tests): only the test windows, each drawn by itself, so nothing else is ever captured
        let px = if apps::fence_on() && !with_overlay { apps::grab_fenced(src, out_w, out_h) } else { capture::grab(src, out_w, out_h) };
        if with_overlay { INCLUDE_OVERLAY_IN_CAPTURE.store(false, SeqCst); }
        let q = int(a, "quality", 0);
        let quality = if q > 0 { q.clamp(20, 95) as u8 } else if region.is_some() { 80 } else { 72 };
        let jpeg = capture::jpeg(&px?, out_w, out_h, quality)?;
        let h = capture::hash(&jpeg);
        if !force && !with_overlay && h == self.last_hash {
            return Ok(ActResult::say(format!("unchanged {out_w}x{out_h}")));
        }
        self.last_hash = h;
        let path = config::path(if region.is_some() { "zoom.jpg" } else { "last.jpg" });
        let _ = std::fs::write(&path, &jpeg);
        let (cx, cy) = input::cursor_pos();
        let (ix, iy) = map.to_image(cx, cy);
        let mut t = format!("{out_w}x{out_h}{}", if full { "" } else { " region" });
        if full && out_map.out_w != map.out_w { t.push_str(&format!(" (scaled; click coords use {}x{} space)", map.out_w, map.out_h)); }
        if !full {
            // mouse coordinates are always in the full-desktop space, so say where this image sits in it:
            // pixel (px, py) here is (x0 + px * cw / out_w, y0 + py * ch / out_h) for the mouse
            let (x0, y0) = map.to_image(src.x, src.y);
            let (x1, y1) = map.to_image(src.right(), src.bottom());
            t.push_str(&format!(" = click box {x0},{y0} {}x{}", x1 - x0, y1 - y0));
        }
        t.push_str(&format!(" cursor {ix},{iy}"));
        if full {
            for d in dpi::displays() {
                let (x0, y0) = map.to_image(d.bounds.x, d.bounds.y);
                let (x1, y1) = map.to_image(d.bounds.right(), d.bounds.bottom());
                t.push_str(&format!(" | {},{} {}x{}{}", x0, y0, x1 - x0, y1 - y0, if d.primary { "*" } else { "" }));
            }
        }
        Ok(ActResult { ok: true, text: t, image: Some(jpeg), mime: "image/jpeg", image_path: Some(path.to_string_lossy().into_owned()) })
    }

    fn ui(&mut self, action: &str, a: &Args) -> R<ActResult> {
        if self.uia.is_none() { self.uia = Some(uia::Uia::new()?); }
        let u = self.uia.as_mut().unwrap();
        let action = if action.is_empty() { "inspect".to_string() } else { action.to_lowercase() };
        let id = s(a, "ref", "");
        let t = match action.as_str() {
            "inspect" => {
                let limit = int(a, "limit", 60).max(0) as usize;
                let root = u.root_for(a)?;
                let items = u.walk(&root, bool_(a, "all", false), limit, int(a, "depth", 25).max(0) as usize);
                u.format(items, limit, bool_(a, "json", false), "No controls found.").replace("+1 more (--limit)", "+more (--limit)")
            }
            "find" => {
                let q = s_any(a, &["query", "name", "text"], "");
                if q.is_empty() { return Err("`query` is required.".into()); }
                let limit = int(a, "limit", 25).max(0) as usize;
                let root = u.root_for(a)?;
                let items = u.collect(&root, Some(&q), bool_(a, "all", false))?;
                u.format(items, limit, bool_(a, "json", false), &format!("No control matching `{q}`."))
            }
            "invoke" | "click" => u.invoke(&id, a)?,
            "set" | "set_value" => u.set_value(&id, &s_any(a, &["value", "text"], ""), a)?,
            "read" => u.read(&id, a)?,
            "toggle" => u.toggle(&id, a)?,
            "focus" => u.focus(&id, a)?,
            _ => return Err(format!("Unknown ui action `{action}`. Use inspect, find, invoke, set, read, toggle, focus.")),
        };
        Ok(ActResult::say(t))
    }
}

fn coord(a: &Args) -> Option<(i64, i64)> {
    if let Some(c) = ints(a, "coordinate") { if c.len() == 2 { return Some((c[0], c[1])); } return None; }
    Some((util::try_int(a, "x")?, util::try_int(a, "y")?))
}
fn need(c: Option<(i64, i64)>) -> R<(i64, i64)> { c.ok_or_else(|| "`coordinate` [x,y] (or x and y) is required.".into()) }

fn mouse(action: &str, a: &Args) -> R<String> {
    let action = if action.is_empty() { "click".to_string() } else { action.to_lowercase().replace('-', "_") };
    let map = ScreenMap::build(config::get().max_dim);
    let c = coord(a);
    match action.as_str() {
        "position" | "cursor_position" => { let (x, y) = input::cursor_pos(); let (ix, iy) = map.to_image(x, y); Ok(format!("{ix},{iy}")) }
        "move" | "mouse_move" => { let (x, y) = need(c)?; let p = map.to_screen(x, y); input::move_to(p, 1.0)?; emit_pointer(p.0, p.1, "move", true); Ok("ok".into()) }
        "click" | "left_click" => { let (x, y) = need(c)?; input::click_at(&map, x, y, "left", 1)?; Ok("ok".into()) }
        "right_click" => { let (x, y) = need(c)?; input::click_at(&map, x, y, "right", 1)?; Ok("ok".into()) }
        "middle_click" => { let (x, y) = need(c)?; input::click_at(&map, x, y, "middle", 1)?; Ok("ok".into()) }
        "double_click" | "doubleclick" => { let (x, y) = need(c)?; input::click_at(&map, x, y, "left", 2)?; Ok("ok".into()) }
        "triple_click" => { let (x, y) = need(c)?; input::click_at(&map, x, y, "left", 3)?; Ok("ok".into()) }
        "drag" | "left_click_drag" => {
            let (x, y) = need(c)?;
            let to = ints(a, "to").filter(|t| t.len() == 2).ok_or("`to` [x,y] is required for drag.")?;
            input::drag(&map, x, y, to[0], to[1])?;
            Ok("ok".into())
        }
        "scroll" => {
            let dir = s_any(a, &["direction", "scroll_direction"], "down").to_lowercase();
            let amount = util::try_int(a, "amount").or_else(|| util::try_int(a, "scroll_amount")).unwrap_or(3);
            if let Some((x, y)) = c { input::move_to(map.to_screen(x, y), 1.0)?; }
            let (x, y) = input::cursor_pos();
            emit_pointer(x, y, "move", true);
            input::scroll(&dir, amount)?;
            Ok("ok".into())
        }
        "down" | "left_mouse_down" => {
            if let Some((x, y)) = c { input::move_to(map.to_screen(x, y), 1.0)?; }
            let (x, y) = input::cursor_pos();
            emit_pointer(x, y, "click", true);
            input::button(true)?;
            Ok("ok".into())
        }
        "up" | "left_mouse_up" => { input::button(false)?; Ok("ok".into()) }
        "wait" => {
            let sec = num(a, "duration", 0.5).clamp(0.0, 10.0);
            let t0 = Instant::now();
            while t0.elapsed().as_secs_f64() < sec {
                if cancelled() { return Err(STOPPED.into()); }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Ok("ok".into())
        }
        _ => Err(format!("Unknown mouse action `{action}`.")),
    }
}

fn keyboard(action: &str, a: &Args) -> R<String> {
    let action = if action.is_empty() { "type".to_string() } else { action.to_lowercase() };
    let text = s_any(a, &["text", "keys"], "");
    match action.as_str() {
        "type" => {
            if text.is_empty() { return Err("`text` is required.".into()); }
            emit_action("type");
            input::type_text(&text)?;
            Ok("ok".into())
        }
        "key" | "press" => {
            if text.is_empty() { return Err("`text` is a chord such as Return, ctrl+s, alt+Tab, or win.".into()); }
            let hold = int(a, "hold", 0).clamp(0, 10000) as u64;
            let chords = input::split_chords(&text);
            for (i, ch) in chords.iter().enumerate() {
                if i > 0 && !ctx().no_yield { yield_::wait_for_idle(&ctx().agent)?; }
                if cancelled() { return Err(STOPPED.into()); }
                if guard::presses(ch) { guard::check_focused()?; }
                emit_action(&format!("key:{}", input::chord_label(ch)));
                input::press_chord(ch, hold)?;
                if i + 1 < chords.len() { std::thread::sleep(std::time::Duration::from_millis(if input::motion_mode() == "smooth" { 120 } else { 30 })); }
            }
            Ok("ok".into())
        }
        "hold" | "hold_key" => {
            if text.is_empty() { return Err("`text` is required.".into()); }
            let sec = num(a, "duration", 1.0).clamp(0.05, 10.0);
            emit_action(&format!("key:{}", input::chord_label(&text)));
            input::press_chord(&text, (sec * 1000.0) as u64)?;
            Ok("ok".into())
        }
        _ => Err(format!("Unknown keyboard action `{action}`.")),
    }
}

fn apps_cmd(action: &str, a: &Args) -> R<String> {
    use windows::Win32::UI::WindowsAndMessaging::{SW_RESTORE, SW_SHOWMAXIMIZED, SW_SHOWMINIMIZED};
    let action = if action.is_empty() { "list".to_string() } else { action.to_lowercase() };
    let target = || apps::resolve(&s_any(a, &["window", "hwnd", "name", "app", "text"], ""));
    match action.as_str() {
        "list" => Ok(list_windows(&s_any(a, &["filter", "query"], ""), bool_(a, "json", false))),
        "open" | "launch" | "start" => {
            let name = s_any(a, &["name", "app", "text", "query"], "");
            if name.is_empty() { return Err("`name` is required (for example notepad, chrome).".into()); }
            apps::open(&name, bool_(a, "new", false))
        }
        "focus" => {
            let h = target()?;
            if let Some(w) = apps::enumerate().into_iter().find(|w| w.h == h && w.elevated) {
                return Err(admin_msg(&w.process));
            }
            apps::force_focus(apps::hwnd(h));
            Ok(format!("focused {} {}", apps::fmt_hwnd(h), apps::title_of(apps::hwnd(h))))
        }
        "close" => { let h = target()?; apps::close(apps::hwnd(h)); Ok(format!("close sent to {}", apps::fmt_hwnd(h))) }
        "minimize" => { apps::show(apps::hwnd(target()?), SW_SHOWMINIMIZED); Ok("ok".into()) }
        "maximize" => { apps::show(apps::hwnd(target()?), SW_SHOWMAXIMIZED); Ok("ok".into()) }
        "restore" => { apps::show(apps::hwnd(target()?), SW_RESTORE); Ok("ok".into()) }
        "displays" => Ok(list_displays()),
        _ => Err(format!("Unknown apps action `{action}`. Use list, open, focus, close, minimize, maximize, restore, displays.")),
    }
}

/// App copy from app/prompts/agent_system.md ("Admin window").
pub fn admin_msg(process: &str) -> String {
    let _ = process;
    "That app is running as administrator, so I can't control it. Close it and reopen it normally, or run me as administrator.".into()
}

fn list_displays() -> String {
    let map = ScreenMap::build(config::get().max_dim);
    let mut s = String::new();
    for (i, d) in dpi::displays().iter().enumerate() {
        let (x0, y0) = map.to_image(d.bounds.x, d.bounds.y);
        let (x1, y1) = map.to_image(d.bounds.right(), d.bounds.bottom());
        let (wx0, wy0) = map.to_image(d.work.x, d.work.y);
        let (wx1, wy1) = map.to_image(d.work.right(), d.work.bottom());
        s.push_str(&format!("d{} {},{} {}x{} work {},{} {}x{}{}\n", i + 1, x0, y0, x1 - x0, y1 - y0, wx0, wy0, wx1 - wx0, wy1 - wy0, if d.primary { " primary" } else { "" }));
    }
    s.push_str(&format!("image {}x{}", map.out_w, map.out_h));
    s
}

fn list_windows(filter: &str, json: bool) -> String {
    let map = ScreenMap::build(config::get().max_dim);
    let f = filter.to_lowercase();
    let mut s = String::new();
    let mut list = Vec::new();
    let fence = apps::fence_on();
    for w in apps::enumerate() {
        if fence && !apps::is_fixture(&w) { continue; } // test fence: the tester's own windows stay unlisted
        if !f.is_empty() && !w.title.to_lowercase().contains(&f) && !w.process.to_lowercase().contains(&f) { continue; }
        let (x0, y0) = map.to_image(w.bounds.x, w.bounds.y);
        let (x1, y1) = map.to_image(w.bounds.right(), w.bounds.bottom());
        let (ww, hh) = ((x1 - x0).max(0), (y1 - y0).max(0));
        if json {
            let mut d = serde_json::json!({ "hwnd": apps::fmt_hwnd(w.h), "title": w.title, "process": w.process, "pid": w.pid,
                "x": x0, "y": y0, "w": ww, "h": hh, "focused": w.focused, "minimized": w.minimized, "maximized": w.maximized });
            if w.elevated { d["admin"] = Value::Bool(true); }
            list.push(d);
            continue;
        }
        s.push_str(&apps::fmt_hwnd(w.h));
        s.push(' ');
        s.push_str(&if w.title.is_empty() { "-".to_string() } else { util::clip(&w.title, 70) });
        if !w.process.eq_ignore_ascii_case(&w.title) { s.push_str(&format!(" [{}]", w.process)); }
        s.push_str(&format!(" {x0},{y0} {ww}x{hh}"));
        if w.focused { s.push_str(" focused"); }
        if w.minimized { s.push_str(" min"); }
        if w.maximized { s.push_str(" max"); }
        if w.elevated { s.push_str(" admin"); }
        s.push('\n');
    }
    if json { return Value::Array(list).to_string(); }
    if s.is_empty() { return if f.is_empty() { "No open windows.".into() } else { format!("No window matched `{filter}`.") }; }
    s.trim_end().to_string()
}

/// `reach selftest`: offscreen checks of the input logic (nothing is sent to the OS).
pub fn selftest() -> String {
    let mut out = String::new();
    let (mut pass, mut fail) = (0, 0);
    let mut ok = |name: &str, c: bool| if c { pass += 1 } else { fail += 1; out.push_str(&format!("FAIL {name}\n")); };
    ok("seq comma", input::split_chords("ctrl+c, ctrl+v").join("|") == "ctrl+c|ctrl+v");
    ok("spaced chord", input::split_chords("ctrl + shift + t").join("|") == "ctrl+shift+t");
    ok("space seq", input::split_chords("ctrl+a Delete").join("|") == "ctrl+a|Delete");
    ok("ctrl+shift+t", input::parse_chord("ctrl+shift+t").map(|c| c.mods == [0x11, 0x10] && c.key == 0x54).unwrap_or(false));
    ok("win+r", input::parse_chord("win+r").map(|c| c.mods == [0x5B] && c.key == 0x52).unwrap_or(false));
    ok("win+l blocked", input::parse_chord("win+l").is_err());
    ok("stop hotkey blocked", input::parse_chord("ctrl+alt+esc").is_err());
    ok("label", input::chord_label("control+Return") == "Ctrl + Enter");
    let mut land = true;
    for i in 0..500 {
        let a = ((i * 37 % 3840) as f64 - 1920.0, (i * 11 % 1080) as f64);
        let b = ((i * 53 % 3840) as f64 - 1920.0, (i * 29 % 1080) as f64);
        land &= input::Path::human(a, b).at(1.0) == b && input::Path::fast(a, b).at(1.0) == b;
    }
    ok("glides land exactly", land);
    let m = ScreenMap::for_rect(Rect { x: -1920, y: 0, w: 3840, h: 1080 }, 1280);
    ok("map negative monitor", m.to_screen(640, 0) == (0, 0) && m.to_image(-1920, 1080) == (0, 360));
    let p = reg::UserPath { key: r"Software\ReachSelfTestRs2\Environment".into() };
    let _ = reg::write(reg::HKCU, &p.key, "Path", r"C:\A;%SystemRoot%\x;", true);
    ok("path add", p.add(r"D:\New Dir") && !p.add(r"d:\new dir\"));
    ok("path keeps vars", p.read() == (r"C:\A;%SystemRoot%\x;D:\New Dir".into(), true));
    ok("path remove", p.remove(r"D:\New Dir") && p.read().0 == r"C:\A;%SystemRoot%\x");
    reg::delete_tree(reg::HKCU, r"Software\ReachSelfTestRs2");
    out.push_str(&format!("{pass} passed, {fail} failed"));
    out
}

/// Serializes callers that must not interleave (the app's agent loop).
pub static RUN_LOCK: Mutex<()> = Mutex::new(());
