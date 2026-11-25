//! Minimal registry access plus the user-PATH logic (HKCU\Environment only, keeps
//! %VARS% and the value type, never touches the system PATH).

use crate::util::{from_wide, wide};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Registry::*;
use windows::Win32::UI::WindowsAndMessaging::{SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE};

pub const HKLM: HKEY = HKEY_LOCAL_MACHINE;
pub const HKCU: HKEY = HKEY_CURRENT_USER;

/// Windows' taskbar and tray theme (Settings > Personalization > Colors > "Choose your mode": Windows).
pub fn taskbar_light() -> bool {
    let (mut v, mut n) = (0u32, 4u32);
    unsafe { RegGetValueW(HKCU, windows::core::w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"), windows::core::w!("SystemUsesLightTheme"),
        RRF_RT_REG_DWORD, None, Some(&mut v as *mut u32 as _), Some(&mut n)) }.is_ok() && v == 1
}

/// (value, is_expand_sz)
pub fn read(root: HKEY, key: &str, name: &str) -> Option<(String, bool)> {
    unsafe {
        let mut h = HKEY::default();
        let k = wide(key);
        if RegOpenKeyExW(root, PCWSTR(k.as_ptr()), Some(0), KEY_READ, &mut h).is_err() { return None; }
        let n = wide(name);
        let (mut ty, mut len) = (REG_VALUE_TYPE::default(), 0u32);
        let mut r = None;
        if RegQueryValueExW(h, PCWSTR(n.as_ptr()), None, Some(&mut ty), None, Some(&mut len)).is_ok() && (ty == REG_SZ || ty == REG_EXPAND_SZ) {
            let mut buf = vec![0u16; len as usize / 2 + 1];
            if RegQueryValueExW(h, PCWSTR(n.as_ptr()), None, None, Some(buf.as_mut_ptr() as *mut u8), Some(&mut len)).is_ok() {
                r = Some((from_wide(&buf), ty == REG_EXPAND_SZ));
            }
        }
        let _ = RegCloseKey(h);
        r
    }
}

pub fn write(root: HKEY, key: &str, name: &str, val: &str, expand: bool) -> Result<(), String> {
    unsafe {
        let mut h = HKEY::default();
        let k = wide(key);
        RegCreateKeyExW(root, PCWSTR(k.as_ptr()), None, PCWSTR::null(), REG_OPTION_NON_VOLATILE, KEY_WRITE, None, &mut h, None).ok().map_err(|e| e.to_string())?;
        let n = wide(name);
        let v = wide(val);
        let bytes = std::slice::from_raw_parts(v.as_ptr() as *const u8, v.len() * 2);
        let r = RegSetValueExW(h, PCWSTR(n.as_ptr()), None, if expand { REG_EXPAND_SZ } else { REG_SZ }, Some(bytes)).ok().map_err(|e| e.to_string());
        let _ = RegCloseKey(h);
        r
    }
}

pub fn delete_tree(root: HKEY, key: &str) {
    let k = wide(key);
    unsafe { let _ = RegDeleteTreeW(root, PCWSTR(k.as_ptr())); let _ = RegDeleteKeyW(root, PCWSTR(k.as_ptr())); }
}

pub fn delete_value(root: HKEY, key: &str, name: &str) {
    unsafe {
        let mut h = HKEY::default();
        let k = wide(key);
        if RegOpenKeyExW(root, PCWSTR(k.as_ptr()), Some(0), KEY_WRITE, &mut h).is_ok() {
            let n = wide(name);
            let _ = RegDeleteValueW(h, PCWSTR(n.as_ptr()));
            let _ = RegCloseKey(h);
        }
    }
}

pub struct UserPath { pub key: String }

impl UserPath {
    pub fn real() -> UserPath { UserPath { key: "Environment".into() } }

    fn norm(p: &str) -> String {
        let p = p.trim().trim_matches('"');
        let expanded = expand_env(p);
        let full = std::path::absolute(&expanded).map(|x| x.to_string_lossy().into_owned()).unwrap_or(expanded);
        full.trim_end_matches('\\').to_lowercase()
    }

    pub fn read(&self) -> (String, bool) { read(HKCU, &self.key, "Path").unwrap_or((String::new(), true)) }

    fn write(&self, v: &str, expand: bool) -> Result<(), String> {
        write(HKCU, &self.key, "Path", v, expand)?;
        if self.key == "Environment" {
            let e = wide("Environment");
            unsafe { let _ = SendMessageTimeoutW(HWND_BROADCAST, WM_SETTINGCHANGE, WPARAM(0), LPARAM(e.as_ptr() as isize), SMTO_ABORTIFHUNG, 2000, None); }
        }
        Ok(())
    }

    pub fn contains(&self, dir: &str) -> bool {
        let n = Self::norm(dir);
        self.read().0.split(';').any(|e| !e.trim().is_empty() && Self::norm(e) == n)
    }

    pub fn add(&self, dir: &str) -> bool {
        let (v, expand) = self.read();
        if self.contains(dir) { return false; }
        let nv = v.trim_end_matches([' ', ';']);
        let nv = if nv.is_empty() { dir.to_string() } else { format!("{nv};{dir}") };
        if nv.len() > 32000 { return false; }
        self.write(&nv, expand).is_ok()
    }

    pub fn remove(&self, dir: &str) -> bool {
        let (v, expand) = self.read();
        let n = Self::norm(dir);
        let keep: Vec<&str> = v.split(';').filter(|e| e.trim().is_empty() || Self::norm(e) != n).collect();
        if keep.len() == v.split(';').count() { return false; }
        self.write(keep.join(";").trim_end_matches(';'), expand).is_ok()
    }
}

pub fn expand_env(s: &str) -> String {
    use windows::Win32::System::Environment::ExpandEnvironmentStringsW;
    let w = wide(s);
    let mut buf = vec![0u16; 4096];
    let n = unsafe { ExpandEnvironmentStringsW(PCWSTR(w.as_ptr()), Some(&mut buf)) };
    if n == 0 || n as usize > buf.len() { s.to_string() } else { from_wide(&buf) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn user_path_logic_on_scratch_key() {
        let p = UserPath { key: r"Software\ReachSelfTestRs\Environment".into() };
        write(HKCU, &p.key, "Path", r"C:\A;%SystemRoot%\x;C:\Tools\reach\;", true).unwrap();
        assert!(!p.add(r"C:\Tools\reach"));
        assert!(!p.add(r"c:\tools\REACH"));
        assert!(p.add(r"D:\New Dir"));
        assert!(!p.add(r"D:\New Dir"));
        assert!(p.contains(r"D:\New Dir\"));
        assert_eq!(p.read(), (r"C:\A;%SystemRoot%\x;C:\Tools\reach\;D:\New Dir".to_string(), true));
        assert!(p.remove(r"d:\new dir"));
        assert_eq!(p.read().0, r"C:\A;%SystemRoot%\x;C:\Tools\reach\");
        delete_value(HKCU, &p.key, "Path");
        assert!(p.add(r"E:\x") && p.contains(r"E:\x"));
        delete_tree(HKCU, r"Software\ReachSelfTestRs");
    }
}
