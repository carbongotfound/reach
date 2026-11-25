//! What the window shows about the desktop: the app behind a window (name and icon, from the
//! Windows shell, Store apps included), the monitors, and small live thumbnails of them.
//! Icons are cached in memory and in <home>/icons; nothing third-party is bundled.

use reach_core::{apps, config, dpi, util};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Mutex;
use windows::core::{HSTRING, PWSTR};
use windows::Win32::Foundation::{CloseHandle, SIZE};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Com::{CoInitializeEx, CoTaskMemFree, COINIT_MULTITHREADED};
use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

/// The app behind a window: `key` finds its icon (a Store app id or an exe path), `name` is what people call it.
#[derive(Clone)]
pub struct App { pub key: String, pub name: String }

static APPS: Mutex<Option<HashMap<isize, App>>> = Mutex::new(None);
static ICONS: Mutex<Option<HashMap<String, Option<String>>>> = Mutex::new(None);

pub fn app_of(h: isize) -> App {
    if let Some(a) = APPS.lock().unwrap().as_ref().and_then(|m| m.get(&h)) { return a.clone(); }
    let a = lookup(h);
    APPS.lock().unwrap().get_or_insert_with(HashMap::new).insert(h, a.clone());
    a
}

fn lookup(h: isize) -> App {
    unsafe { let _ = CoInitializeEx(None, COINIT_MULTITHREADED); }
    // Store apps (Calculator, the new Notepad): the window's or the process's app id
    let aumid = window_aumid(h).or_else(|| process_path(h).1);
    if let Some(id) = aumid {
        let key = format!("shell:AppsFolder\\{id}");
        if let Some(name) = display_name(&key) { return App { key, name }; }
    }
    let (path, _) = process_path(h);
    let path = path.unwrap_or_default();
    let stem = std::path::Path::new(&path).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let name = file_description(&path).unwrap_or_else(|| { let mut c = stem.chars(); c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default() });
    App { key: path, name }
}

fn window_aumid(h: isize) -> Option<String> {
    use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
    use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow};
    use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
    unsafe {
        let store: IPropertyStore = SHGetPropertyStoreForWindow(apps::hwnd(h)).ok()?;
        let v = store.GetValue(&PKEY_AppUserModel_ID).ok()?;
        let p = PropVariantToStringAlloc(&v).ok()?;
        let s = p.to_string().unwrap_or_default();
        CoTaskMemFree(Some(p.0 as _));
        (!s.is_empty()).then_some(s)
    }
}

/// (exe path, packaged app id) of the window's process.
fn process_path(h: isize) -> (Option<String>, Option<String>) {
    use windows::Win32::Storage::Packaging::Appx::GetApplicationUserModelId;
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(apps::hwnd(h), Some(&mut pid));
        let Ok(p) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return (None, None) };
        let mut buf = [0u16; 1024];
        let mut n = buf.len() as u32;
        let path = QueryFullProcessImageNameW(p, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut n).is_ok().then(|| String::from_utf16_lossy(&buf[..n as usize]));
        let mut id = [0u16; 512];
        let mut len = id.len() as u32;
        let aumid = (GetApplicationUserModelId(p, &mut len, Some(PWSTR(id.as_mut_ptr()))).0 == 0)
            .then(|| String::from_utf16_lossy(&id[..len.saturating_sub(1) as usize])).filter(|s| !s.is_empty());
        let _ = CloseHandle(p);
        (path, aumid)
    }
}

fn display_name(parsing: &str) -> Option<String> {
    unsafe {
        let item: IShellItem = SHCreateItemFromParsingName(&HSTRING::from(parsing), None).ok()?;
        let p = item.GetDisplayName(SIGDN_NORMALDISPLAY).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as _));
        s.filter(|s| !s.is_empty())
    }
}

