//! Per-monitor-v2 DPI awareness and the image <-> screen coordinate map.
//! Image pixels are the virtual desktop (all monitors, physical pixels) scaled so
//! the long edge is at most max_dim. Agents only ever see image pixels.

use windows::Win32::Foundation::{LPARAM, POINT, RECT};
use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN};
use windows::core::BOOL;

/// Call once, first thing in main. Safe to call twice.
pub fn make_aware() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}
impl Rect {
    pub fn right(&self) -> i32 { self.x + self.w }
    pub fn bottom(&self) -> i32 { self.y + self.h }
    pub fn from_ltrb(l: i32, t: i32, r: i32, b: i32) -> Rect { Rect { x: l, y: t, w: r - l, h: b - t } }
    fn from_win(r: RECT) -> Rect { Rect::from_ltrb(r.left, r.top, r.right, r.bottom) }
    pub fn intersect(&self, o: &Rect) -> Rect {
        let l = self.x.max(o.x);
        let t = self.y.max(o.y);
        let r = self.right().min(o.right());
        let b = self.bottom().min(o.bottom());
        if r <= l || b <= t { Rect { x: 0, y: 0, w: 0, h: 0 } } else { Rect::from_ltrb(l, t, r, b) }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ScreenMap {
    pub virt: Rect,
    pub scale: f64,
    pub out_w: i32,
    pub out_h: i32,
}

/// C# Math.Round is banker's rounding; match it so both builds give the same pixels.
pub fn round(v: f64) -> i32 {
    let r = v.round();
    if (v - v.trunc()).abs() == 0.5 && (r as i64) % 2 != 0 { (r - v.signum()) as i32 } else { r as i32 }
}

impl ScreenMap {
    pub fn virtual_screen() -> Rect {
        unsafe {
            Rect {
                x: GetSystemMetrics(SM_XVIRTUALSCREEN),
                y: GetSystemMetrics(SM_YVIRTUALSCREEN),
                w: GetSystemMetrics(SM_CXVIRTUALSCREEN),
                h: GetSystemMetrics(SM_CYVIRTUALSCREEN),
            }
        }
    }

    pub fn build(max_dim: i32) -> ScreenMap {
        Self::for_rect(Self::virtual_screen(), max_dim)
    }

    pub fn for_rect(vs: Rect, max_dim: i32) -> ScreenMap {
        let long = vs.w.max(vs.h);
        let scale = if long > max_dim { max_dim as f64 / long as f64 } else { 1.0 };
        ScreenMap {
            virt: vs,
            scale,
            out_w: 1.max(round(vs.w as f64 * scale)),
            out_h: 1.max(round(vs.h as f64 * scale)),
        }
    }

    pub fn to_screen(&self, x: i64, y: i64) -> (i32, i32) {
        (self.virt.x + round(x as f64 / self.scale), self.virt.y + round(y as f64 / self.scale))
    }

    pub fn to_image(&self, sx: i32, sy: i32) -> (i32, i32) {
        (round((sx - self.virt.x) as f64 * self.scale), round((sy - self.virt.y) as f64 * self.scale))
    }
}

#[derive(Clone, Debug)]
pub struct Display {
    pub bounds: Rect,
    pub work: Rect,
    pub primary: bool,
    pub dpi: u32,
}

pub fn displays() -> Vec<Display> {
    unsafe extern "system" fn cb(m: HMONITOR, _: HDC, _: *mut RECT, l: LPARAM) -> BOOL {
        let list = &mut *(l.0 as *mut Vec<Display>);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if GetMonitorInfoW(m, &mut mi).as_bool() {
            let (mut dx, mut dy) = (96u32, 96u32);
            let _ = GetDpiForMonitor(m, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
            list.push(Display { bounds: Rect::from_win(mi.rcMonitor), work: Rect::from_win(mi.rcWork), primary: mi.dwFlags & 1 != 0, dpi: dx });
        }
        BOOL(1)
    }
    let mut list: Vec<Display> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(cb), LPARAM(&mut list as *mut _ as isize));
    }
    list
}

pub fn dpi_scale_at(x: i32, y: i32) -> f32 {
    use windows::Win32::Graphics::Gdi::{MonitorFromPoint, MONITOR_DEFAULTTONEAREST};
    unsafe {
        let m = MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST);
        let (mut dx, mut dy) = (96u32, 96u32);
        if GetDpiForMonitor(m, MDT_EFFECTIVE_DPI, &mut dx, &mut dy).is_ok() && dx > 0 { dx as f32 / 96.0 } else { 1.0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn map_roundtrip_two_monitors_left_negative() {
        // two 1920x1080, one at x=-1920 (this PC), max 1280 -> 1280x360
        let m = ScreenMap::for_rect(Rect { x: -1920, y: 0, w: 3840, h: 1080 }, 1280);
        assert_eq!((m.out_w, m.out_h), (1280, 360));
        assert_eq!(m.to_screen(640, 0), (0, 0));
        assert_eq!(m.to_image(0, 0), (640, 0));
        assert_eq!(m.to_image(-1920, 1080), (0, 360));
        // 150%: a 2560x1440 panel is still physical pixels under PMv2; mapping is linear
        let m = ScreenMap::for_rect(Rect { x: 0, y: 0, w: 2560, h: 1440 }, 1280);
        assert_eq!(m.to_screen(1279, 719), (2558, 1438));
    }
    #[test]
    fn bankers_round_matches_csharp() {
        assert_eq!(round(2.5), 2);
        assert_eq!(round(3.5), 4);
        assert_eq!(round(-2.5), -2);
        assert_eq!(round(1.4), 1);
    }
}
