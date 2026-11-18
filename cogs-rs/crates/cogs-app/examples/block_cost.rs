//! What the audio callback costs per second of sound at each block size, and how much of it is the machine: every block
//! in a tight loop, each figure the best of three runs (a shared machine only ever adds time).
//!   cargo run -p cogs-app --release --example block_cost
use cogs_sim::World;
use std::time::Instant;
const SECS: f64 = 8.;
fn sim(src: &str, n: usize) -> f64 {
    let mut w = World::from_str(src).unwrap(); w.set_paused(false);
    let dt = n as f64 / 48000.; let (mut t, mut ev) = (0., 0u64);
    let t0 = Instant::now();
    while t < SECS { t += dt; w.advance(t, &mut |_| ev += 1) }
    std::hint::black_box(ev);
    t0.elapsed().as_secs_f64() * 1000. / SECS
}
fn all(src: &str, n: usize) -> f64 {
    let mut w = World::from_str(src).unwrap(); w.set_paused(false);
    let (sh, mut render) = cogs::play::start(w, 48000.);
    std::thread::sleep(std::time::Duration::from_millis(250));
    let (mut l, mut r) = (vec![0f32; n], vec![0f32; n]);
    let blocks = (SECS * 48000. / n as f64) as usize;
    let t0 = Instant::now();
    for _ in 0..blocks { render.block(&mut l, &mut r, 0.01) }
    let e = t0.elapsed().as_secs_f64() * 1000. / SECS;
    sh.running.store(false, std::sync::atomic::Ordering::Relaxed);
    e
}
fn best(f: impl Fn() -> f64) -> f64 { (0..3).map(|_| f()).fold(f64::MAX, f64::min) }
fn main() {
    for (name, src) in cogs::WORLDS {
        println!("{name}: ms of CPU per second of sound");
        for n in [64, 128, 192, 256, 480, 960] {
            let (a, s) = (best(|| all(src, n)), best(|| sim(src, n)));
            println!("  {n:4} frames: all {a:6.1}   machine {s:6.1}   sound {:6.1}", a - s);
        }
    }
}
