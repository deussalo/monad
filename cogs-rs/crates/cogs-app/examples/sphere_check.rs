//! The sphere's notes and frame, and what the synth editor is told, through the same code the app runs:
//!   cargo run -p cogs-app --release --example sphere_check
use cogs::link::{self, AudioSide};
use cogs_dsp::params::{param_index, SRC_LFO0, SRC_MAC0};
use cogs_dsp::patch::ModRoute;
use cogs_dsp::WavePatch;
use cogs_sim::{pit::make_ball, Part, World};
use serde_json::{json, Value};

fn world() -> (World, u32, u32, u32) {
    let mut j: Value = serde_json::from_str(include_str!("../../../../cogs/src/default-world.json")).unwrap();
    let mut p = WavePatch::preset("pluck").unwrap();
    let route = |src: usize, to: &str, bi| ModRoute { src: src as u8, dst: param_index(to).unwrap() as u16, amt: 0.4, bi, off: false, aux: None, aux_inv: false, curve: 0. };
    p.mods = vec![route(SRC_LFO0, "osc1.cut", true), route(SRC_MAC0, "osc1.res", false)];
    let mut v = p.to_json();
    v["kind"] = json!("wave");
    j["readers"][0]["voice"] = v;
    let mut w = World::from_str(&j.to_string()).unwrap();
    w.set_paused(false);
    let reader = w.readers[0].id;
    let c = w.cogs[0].clone();
    let cam = w.add_cam(c.x, c.y).expect("a cam on the first cog");
    assert!(w.link_cam(cam, reader));
    w.solve();
    let pit = w.add_pit(900., 900.);
    let p = w.pits.iter_mut().find(|p| p.id == pit).unwrap();
    p.notes = vec![60, 64, 67];
    for (i, n) in [60, 64, 67].into_iter().enumerate() { p.balls.push(make_ball(n, 900. + 30. * i as f64, 900., 0., 0., i as u32 + 1)); }
    p.nb = 4;
    (w, reader, pit, cam)
}

fn main() {
    let (mut ui, reader, pit, _) = world();
    let mut a = AudioSide::new(48000.);
    a.message(&link::world_message(&ui));

    // a note taken away: its balls go to the nearest left, on both sides of the web's link
    let p = ui.pits.iter_mut().find(|p| p.id == pit).unwrap();
    p.notes.retain(|n| *n != 64);
    p.retune();
    let notes: Vec<i32> = p.balls.iter().map(|b| b.note).collect();
    assert_eq!(notes, [60, 67, 67], "64 is nearer 67 than 60");
    // moved and doubled: the balls keep their places in it
    let before = (p.balls[1].x - p.x, p.balls[1].y - p.y);
    let (x, y) = (p.x + 50., p.y - 20.);
    p.reframe(x, y, 440.);
    assert_eq!((p.w, p.h), (440., 440.));
    assert!((p.balls[1].x - p.x - before.0 * 2.).abs() < 1e-9 && (p.balls[1].y - p.y - before.1 * 2.).abs() < 1e-9);
    let mut q = p.clone();
    q.reframe(x, y, 1.);
    assert_eq!(q.w, cogs_sim::pit::PIT_MIN_D);
    a.message(&link::world_message(&ui));
    {
        let w = a.sh.world.lock().unwrap();
        let p = w.pits.iter().find(|p| p.id == pit).unwrap();
        assert_eq!(p.balls.iter().map(|b| b.note).collect::<Vec<_>>(), [60, 67, 67], "the worklet takes the notes");
        assert_eq!(p.w, 440.);
        assert!(p.balls.iter().all(|b| (b.x - p.x).hypot(b.y - p.y) <= p.w / 2.), "the worklet's balls come along");
    }
    println!("sphere: notes retune at once, the frame carries its balls, the worklet follows");

    // a turning cam moves its macro, and the editor can read it from the world alone
    let slot = (ui.cams[0].macro_ - 1) as u8;
    let vals: Vec<f64> = (0..8).map(|i| { ui.advance(i as f64 * 0.11, &mut |_| {}); ui.macro_value(Part::Reader(reader), slot, ui.clock.sim_t).unwrap() }).collect();
    assert!(vals.iter().cloned().fold(f64::MIN, f64::max) - vals.iter().cloned().fold(f64::MAX, f64::min) > 0.05, "{vals:?}");
    println!("cam macro over time: {:?}", vals.iter().map(|v| (v * 100.).round() / 100.).collect::<Vec<_>>());

    // the audio's word on the routed knobs: only while watched, moving while a note sounds
    let mut seen: Vec<Vec<f64>> = Vec::new();
    let run = |a: &mut AudioSide, n: usize, seen: &mut Vec<Vec<f64>>| {
        for k in 0..n {
            a.block(k as f64 * 128. / 48000.);
            if let Some(s) = a.outbox() {
                let v: Value = serde_json::from_str(s).unwrap();
                if let Some(m) = v["mods"].as_array() { seen.push(m.iter().map(|x| x.as_f64().unwrap()).collect()) }
                a.clear_outbox();
            }
        }
    };
    run(&mut a, 200, &mut seen);
    a.message(&format!(r#"{{"type":"audition","part":{reader},"midi":60,"key":77}}"#));
    run(&mut a, 200, &mut seen);
    assert!(seen.is_empty(), "nothing is reported until the editor watches");
    a.message(&format!(r#"{{"type":"watch","part":{reader}}}"#));
    a.message(&format!(r#"{{"type":"audition","part":{reader},"midi":60,"key":78}}"#));
    run(&mut a, 2000, &mut seen);
    let cut = param_index("osc1.cut").unwrap() as f64;
    let at: Vec<f64> = seen.iter().filter_map(|m| m.chunks(2).find(|c| c[0] == cut).map(|c| c[1])).collect();
    let spread = at.iter().cloned().fold(f64::MIN, f64::max) - at.iter().cloned().fold(f64::MAX, f64::min);
    assert!(at.len() > 20 && spread > 0.01, "{} reports, spread {spread}", at.len());
    println!("LFO route: {} reports, the knob moved over {:.3} of its travel", at.len(), spread);
    a.message(r#"{"type":"watch"}"#);
    run(&mut a, 20, &mut Vec::new());
    seen.clear();
    run(&mut a, 200, &mut seen);
    assert!(seen.is_empty(), "stopped watching: no more reports");
    println!("ok");
}
