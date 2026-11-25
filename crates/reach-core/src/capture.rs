//! Screen capture: GDI StretchBlt (HALFTONE) straight into a top-down 32-bit DIB at
//! the output size, then JPEG. Physical pixels (the process is per-monitor-v2 aware).

use crate::dpi::Rect;
use windows::Win32::Graphics::Gdi::*;

/// Returns BGRA pixels of `src` (screen px) scaled to out_w x out_h.
pub fn grab(src: Rect, out_w: i32, out_h: i32) -> Result<Vec<u8>, String> {
    unsafe {
        let screen = GetDC(None);
        if screen.is_invalid() {
            return Err("GetDC failed".into());
        }
        let r = blit(screen, src, out_w, out_h);
        ReleaseDC(None, screen);
        r
    }
}

#[link(name = "user32")]
unsafe extern "system" {
    fn PrintWindow(hwnd: isize, hdc: isize, flags: u32) -> i32;
}

/// BGRA pixels of one window's own content (`frame` = its visible bounds, screen px), scaled to
/// out_w x out_h. Drawn by the window itself (PrintWindow, full content), so nothing on top of it
/// can end up in the picture: the test fence uses this so other windows are never captured.
pub fn grab_window(hwnd: isize, frame: Rect, out_w: i32, out_h: i32) -> Result<Vec<u8>, String> {
    use windows::Win32::{Foundation::{HWND, RECT}, UI::WindowsAndMessaging::GetWindowRect};
    unsafe {
        let mut wr = RECT::default();
        GetWindowRect(HWND(hwnd as _), &mut wr).map_err(|e| e.to_string())?;
        let (ww, wh) = (wr.right - wr.left, wr.bottom - wr.top);
        if ww < 1 || wh < 1 { return Err("Empty window.".into()); }
        let screen = GetDC(None);
        let dc = CreateCompatibleDC(Some(screen));
        let bmp = CreateCompatibleBitmap(screen, ww, wh);
        let old = SelectObject(dc, bmp.into());
        const PW_RENDERFULLCONTENT: u32 = 2;
        let r = if PrintWindow(hwnd, dc.0 as isize, PW_RENDERFULLCONTENT) != 0 {
            // the window bitmap starts at the window rect; the visible frame sits inside it (shadow borders)
            blit(dc, Rect { x: frame.x - wr.left, y: frame.y - wr.top, w: frame.w, h: frame.h }, out_w, out_h)
        } else { Err("PrintWindow failed".into()) };
        SelectObject(dc, old);
        let _ = DeleteObject(bmp.into());
        let _ = DeleteDC(dc);
        ReleaseDC(None, screen);
        r
    }
}

/// Copies `src` (in `from`'s coordinates) into a top-down 32-bit DIB of out_w x out_h.
unsafe fn blit(from: HDC, src: Rect, out_w: i32, out_h: i32) -> Result<Vec<u8>, String> {
    unsafe {
        let screen = from;
        let mem = CreateCompatibleDC(Some(screen));
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: out_w,
                biHeight: -out_h, // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let dib = CreateDIBSection(Some(mem), &bmi, DIB_RGB_COLORS, &mut bits, None, 0);
        let result = match dib {
            Ok(dib) => {
                let old = SelectObject(mem, dib.into());
                let ok = if out_w == src.w && out_h == src.h {
                    BitBlt(mem, 0, 0, out_w, out_h, Some(screen), src.x, src.y, SRCCOPY).is_ok()
                } else {
                    SetStretchBltMode(mem, HALFTONE);
                    let _ = SetBrushOrgEx(mem, 0, 0, None);
                    StretchBlt(mem, 0, 0, out_w, out_h, Some(screen), src.x, src.y, src.w, src.h, SRCCOPY).as_bool()
                };
                let _ = GdiFlush();
                let out = if ok && !bits.is_null() {
                    Ok(std::slice::from_raw_parts(bits as *const u8, (out_w * out_h * 4) as usize).to_vec())
                } else {
                    Err("screen capture failed".to_string())
                };
                SelectObject(mem, old);
                let _ = DeleteObject(dib.into());
                out
            }
            Err(e) => Err(format!("CreateDIBSection failed: {e}")),
        };
        let _ = DeleteDC(mem);
        result
    }
}

pub fn jpeg(bgra: &[u8], w: i32, h: i32, quality: u8) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let enc = jpeg_encoder::Encoder::new(&mut out, quality);
    enc.encode(bgra, w as u16, h as u16, jpeg_encoder::ColorType::Bgra).map_err(|e| e.to_string())?;
    Ok(out)
}

pub fn hash(data: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    data.hash(&mut h);
    h.finish()
}
