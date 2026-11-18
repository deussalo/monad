//! The UI holding the world while the sound plays: with the machine running ahead (`set_lookahead`), every note still
//! comes, on time. Blocks of 240 frames; a thread takes the world for `hold` ms, `every` ms apart.
//!   cargo run -p cogs-app --release --example lock_miss
use cogs_sim::World;
use std::sync::atomic::Ordering::Relaxed;
fn run(src: &str, look: u64, hold_ms: u64) -> (u64, u64, u64, u64) {
    let mut w = World::from_str(src).unwrap(); w.set_paused(false);
    let (sh, mut render) = cogs::play::start(w, 48000.);
    render.set_lookahead(look);
    std::thread::sleep(std::time::Duration::from_millis(250));
    let s2 = sh.clone();
    let h = std::thread::spawn(move || while s2.running.load(Relaxed) {
        if hold_ms > 0 { let g = s2.world.lock().unwrap(); std::thread::sleep(std::time::Duration::from_millis(hold_ms)); drop(g) }
        std::thread::sleep(std::time::Duration::from_millis(40));
    });
    let (mut l, mut r) = ([0f32; 240], [0f32; 240]);
    for _ in 0..1000 { render.block(&mut l, &mut r, 0.01); std::thread::sleep(std::time::Duration::from_millis(5)) }
    let m = &sh.meters;
    let out = (m.events.load(Relaxed), m.lock_misses.load(Relaxed), m.blocks.load(Relaxed), m.late_notes.load(Relaxed));
    sh.running.store(false, Relaxed); let _ = h.join();
    out
}
fn main() {
    for (name, src) in cogs::WORLDS { for (look, hold) in [(0, 8), (1152, 8), (1152, 16), (1152, 0)] {
        let (ev, miss, blocks, late) = run(src, look, hold);
        println!("{name}: lookahead {:.0} ms, UI holds {hold} ms: {ev} notes, world locked in {miss} of {blocks} blocks, {late} notes late", look as f64 / 48.);
    } }
}
