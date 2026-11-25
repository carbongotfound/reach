// Debug: physical monitor layout and DPI as Reach sees it (per-monitor-v2 aware).
fn main() {
    reach_core::dpi::make_aware();
    for d in reach_core::dpi::displays() {
        println!("{:?} work {:?} dpi {} ({}%) primary {}", d.bounds, d.work, d.dpi, d.dpi * 100 / 96, d.primary);
    }
}
