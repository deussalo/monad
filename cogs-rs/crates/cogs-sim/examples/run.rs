//! Load the demo world and Capstan, advance each 10 s in 15 ms ticks (as the page's timer does), print event counts
//! per source and the first events, and round-trip the JSON.
//! `cargo run -p cogs-sim --release --example run [-- --dump DIR]` also writes DIR/<name>.events.json and
//! DIR/<name>.save.json for comparing with the JS (examples/js_run.js).

use cogs_sim::{Event, Source, World};
use std::collections::BTreeMap;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dump = args.iter().position(|a| a == "--dump").and_then(|i| args.get(i + 1)).cloned();
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../cogs");
    let secs: f64 = std::env::var("SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(10.0);
    for (name, path) in [("default", "src/default-world.json"), ("capstan", "worlds/capstan.json"), ("dubstep", "worlds/dubstep.json")] {
        let text = std::fs::read_to_string(format!("{root}/{path}")).expect("read world");
        let t0 = std::time::Instant::now();
        let mut w = World::from_str(&text).expect("load");
        let load = t0.elapsed();
        let saved = w.serialize();
        // round trip: what it writes, it reads back to the same
        let again = World::from_json(&saved).expect("reload").serialize();
        let same = saved == again;

        w.set_paused(false);
        let mut evs: Vec<Event> = Vec::with_capacity(4096);
        let tick = 0.015;
        let n = (secs / tick).round() as usize;
        let t1 = std::time::Instant::now();
        for i in 1..=n {
            w.advance(i as f64 * tick, &mut |e| evs.push(e));
        }
        let run = t1.elapsed();
        let mut per: BTreeMap<&str, usize> = BTreeMap::new();
        for e in &evs {
            *per.entry(match e.src {
                Source::Arm { .. } => "arm>pad",
                Source::Dot { .. } => "dot>reader",
                Source::Pluck { .. } => "arm>string",
                Source::Ball { .. } => "ball",
            }).or_default() += 1;
        }
        println!("{name}: {} parts loaded in {:?}; round trip {}; {} events in {secs} s ({} steps) in {:?}",
            w.cogs.len() + w.arms.len() + w.pads.len() + w.readers.len() + w.cams.len() + w.pits.len() + w.probes.len() + w.fields.len(),
            load, if same { "identical" } else { "DIFFERS" }, evs.len(), w.stats.steps, run);
        println!("  per source: {per:?}");
        for e in evs.iter().take(8) {
            println!("  t={:.6} {:<28} midi={:<6} amp={:.4} vn={:.2} hold={:.3} sends={}", e.t, e.src.tag(), e.midi, e.amp, e.vn, e.hold, e.n_sends);
        }
        let mut motions = 0;
        w.motions(&mut |_| motions += 1);
        println!("  motions: {motions}");
        if let Some(dir) = &dump {
            let list: Vec<serde_json::Value> = evs.iter().map(|e| serde_json::json!([e.t, e.src.tag(), e.midi, e.amp, e.vn, e.hold, e.key])).collect();
            std::fs::write(format!("{dir}/{name}.rs.events.json"), serde_json::to_string(&list).unwrap()).unwrap();
            std::fs::write(format!("{dir}/{name}.rs.save.json"), serde_json::to_string(&saved).unwrap()).unwrap();
            std::fs::write(format!("{dir}/{name}.rs.after.json"), serde_json::to_string(&w.serialize()).unwrap()).unwrap();
        }
    }
}
