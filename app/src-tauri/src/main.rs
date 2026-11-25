//! Reach app: Tauri window + reach-core engine and MCP server in-process. The brain
//! (claude CLI) gets this process's server on a random loopback port with a fresh token.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod brain;
mod desk;
mod prompts;
mod settings;
mod strings;
mod update;

use reach_core::{config, server, Engine};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use tauri::Emitter;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_ESCAPE, VK_MENU};

pub static APP: OnceLock<tauri::AppHandle> = OnceLock::new();
/// (port, token) of the in-process MCP server.
pub static MCP: OnceLock<(u16, String)> = OnceLock::new();

/// Ctrl+Alt+Esc from the physical keyboard, wherever focus is: stop the run.
fn hotkey(vk: u32, down: bool) -> bool {
    if !down || vk != VK_ESCAPE.0 as u32 { return false; }
    let held = |k: i32| unsafe { GetAsyncKeyState(k) } < 0;
    if !(held(VK_CONTROL.0 as i32) && held(VK_MENU.0 as i32)) { return false; }
    reach_core::stop(); // instant; the rest runs off the hook thread
    std::thread::spawn(|| {
        brain::stop("hotkey");
        if let Some(a) = APP.get() { let _ = a.emit("hotkey-stop", ()); }
    });
    true
}

fn main() {
    reach_core::dpi::make_aware();
    // The app keeps its own Reach home, so it never fights a CLI server over ~/.reach.
    if std::env::var("REACH_HOME").map(|h| h.is_empty()).unwrap_or(true) {
        let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into());
        std::env::set_var("REACH_HOME", PathBuf::from(base).join("Reach"));
    }
    let fresh = !config::path("config.json").exists();
    config::load();
    if fresh { config::update(|c| { c.motion = "fast".into(); c.auto_path = false; }); }
    config::update(|c| c.paused = false);

    let _ = reach_core::yield_::KEY_LISTENER.set(hotkey);
    if std::env::var("REACH_TEST_HOTKEY").as_deref() == Ok("1") { reach_core::yield_::LISTEN_INJECTED.store(true, std::sync::atomic::Ordering::Relaxed); }
    reach_core::yield_::start();
    let _ = reach_core::overlay::ANSWER.set(brain::answer_from_pill);
    reach_core::overlay::start();
    let engine = Engine::start();
    reach_core::events::subscribe(brain::on_event);
    let _ = reach_core::guard::HOOK.set(brain::guard);
    let token = reach_core::util::random_hex(32);
    let port = std::net::TcpListener::bind(("127.0.0.1", 0)).map_err(|e| e.to_string())
        .and_then(|l| server::start(engine, l, server::Opts { token: Some(token.clone()), hook: Some(Arc::new(brain::gate)), extra_tools: vec![brain::approval_tool()], after: Some(Arc::new(brain::flag_injection)) }))
        .unwrap_or_else(|e| { config::log(&format!("app server: {e}")); 0 });
    let _ = brain::write_mcp_config(port, &token);
    let _ = MCP.set((port, token));
    config::log(&format!("app started, MCP on 127.0.0.1:{port}"));

    tauri::Builder::default()
        .setup(|app| {
            use tauri::Manager;
            let _ = APP.set(app.handle().clone());
            if let Some(w) = app.get_webview_window("main") {
                settings::apply_theme(&w);
                // title bar and taskbar icon follow Windows' taskbar theme (#111 on light, white on dark)
                if let Ok(h) = w.hwnd() { let h = h.0 as isize; reach_core::brand::watch(move |light| reach_core::brand::set_window_icons(h, light)); }
                // tests only: open at "x,y" (physical px), so a test window never shows up on the user's screen first
                if let Some((x, y)) = std::env::var("REACH_WINDOW_POS").ok().and_then(|v| { let (x, y) = v.split_once(',')?; Some((x.trim().parse::<i32>().ok()?, y.trim().parse::<i32>().ok()?)) }) {
                    let _ = w.set_position(tauri::PhysicalPosition::new(x, y));
                }
                let _ = w.show();
            }
            update::start();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            brain::check_brain, brain::start_task, brain::send_message, brain::stop_task, brain::take_over, brain::resume, brain::get_speed, brain::set_speed, brain::list_runs, brain::load_run, brain::rename_run, brain::delete_run, brain::open_run_folder, brain::answer_approval, brain::get_brain, brain::set_brain,
            desk::app_icon, desk::list_screens, desk::screen_thumb,
            settings::get_settings, settings::set_setting, settings::open_home, settings::set_titlebar,
            update::update_state, update::update_check, update::update_install, update::update_cancel, update::update_run, update::update_skip, update::update_later, update::update_open
        ])
        .run(tauri::generate_context!())
        .expect("error while running Reach");
}
