//! The synth's global amp envelope and EQ, heard through the app's own audio path:
//!   cargo run -p cogs-app --release --example synth_global_check
use cogs::link::{AudioSide, QUANTUM};
use cogs_dsp::params::*;
use cogs_dsp::WavePatch;
use cogs_sim::World;
use serde_json::{json, Value};

const SR: f64 = 48000.;
/// One note of the patch, alone: its samples.
fn render(p: &WavePatch, secs: f64) -> Vec<f32> { play(p, secs, None) }
/// The world holding this patch on its first part, as the audio side is sent it.
fn world(p: &WavePatch) -> (World, u64) {
    let mut j: Value = serde_json::from_str(include_str!("../../../../cogs/src/default-world.json")).unwrap();
    let mut v = p.to_json();
    v["kind"] = json!("wave");
    j["readers"][0]["voice"] = v;
    j["SPACE"]["wet"] = json!(0); // the room's reverb would blur where the voice itself ends
    let id = j["readers"][0]["id"].as_u64().unwrap();
    let mut w = World::from_str(&j.to_string()).unwrap();
    w.set_paused(true);
    (w, id)
}
/// An audition holds its note for 0.3 s; `swap` sends a changed patch while it sounds, at 0.25 s.
fn play(p: &WavePatch, secs: f64, swap: Option<&WavePatch>) -> Vec<f32> {
    let (w, id) = world(p);
    let mut a = AudioSide::new(SR as f32);
    a.message(&cogs::link::world_message(&w));
    for k in 0..64 { a.block(k as f64 * QUANTUM as f64 / SR); a.clear_outbox(); }
    a.message(&format!(r#"{{"type":"audition","part":{id},"midi":60,"key":5}}"#));
    let mut out = Vec::new();
    for k in 0..(secs * SR / QUANTUM as f64) as usize {
        let t = k as f64 * QUANTUM as f64 / SR;
        let at = |s: f64| k == (s * SR / QUANTUM as f64) as usize;
        if at(0.25) { if let Some(q) = swap { a.message(&cogs::link::world_message(&world(q).0)); } }
        a.block(t); a.clear_outbox(); out.extend_from_slice(&a.l[..QUANTUM]);
    }
    out
}
fn rms(x: &[f32], t0: f64, t1: f64) -> f64 {
    let s = &x[(t0 * SR) as usize..((t1 * SR) as usize).min(x.len())];
    (s.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / s.len().max(1) as f64).sqrt()
}
fn main() {
    let base = WavePatch::preset("pluck").unwrap();
    let dry = render(&base, 4.);

    // saved and read back, nothing is lost; off, the new knobs change nothing
    let mut p = base.clone();
    p.set("amp.a", 0.3); p.set("amp.r", 2.); p.set("eq.m1", 9.); p.set("eq.m1f", 1000.);
    let back = WavePatch::from_json(&p.to_json());
    assert_eq!(back.get("amp.r"), Some(2.)); assert_eq!(back.get("eq.m1f"), Some(1000.));
    assert_eq!(render(&p, 4.), dry, "both off: the same sound, sample for sample");

    // a patch saved before these existed reads as off and flat
    let mut old = base.to_json();
    old.as_object_mut().unwrap().remove("amp");
    for k in ["lowf", "m1", "m1f", "m1q", "m2", "m2f", "m2q", "highf"] { old["fx"]["eq"].as_object_mut().unwrap().remove(k); }
    let o = WavePatch::from_json(&old);
    assert!(!o.amp_on); assert_eq!(o.get("eq.m1"), Some(0.)); assert_eq!(o.get("eq.lowf"), Some(200.));

    // on: a slow attack holds the start back; a long release outlasts the oscillators' own
    p.amp_on = true;
    let shaped = render(&p, 4.);
    println!("first 40 ms  off {:.4}  on {:.4}", rms(&dry, 0.05, 0.09), rms(&shaped, 0.05, 0.09));
    assert!(rms(&shaped, 0., 0.02) < 0.25 * rms(&dry, 0., 0.02).max(1e-6) || rms(&dry, 0., 0.02) < 1e-6);
    println!("at 2.5 s     off {:.5}  on {:.5}", rms(&dry, 2.5, 2.7), rms(&shaped, 2.5, 2.7));
    assert!(rms(&shaped, 2.5, 2.7) > 4. * rms(&dry, 2.5, 2.7).max(1e-6));
    let mut q = p.clone(); q.set("amp.r", 0.05); q.set("amp.a", 0.);
    q.fx_on = Default::default();
    let short = render(&q, 3.);
    println!("short release at 2.5 s: {:.7}", rms(&short, 2.5, 2.9));
    assert!(rms(&short, 2.5, 2.9) < 1e-5, "a short release ends the voice");

    // the EQ does what its curve says
    let mut e = base.clone();
    e.fx_on[FX_EQ] = true;
    assert_eq!(render(&e, 1.), render(&base, 1.), "flat: untouched");
    e.set("eq.m1", 12.); e.set("eq.m1f", 1000.); e.set("eq.m1q", 1.);
    assert!((cogs_dsp::wave::eq_db(&e.values, 1000., SR) - 12.).abs() < 0.2);
    assert!(cogs_dsp::wave::eq_db(&e.values, 60., SR).abs() < 0.5);
    let loud = render(&e, 1.);
    println!("EQ +12 dB at 1 kHz: rms {:.4} -> {:.4}", rms(&dry, 0.02, 0.5), rms(&loud, 0.02, 0.5));
    assert!(rms(&loud, 0.02, 0.5) > 1.15 * rms(&dry, 0.02, 0.5));
    // a route taken off a knob while its note sounds: the engine carries on, and the knob stops moving
    let cut = wave_params().iter().position(|d| d.path == "osc1.cut").unwrap() as u16;
    let route = |src: usize, amt: f64| cogs_dsp::patch::ModRoute { src: src as u8, dst: cut, amt, bi: false, off: false, aux: None, aux_inv: false, curve: 0. };
    let mut m = base.clone();
    m.mods = vec![route(SRC_ENV0, 0.3), route(SRC_LFO0, 0.3), route(SRC_MAC0, 0.5)];
    let mut less = m.clone(); less.mods.pop();
    let swapped = play(&m, 1., Some(&less));
    assert!(swapped.iter().all(|x| x.is_finite()) && rms(&swapped, 0.26, 0.3) > 1e-4, "still sounding after the route goes");
    println!("ok");
}
