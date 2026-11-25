// Test helper: read or set a monitor's display scaling (same as Settings > Display > Scale).
//   setdpi list                 -> "<n> <device> rec <pct> cur <pct>"
//   setdpi set <n> <percent>
// Uses the DisplayConfig DPI-scale requests (-3 get, -4 set) that Settings itself uses.
use windows::Win32::Devices::Display::*;
use windows::Win32::Foundation::LUID;

const SCALES: [u32; 12] = [100, 125, 150, 175, 200, 225, 250, 300, 350, 400, 450, 500];

#[repr(C)] #[derive(Default)]
struct Get { header: DISPLAYCONFIG_DEVICE_INFO_HEADER, min: i32, cur: i32, max: i32 }
#[repr(C)] #[derive(Default)]
struct Set { header: DISPLAYCONFIG_DEVICE_INFO_HEADER, rel: i32 }

fn sources() -> Vec<(LUID, u32, String)> {
    unsafe {
        let (mut np, mut nm) = (0u32, 0u32);
        let _ = GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut np, &mut nm);
        let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); np as usize];
        let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); nm as usize];
        let _ = QueryDisplayConfig(QDC_ONLY_ACTIVE_PATHS, &mut np, paths.as_mut_ptr(), &mut nm, modes.as_mut_ptr(), None);
        paths.truncate(np as usize);
        let mut out = Vec::new();
        for p in paths {
            let mut name = DISPLAYCONFIG_SOURCE_DEVICE_NAME::default();
            name.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME;
            name.header.size = std::mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32;
            name.header.adapterId = p.sourceInfo.adapterId;
            name.header.id = p.sourceInfo.id;
            DisplayConfigGetDeviceInfo(&mut name.header);
            let n = String::from_utf16_lossy(&name.viewGdiDeviceName).trim_end_matches('\0').to_string();
            if !out.iter().any(|(_, _, x): &(LUID, u32, String)| *x == n) { out.push((p.sourceInfo.adapterId, p.sourceInfo.id, n)); }
        }
        out
    }
}

fn get(a: LUID, id: u32) -> Get {
    let mut g = Get::default();
    g.header.r#type = DISPLAYCONFIG_DEVICE_INFO_TYPE(-3);
    g.header.size = std::mem::size_of::<Get>() as u32;
    g.header.adapterId = a;
    g.header.id = id;
    unsafe { DisplayConfigGetDeviceInfo(&mut g.header) };
    g
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let src = sources();
    match args.first().map(String::as_str) {
        Some("set") if args.len() == 3 => {
            let i: usize = args[1].parse().expect("n");
            let pct: u32 = args[2].parse().expect("percent");
            let (a, id, _) = &src[i];
            let g = get(*a, *id);
            let rec = g.min.unsigned_abs() as usize;
            let target = SCALES.iter().position(|s| *s == pct).expect("scale not in list") as i32 - rec as i32;
            if target < g.min || target > g.max { eprintln!("{pct}% not allowed here"); std::process::exit(2); }
            let mut s = Set::default();
            s.header.r#type = DISPLAYCONFIG_DEVICE_INFO_TYPE(-4);
            s.header.size = std::mem::size_of::<Set>() as u32;
            s.header.adapterId = *a;
            s.header.id = *id;
            s.rel = target;
            let r = unsafe { DisplayConfigSetDeviceInfo(&s.header) };
            println!("set {} -> {pct}% ({r})", src[i].2);
        }
        _ => for (i, (a, id, n)) in src.iter().enumerate() {
            let g = get(*a, *id);
            let rec = g.min.unsigned_abs() as usize;
            let cur = (rec as i32 + g.cur) as usize;
            println!("{i} {n} rec {} cur {} max {}", SCALES[rec], SCALES[cur.min(11)], SCALES[(rec as i32 + g.max) as usize]);
        },
    }
}
