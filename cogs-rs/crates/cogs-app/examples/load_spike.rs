//! What loading a world costs the audio callback: blocks of 192 frames, a world played, then replaced as the panel does
//! it. Prints each block's CPU time, normal and worst, before and after the load, with the sync thread doing its share.
//!   cargo run -p cogs-app --release --example load_spike
use cogs_sim::World;
/// This thread's CPU time, so that a busy machine's preemption does not count as the block's cost.
fn cpu_ms() -> f64 { std::fs::read_to_string("/proc/thread-self/schedstat").ok().and_then(|s| s.split(' ').next()?.parse::<f64>().ok()).unwrap_or(0.) / 1e6 }
fn main() {
    for (a, b) in [(0usize, 1usize), (1, 2), (2, 0)] {
        let mut w = World::from_str(cogs::WORLDS[a].1).unwrap(); w.set_paused(false);
        let (sh, mut render) = cogs::play::start(w, 48000.);
        std::thread::sleep(std::time::Duration::from_millis(300));
        let (mut l, mut r) = ([0f32; 192], [0f32; 192]);
        let (mut before, mut after): (Vec<f64>, Vec<f64>) = (vec![], vec![]);
        let mut block = |render: &mut cogs::play::Render, out: &mut Vec<f64>| { let c0 = cpu_ms(); render.block(&mut l, &mut r, 0.01); out.push(cpu_ms() - c0); std::thread::sleep(std::time::Duration::from_millis(4)) };
        for _ in 0..2500 { block(&mut render, &mut before) }
        let mut nw = World::from_str(cogs::WORLDS[b].1).unwrap();
        { let mut w = sh.world.lock().unwrap(); cogs::play::bump_versions(&w, &mut nw); let p = w.clock.paused; *w = nw; w.set_paused(p); }
        for _ in 0..250 { block(&mut render, &mut after) }
        let mut early: Vec<(usize, f64)> = before.iter().copied().enumerate().collect();
        early.sort_by(|x, y| y.1.total_cmp(&x.1));
        println!("  playing, worst blocks (index, ms): {:?}", early[..5].iter().map(|x| (x.0, (x.1 * 100.).round() / 100.)).collect::<Vec<_>>());
        before.sort_by(f64::total_cmp);
        let med = before[before.len() / 2];
        let mut top: Vec<(usize, f64)> = after.iter().copied().enumerate().collect();
        top.sort_by(|x, y| y.1.total_cmp(&x.1));
        println!("{} -> {}: normal block {med:.2} ms, worst before {:.2}; after the load, worst blocks (index, ms): {:?}; over 4 ms: {}", cogs::WORLDS[a].0, cogs::WORLDS[b].0, before[before.len() - 1],
            top[..6].iter().map(|x| (x.0, (x.1 * 100.).round() / 100.)).collect::<Vec<_>>(), after.iter().filter(|x| **x > 4.).count());
        sh.running.store(false, std::sync::atomic::Ordering::Relaxed);
    }
}