fn file_description(path: &str) -> Option<String> {
    use windows::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW};
    if path.is_empty() { return None; }
    unsafe {
        let p = HSTRING::from(path);
        let size = GetFileVersionInfoSizeW(&p, None);
        if size == 0 { return None; }
        let mut data = vec![0u8; size as usize];
        GetFileVersionInfoW(&p, None, size, data.as_mut_ptr() as _).ok()?;
        // first language/codepage pair
        let (mut tr, mut n) = (std::ptr::null_mut(), 0u32);
        if !VerQueryValueW(data.as_ptr() as _, &HSTRING::from("\\VarFileInfo\\Translation"), &mut tr, &mut n).as_bool() || n < 4 { return None; }
        let t = *(tr as *const [u16; 2]);
        let (mut v, mut vn) = (std::ptr::null_mut(), 0u32);
        let q = format!("\\StringFileInfo\\{:04x}{:04x}\\FileDescription", t[0], t[1]);
        if !VerQueryValueW(data.as_ptr() as _, &HSTRING::from(q), &mut v, &mut vn).as_bool() || vn == 0 { return None; }
        let s = String::from_utf16_lossy(std::slice::from_raw_parts(v as *const u16, vn as usize)).trim_end_matches('\0').trim().to_string();
        (!s.is_empty()).then_some(s)
    }
}

/// The app's icon as a PNG data URL (64 px), or None (the window shows a neutral icon).
#[tauri::command]
pub async fn app_icon(key: String) -> Option<String> {
    if key.is_empty() { return None; }
    if let Some(v) = ICONS.lock().unwrap().as_ref().and_then(|m| m.get(&key)) { return v.clone(); }
    let k = key.clone();
    let v = tauri::async_runtime::spawn_blocking(move || icon_png(&k)).await.ok().flatten();
    ICONS.lock().unwrap().get_or_insert_with(HashMap::new).insert(key, v.clone());
    v
}

fn icon_png(key: &str) -> Option<String> {
    let dir = config::home().join("icons");
    let file = dir.join(format!("{:016x}.png", reach_core::capture::hash(key.as_bytes())));
    if let Ok(b) = std::fs::read(&file) { return Some(format!("data:image/png;base64,{}", util::base64(&b))); }
    const N: i32 = 64;
    let bgra = unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let f: IShellItemImageFactory = match SHCreateItemFromParsingName(&HSTRING::from(key), None) { Ok(f) => f, Err(e) => { config::log(&format!("app icon: {key}: {e}")); return None } };
        let bmp = match f.GetImage(SIZE { cx: N, cy: N }, SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK) { Ok(b) => b, Err(e) => { config::log(&format!("app icon: {key}: {e}")); return None } };
        let dc = CreateCompatibleDC(None);
        let mut bmi = BITMAPINFO { bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: N, biHeight: -N, biPlanes: 1, biBitCount: 32, biCompression: BI_RGB.0, ..Default::default() }, ..Default::default() };
        let mut px = vec![0u8; (N * N * 4) as usize];
        let rows = GetDIBits(dc, bmp, 0, N as u32, Some(px.as_mut_ptr() as _), &mut bmi, DIB_RGB_COLORS);
        let _ = DeleteDC(dc);
        let _ = DeleteObject(bmp.into());
        if rows == 0 { return None; }
        px
    };
    let png = png_rgba(&unpremultiply(bgra), N as u32, N as u32);
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(&file, &png);
    Some(format!("data:image/png;base64,{}", util::base64(&png)))
}

/// BGRA premultiplied (as the shell hands it out) to straight RGBA.
fn unpremultiply(mut p: Vec<u8>) -> Vec<u8> {
    let any_alpha = p.chunks(4).any(|c| c[3] != 0);
    for c in p.chunks_mut(4) {
        let a = if any_alpha { c[3] } else { 255 }; // no alpha channel at all: opaque
        let f = |v: u8| if a == 0 { 0 } else { ((v as u32 * 255 + a as u32 / 2) / a as u32).min(255) as u8 };
        let (b, g, r) = (f(c[0]), f(c[1]), f(c[2]));
        c[0] = r; c[1] = g; c[2] = b; c[3] = a;
    }
    p
}

