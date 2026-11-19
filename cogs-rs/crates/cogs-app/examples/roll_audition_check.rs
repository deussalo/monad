//! Dragging a note across the roll's rows sounds each new pitch: the messages the app sends, in its order (the new
//! preview, then the release of the last), through the app's own audio path.
//!   cargo run -p cogs-app --release --example roll_audition_check
use cogs::link::{AudioSide, QUANTUM};
use cogs::stage::roll::PREVIEW_KEYS;
use cogs_dsp::WavePatch;
use cogs_sim::World;
use serde_json::{json, Value};

const SR: f64 = 48000.;
/// Three pitches a drag crosses, 0.15 s apart; each step sends the new audition, then releases the last key, as the
/// app's frame does. The level heard in the 100 ms after each new pitch begins.
fn drag(keys: [u32; 3]) -> Vec<f64> {
    let mut j: Value = serde_json::from_str(include_str!("../../../../cogs/src/default-world.json")).unwrap();
    let mut p = WavePatch::preset("pluck").unwrap();
    p.amp_on = true; p.set("amp.s", 1.); p.set("amp.r", 0.05); // a held sound, so a release shows
    let mut v = p.to_json();
    v["kind"] = json!("wave");
    j["readers"][0]["voice"] = v;
    j["SPACE"]["wet"] = json!(0);
    let id = j["readers"][0]["id"].as_u64().unwrap();
    let mut w = World::from_str(&j.to_string()).unwrap();
    w.set_paused(true);
    let mut a = AudioSide::new(SR as f32);
    a.message(&cogs::link::world_message(&w));
    for k in 0..64 { a.block(k as f64 * QUANTUM as f64 / SR); a.clear_outbox(); }
    let step = (0.15 * SR / QUANTUM as f64) as usize;
    let mut out: Vec<f32> = Vec::new();
    for k in 0..3 * step {
        if k % step == 0 {
            let i = k / step;
            a.message(&format!(r#"{{"type":"audition","part":{id},"midi":{},"key":{}}}"#, 60 + 2 * i, keys[i]));
            if i > 0 { a.message(&format!(r#"{{"type":"release","key":{}}}"#, keys[i - 1])); }
        }
        a.block(k as f64 * QUANTUM as f64 / SR); a.clear_outbox(); out.extend_from_slice(&a.l[..QUANTUM]);
    }
    (0..3).map(|i| {
        let s = &out[i * step * QUANTUM..(i * step * QUANTUM + (0.1 * SR) as usize)];
        (s.iter().map(|x| (*x as f64).powi(2)).sum::<f64>() / s.len() as f64).sqrt()
    }).collect()
}
fn main() {
    let old = drag([383; 3]);
    let new = drag([PREVIEW_KEYS + 1, PREVIEW_KEYS + 2, PREVIEW_KEYS + 3]);
    println!("level after each new pitch, one key for all (before): {old:.4?}");
    println!("level after each new pitch, a key each (now):         {new:.4?}");
    for i in 1..3 { assert!(new[i] > 0.5 * new[0], "pitch {i} sounds as the first did"); }
    assert!(old[1] < 0.5 * new[1], "the old way cut the second pitch: the check would not catch the bug otherwise");
    println!("ok");
}
