//! The web build's two halves, natively: the worklet's AudioSide and the UI's own world, advanced to the heard time
//! and sent over every `every` seconds (as web.rs does while something is being dragged). Machine notes must come
//! out the same as with no syncing: nothing played twice, nothing skipped.
//!   cargo run -p cogs-app --release --example web_link [every_s] [world.json]
use cogs::link::{self, AudioSide, QUANTUM};
use cogs_sim::World;

fn run(json: &str, every: f64, secs: f64, edit: fn(&mut World)) -> (u64, f32) {
    let sr = 48000.;
    let mut a = AudioSide::new(sr as f32);
    let mut ui = World::from_str(json).unwrap();
    ui.set_paused(false);
    edit(&mut ui);
    a.message(&link::world_message(&ui));
    let (mut t, mut ct, mut last, mut next, mut peak) = (0., 0., (0., 0.), every, 0f32);
    let n = (secs * sr / QUANTUM as f64) as usize;
    for _ in 0..n {
        a.block(ct);
        ct += QUANTUM as f64 / sr;
        for i in 0..QUANTUM { peak = peak.max(a.l[i].abs()) }
        if let Some(s) = a.outbox() { let v: serde_json::Value = serde_json::from_str(s).unwrap(); last = (v["t"].as_f64().unwrap(), v["ct"].as_f64().unwrap()); a.clear_outbox(); }
        // the UI: heard = the worklet's time, less 40 ms of output latency
        let heard = last.0 - (last.1 - (ct - 0.04));
        if heard > ui.clock.sim_t { ui.advance(heard, &mut |_| {}); }
        t += QUANTUM as f64 / sr;
        if every > 0. && t >= next { next += every; a.message(&link::world_message(&ui)); }
    }
    let ev = a.sh.meters.events.load(std::sync::atomic::Ordering::Relaxed);
    (ev, peak)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let every: f64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0.1);
    let path = args.get(2).cloned().unwrap_or_else(|| "../cogs/src/default-world.json".into());
    let json = std::fs::read_to_string(&path).unwrap();
    let (e0, p0) = run(&json, 0., 12., |_| {});
    let (e1, p1) = run(&json, every, 12., |_| {});
    // a pin added on a turning plate: it sounds, unless a key is still held on it (skip), which must reach the worklet
    fn pin(w: &mut World, skip: f64) {
        let Some(c) = w.cogs.iter_mut().find(|c| c.w != 0. && c.track.as_ref().map_or(false, |t| !t.dots.is_empty())) else { return };
        let t = c.track.as_mut().unwrap(); let mut d = t.dots[0].clone(); d.a += 0.123; d.p = 1.; d.skip = skip; t.dots.push(d);
    }
    let (free, _) = run(&json, every, 12., |w| pin(w, f64::NEG_INFINITY));
    let (held, _) = run(&json, every, 12., |w| pin(w, f64::INFINITY));
    println!("an added pin: free {free} notes; held by a key {held} notes (must equal the {e1} without it)");
    println!("{path}: 12 s, no sync: {e0} notes (peak {p0:.3}); synced every {every} s: {e1} notes (peak {p1:.3})");
}

