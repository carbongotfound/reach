//! Reach's icon for the tray and the taskbar, following Windows' taskbar theme: the #111 mark on a light
//! taskbar, the white one on a dark taskbar. 16 px is the hand only (no dot). PNGs from assets/brand/logo.py.

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
use windows::Win32::UI::WindowsAndMessaging::*;

const SIZES: [i32; 5] = [16, 20, 24, 32, 48];
static LIGHT: [&[u8]; 5] = [
    include_bytes!("../../../assets/brand/png/tray-light-16.png"), include_bytes!("../../../assets/brand/png/tray-light-20.png"),
    include_bytes!("../../../assets/brand/png/tray-light-24.png"), include_bytes!("../../../assets/brand/png/tray-light-32.png"),
    include_bytes!("../../../assets/brand/png/tray-light-48.png")];
static DARK: [&[u8]; 5] = [
    include_bytes!("../../../assets/brand/png/tray-dark-16.png"), include_bytes!("../../../assets/brand/png/tray-dark-20.png"),
    include_bytes!("../../../assets/brand/png/tray-dark-24.png"), include_bytes!("../../../assets/brand/png/tray-dark-32.png"),
    include_bytes!("../../../assets/brand/png/tray-dark-48.png")];

/// The icon at `size` px for a light (true) or dark taskbar: the nearest drawn size at or above, scaled.
pub fn icon(light: bool, size: i32) -> HICON {
    let i = SIZES.iter().position(|&s| s >= size).unwrap_or(SIZES.len() - 1);
    let png = if light { LIGHT[i] } else { DARK[i] };
    unsafe { CreateIconFromResourceEx(png, true, 0x0003_0000, size, size, LR_DEFAULTCOLOR).unwrap_or_default() }
}

/// Small (title bar) and big (taskbar, Alt+Tab) icons of a window, at its DPI.
pub fn set_window_icons(hwnd: isize, light: bool) {
    let h = HWND(hwnd as *mut _);
    unsafe {
        let dpi = GetDpiForWindow(h).max(96);
        for (kind, metric) in [(ICON_SMALL, SM_CXSMICON), (ICON_BIG, SM_CXICON)] {
            let ic = icon(light, GetSystemMetricsForDpi(metric, dpi));
            // the replaced icon isn't destroyed: the first one belongs to the window framework (one leak per theme change)
            SendMessageW(h, WM_SETICON, Some(WPARAM(kind as usize)), Some(LPARAM(ic.0 as isize)));
        }
    }
}

/// Calls `f(light)` now and whenever the taskbar theme changes.
// ponytail: polls the registry every 2 s; a WM_SETTINGCHANGE listener if that ever shows up in a profile
pub fn watch(f: impl Fn(bool) + Send + 'static) {
    std::thread::Builder::new().name("reach-taskbar-theme".into()).spawn(move || {
        let mut last = None;
        loop {
            let light = crate::reg::taskbar_light();
            if last != Some(light) { last = Some(light); f(light); }
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
    }).ok();
}

#[cfg(test)]
mod tests {
    #[test]
    fn theme_icons_load_at_every_size() {
        for light in [true, false] { for s in [16, 20, 24, 32, 40, 48, 64] { assert!(!super::icon(light, s).is_invalid(), "{light} {s}"); } }
    }
}
