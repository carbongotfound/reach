//! reach.exe: CLI, loopback HTTP + MCP server, and tray icon. Same commands and
//! output as the 0.2 C# build, on top of reach-core.

mod cli;
mod tray;

use reach_core::{config, dpi, reg::UserPath, util::wide};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
use windows::Win32::System::Console::{FreeConsole, GetConsoleProcessList, GetConsoleWindow};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_ESCAPE, VK_MENU};
use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};

fn exe_dir() -> String {
    std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_string_lossy().into_owned())).unwrap_or_default()
}

fn main() {
    dpi::make_aware();
    let _ = std::fs::create_dir_all(config::home());
    config::load();
    let mut no_path = false;
    let args: Vec<String> = std::env::args().skip(1).filter(|a| if a == "--no-path" { no_path = true; false } else { true }).collect();
    let c0 = args.first().map(|s| s.to_lowercase()).unwrap_or_default();
    if c0 == "path" { std::process::exit(path_cmd(&args)); }
    if !no_path && !config::isolated() && c0 != "selftest" && config::get().auto_path {
        let d = exe_dir();
        if UserPath::real().add(&d) { println!("Added {d} to PATH, open a new terminal"); }
    }
    if c0 == "selftest" {
        let r = reach_core::selftest();
        println!("{r}");
        std::process::exit(if r.contains("FAIL") { 1 } else { 0 });
    }
    if args.is_empty() {
        // Startup shortcut and double-click: run the tray server, or print status if one is up.
        if let Some(t) = cli::health() { println!("{t}"); return; }
        std::process::exit(serve());
    }
    if matches!(c0.as_str(), "serve" | "start" | "mcp") { std::process::exit(serve()); }
    match cli::run(args) {
        Ok(code) => std::process::exit(code),
        Err(e) => { eprintln!("{e}"); config::log(&format!("cli: {e}")); std::process::exit(1); }
    }
}

fn path_cmd(args: &[String]) -> i32 {
    let a = args.get(1).map(|s| s.to_lowercase()).unwrap_or_else(|| "status".into());
    let d = exe_dir();
    let p = UserPath::real();
    match a.as_str() {
        "add" => { config::update(|c| c.auto_path = true); println!("{}", if p.add(&d) { format!("Added {d} to PATH, open a new terminal") } else { format!("{d} is already on PATH") }); }
        "remove" => { config::update(|c| c.auto_path = false); println!("{}", if p.remove(&d) { format!("Removed {d} from PATH (auto-add off)") } else { format!("{d} wasn't on PATH (auto-add off)") }); }
        _ => println!("{d} {} your PATH, auto-add {}", if p.contains(&d) { "is on" } else { "is not on" }, if config::get().auto_path { "on" } else { "off" }),
    }
    0
}

fn mutex_name() -> String {
    let mut n = "Local\\ReachDesktopControl.1".to_string();
    if config::isolated() {
        let h = config::home().to_string_lossy().to_lowercase().bytes().fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619));
        n.push_str(&format!(".rs{h}"));
    }
    n
}

/// Ctrl+Alt+Esc (physical keys only): stop the action in progress and pause Reach.
fn stop_hotkey(vk: u32, down: bool) -> bool {
    if !down || vk != VK_ESCAPE.0 as u32 { return false; }
    let held = |k: i32| unsafe { GetAsyncKeyState(k) } < 0;
    if !(held(VK_CONTROL.0 as i32) && held(VK_MENU.0 as i32)) { return false; }
    reach_core::stop();
    config::update(|c| c.paused = true);
    config::log("stop hotkey: stopped and paused");
    tray::refresh();
    true
}

fn serve() -> i32 {
    let name = wide(&mutex_name());
    let _m = unsafe { CreateMutexW(None, true, PCWSTR(name.as_ptr())) };
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        println!("Reach is already running.");
        return 0;
    }
    unsafe {
        // Only hide a console that belongs to us alone, never the user's terminal.
        let mut list = [0u32; 4];
        if GetConsoleProcessList(&mut list) == 1 {
            let cw = GetConsoleWindow();
            if !cw.0.is_null() { let _ = ShowWindow(cw, SW_HIDE); }
        }
        let _ = FreeConsole();
    }
    reach_core::overlay::start();
    let _ = reach_core::yield_::KEY_LISTENER.set(stop_hotkey);
    reach_core::yield_::start();
    let engine = reach_core::Engine::start();
    // Configured port, or port+1 (saved) when taken, as the C# build does.
    let port = config::get().port;
    let l = std::net::TcpListener::bind(("127.0.0.1", port)).or_else(|_| {
        config::update(|c| c.port = port + 1);
        std::net::TcpListener::bind(("127.0.0.1", port + 1))
    });
    if let Err(e) = l.map_err(|e| e.to_string()).and_then(|l| reach_core::server::start(engine, l, Default::default())) {
        config::log(&format!("server: {e}"));
        return 1;
    }
    tray::run();
    0
}