/// A minimal PNG: one IDAT of stored (uncompressed) deflate blocks. Icons are 16 KB, so no compressor.
fn png_rgba(rgba: &[u8], w: u32, h: u32) -> Vec<u8> {
    fn crc(data: &[u8]) -> u32 {
        let mut c = !0u32;
        for &b in data { c ^= b as u32; for _ in 0..8 { c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 }; } }
        !c
    }
    fn chunk(out: &mut Vec<u8>, kind: &[u8], data: &[u8]) {
        out.extend((data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec(); body.extend(data);
        out.extend(&body); out.extend(crc(&body).to_be_bytes());
    }
    let mut raw = Vec::with_capacity(((w * 4 + 1) * h) as usize);
    for row in rgba.chunks((w * 4) as usize) { raw.push(0); raw.extend(row); }
    let mut z = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65535).collect();
    for (i, b) in blocks.iter().enumerate() {
        z.push((i + 1 == blocks.len()) as u8);
        z.extend((b.len() as u16).to_le_bytes()); z.extend((!(b.len() as u16)).to_le_bytes()); z.extend(*b);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &x in &raw { a = (a + x as u32) % 65521; b = (b + a) % 65521; }
    z.extend(((b << 16) | a).to_be_bytes());
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut ihdr = w.to_be_bytes().to_vec(); ihdr.extend(h.to_be_bytes()); ihdr.extend([8, 6, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr); chunk(&mut out, b"IDAT", &z); chunk(&mut out, b"IEND", &[]);
    out
}

// ---------- screens ----------

/// Monitors, numbered the way people count them here: the main one is Screen 1, then left to right.
pub fn screens() -> Vec<dpi::Display> {
    let mut d = dpi::displays();
    d.sort_by_key(|m| (!m.primary, m.bounds.x, m.bounds.y));
    d
}

/// Which screen (1-based) a point is on.
pub fn screen_at(x: i32, y: i32) -> u32 {
    screens().iter().position(|m| x >= m.bounds.x && x < m.bounds.right() && y >= m.bounds.y && y < m.bounds.y + m.bounds.h).map(|i| i as u32 + 1).unwrap_or(1)
}

#[tauri::command]
pub fn list_screens() -> Vec<Value> {
    screens().iter().enumerate().map(|(i, m)| json!({ "n": i + 1, "primary": m.primary, "w": m.bounds.w, "h": m.bounds.h, "scale": m.dpi as f64 / 96.0 })).collect()
}

/// A small live picture of screen `n` (JPEG data URL, 320 px wide). In test mode
/// (REACH_TEST_SCREEN) only the test windows are drawn, on a plain background.
#[tauri::command]
pub async fn screen_thumb(n: u32) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || {
        let m = screens().into_iter().nth(n.checked_sub(1)? as usize)?;
        let r = m.bounds;
        let (w, h) = (320, (r.h as i64 * 320 / r.w.max(1) as i64) as i32);
        // test mode: only the test windows; None (screen hidden) when none is on this screen
        let px = if apps::fence_on() { apps::grab_fenced(r, w, h) } else { reach_core::capture::grab(r, w, h) }.ok()?;
        let jpg = reach_core::capture::jpeg(&px, w, h, 72).ok()?;
        Some(format!("data:image/jpeg;base64,{}", util::base64(&jpg)))
    }).await.ok().flatten()
}

#[cfg(test)]
mod tests {
    #[test]
    fn png_is_well_formed() {
        let png = super::png_rgba(&[255, 0, 0, 255, 0, 255, 0, 128], 2, 1);
        assert_eq!(&png[1..4], b"PNG");
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(&png[png.len() - 8..png.len() - 4], b"IEND");
    }
}
