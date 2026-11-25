//! Small helpers: tolerant argument parsing (same rules as the C# build), text
//! clipping, random numbers, base64, wide strings, local time.

use serde_json::{Map, Value};
use std::cell::Cell;

pub type Args = Map<String, Value>;

pub fn s(a: &Args, k: &str, def: &str) -> String {
    match a.get(k) {
        None | Some(Value::Null) => def.to_string(),
        Some(Value::String(v)) => v.clone(),
        Some(Value::Bool(b)) => if *b { "True".into() } else { "False".into() },
        Some(v) => v.to_string(),
    }
}

/// First present key wins (mirrors the C# nested Str(a, k1, Str(a, k2, ...)) chains).
pub fn s_any(a: &Args, keys: &[&str], def: &str) -> String {
    for k in keys {
        if let Some(v) = a.get(*k) {
            if !v.is_null() {
                return s(a, k, def);
            }
        }
    }
    def.to_string()
}

pub fn try_int(a: &Args, k: &str) -> Option<i64> {
    match a.get(k)? {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f.round() as i64)),
        Value::String(v) => v.trim().parse::<f64>().ok().map(|f| f.round() as i64),
        Value::Bool(b) => Some(*b as i64),
        _ => None,
    }
}

pub fn int(a: &Args, k: &str, def: i64) -> i64 {
    try_int(a, k).unwrap_or(def)
}

pub fn num(a: &Args, k: &str, def: f64) -> f64 {
    match a.get(k) {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(def),
        Some(Value::String(v)) => v.trim().parse().unwrap_or(def),
        _ => def,
    }
}

pub fn bool_(a: &Args, k: &str, def: bool) -> bool {
    match a.get(k) {
        Some(Value::Bool(b)) => *b,
        Some(Value::String(v)) => match v.as_str() {
            "1" | "true" | "True" | "yes" => true,
            "0" | "false" | "False" | "no" => false,
            _ => def,
        },
        Some(Value::Number(n)) => match n.as_i64() {
            Some(1) => true,
            Some(0) => false,
            _ => def,
        },
        _ => def,
    }
}

/// [1,2] or "1,2" or "1 2" or "1x2".
pub fn ints(a: &Args, k: &str) -> Option<Vec<i64>> {
    match a.get(k)? {
        Value::Array(arr) => Some(arr.iter().map(|v| v.as_f64().map(|f| f.round() as i64)
            .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok())).unwrap_or(0)).collect()),
        Value::String(v) => {
            let out: Vec<i64> = v.split(|c| c == ',' || c == ' ' || c == 'x')
                .filter_map(|p| p.trim().parse().ok()).collect();
            if out.is_empty() { None } else { Some(out) }
        }
        Value::Number(n) => n.as_i64().map(|i| vec![i]),
        _ => None,
    }
}

pub fn clip(s: &str, n: usize) -> String {
    let s = s.replace(['\r', '\n'], " ");
    let s = s.trim();
    if s.chars().count() > n {
        s.chars().take(n - 1).collect::<String>() + "…"
    } else {
        s.to_string()
    }
}

pub fn quote(s: &str) -> String {
    let mut s = s.replace(['\r', '\n'], " ");
    if s.chars().count() > 80 {
        s = s.chars().take(77).collect::<String>() + "...";
    }
    format!("\"{}\"", s.replace('"', "'"))
}

pub fn short(s: &str) -> String {
    let s = s.replace(['\r', '\n'], " ");
    if s.chars().count() > 80 { s.chars().take(77).collect::<String>() + "..." } else { s }
}

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn from_wide(w: &[u16]) -> String {
    let n = w.iter().position(|&c| c == 0).unwrap_or(w.len());
    String::from_utf16_lossy(&w[..n])
}

thread_local!(static RNG: Cell<u64> = Cell::new(seed()));

fn seed() -> u64 {
    let mut b = [0u8; 8];
    fill_random(&mut b);
    u64::from_le_bytes(b) | 1
}

/// Uniform in [0, 1). xorshift64*: plenty for motion jitter, not for secrets.
pub fn rand() -> f64 {
    RNG.with(|r| {
        let mut x = r.get();
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        r.set(x);
        (x.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64 / (1u64 << 53) as f64
    })
}

pub fn fill_random(buf: &mut [u8]) {
    use windows::Win32::Security::Cryptography::{BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG};
    unsafe {
        let _ = BCryptGenRandom(None, buf, BCRYPT_USE_SYSTEM_PREFERRED_RNG);
    }
}

pub fn random_hex(nbytes: usize) -> String {
    let mut b = vec![0u8; nbytes];
    fill_random(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Local time as yyyy-MM-ddTHH:mm:ss (C# "s" format).
pub fn now_iso() -> String {
    let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond)
}

pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn b64() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
    #[test]
    fn arg_parsing() {
        let a: Args = serde_json::from_str(r#"{"c":"10, 20","d":[1.6,2],"b":"yes","n":"7"}"#).unwrap();
        assert_eq!(ints(&a, "c"), Some(vec![10, 20]));
        assert_eq!(ints(&a, "d"), Some(vec![2, 2]));
        assert!(bool_(&a, "b", false));
        assert_eq!(int(&a, "n", 0), 7);
        assert_eq!(clip("abcdef", 4), "abc…");
    }
}
