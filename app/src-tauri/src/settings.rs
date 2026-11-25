//! Settings screen commands, and the native title bar coloured to match the app theme.
use crate::strings;
use reach_core::config;
use serde_json::{json, Value};

#[tauri::command]
pub fn get_settings() -> Value {
    let c = config::get();
    json!({
        "speed": c.motion, "pauseOnUser": c.pause_on_user, "pauseIdleMs": c.user_pause_idle_ms,
        "agentCursor": c.agent_cursor, "cursorRipple": c.cursor_ripple, "maxDim": c.max_dim, "theme": c.theme,
        "updateAuto": c.update_auto, "updateBeta": c.update_beta,
        "home": config::path("").display().to_string(), "version": env!("CARGO_PKG_VERSION"),
    })
}

/// One setting from the Settings screen. Only these keys can be changed from the UI.
#[tauri::command]
pub fn set_setting(window: tauri::WebviewWindow, key: String, value: Value) -> Result<(), String> {
    let bad = || strings::VALUE_NOT_ALLOWED.to_string();
    match key.as_str() {
        "pauseOnUser" => { let v = value.as_bool().ok_or_else(bad)?; config::update(|c| c.pause_on_user = v) }
        "agentCursor" => { let v = value.as_bool().ok_or_else(bad)?; config::update(|c| c.agent_cursor = v) }
        "cursorRipple" => { let v = value.as_bool().ok_or_else(bad)?; config::update(|c| c.cursor_ripple = v) }
        "updateAuto" => { let v = value.as_bool().ok_or_else(bad)?; config::update(|c| c.update_auto = v) }
        "updateBeta" => {
            let v = value.as_bool().ok_or_else(bad)?;
            config::update(|c| c.update_beta = v);
            std::thread::spawn(|| crate::update::check(false, false));
        }
        "pauseIdleMs" => {
            let v = value.as_i64().filter(|v| (500..=10_000).contains(v)).ok_or_else(bad)?;
            config::update(|c| c.user_pause_idle_ms = v)
        }
        "maxDim" => {
            let v = value.as_i64().filter(|v| [1024, 1280, 1600, 1920].contains(v)).ok_or_else(bad)? as i32;
            config::update(|c| c.max_dim = v)
        }
        "theme" => {
            let v = value.as_str().filter(|v| ["system", "light", "dark"].contains(v)).ok_or_else(bad)?.to_string();
            config::update(|c| c.theme = v);
            apply_theme(&window);
        }
        _ => return Err(strings::UNKNOWN_SETTING.into()),
    }
    Ok(())
}

/// Light or dark for this window only, from the Theme setting ("system" follows Windows).
/// The page follows through prefers-color-scheme, and the title bar through set_titlebar.
pub fn apply_theme(window: &tauri::WebviewWindow) {
    let t = match config::get().theme.as_str() { "light" => Some(tauri::Theme::Light), "dark" => Some(tauri::Theme::Dark), _ => None };
    let _ = window.set_theme(t);
}

/// Opens the folder with run history and settings in Explorer.
#[tauri::command]
pub fn open_home() -> Result<(), String> {
    std::process::Command::new("explorer.exe").arg(config::path("")).spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// Paints the Windows 11 title bar in the app's own background colour (light or dark), so the
/// native frame keeps snap layouts and still matches the page. `bg` and `fg` are "#rrggbb".
#[tauri::command]
pub fn set_titlebar(window: tauri::WebviewWindow, bg: String, fg: String) -> Result<(), String> {
    use windows::Win32::Foundation::{COLORREF, HWND};
    use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE};
    let rgb = |h: &str| -> Result<COLORREF, String> {
        let v = u32::from_str_radix(h.trim_start_matches('#'), 16).map_err(|_| strings::BAD_COLOUR.to_string())?;
        Ok(COLORREF(((v & 0xff) << 16) | (v & 0xff00) | ((v >> 16) & 0xff)))
    };
    let (b, f) = (rgb(&bg)?, rgb(&fg)?);
    let dark: i32 = if (b.0 & 0xff) + ((b.0 >> 8) & 0xff) + ((b.0 >> 16) & 0xff) < 384 { 1 } else { 0 };
    let hwnd = HWND(window.hwnd().map_err(|e| e.to_string())?.0 as _);
    unsafe {
        let sz = std::mem::size_of::<u32>() as u32;
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &dark as *const _ as _, sz);
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_CAPTION_COLOR, &b as *const _ as _, sz);
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR, &b as *const _ as _, sz);
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_TEXT_COLOR, &f as *const _ as _, sz);
    }
    Ok(())
}
