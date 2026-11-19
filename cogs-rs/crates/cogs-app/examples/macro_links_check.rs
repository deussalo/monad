//! One cam or probe turning many synth macros, through the code the app runs:
//!   cargo run -p cogs-app --release --example macro_links_check
use cogs_dsp::WavePatch;
use cogs_sim::{MotionFrom, Part, World, MACRO_LINKS};
use serde_json::{json, Value};

fn main() {
    let mut j: Value = serde_json::from_str(include_str!("../../../../cogs/src/default-world.json")).unwrap();
    let mut v = WavePatch::preset("pluck").unwrap().to_json();
    v["kind"] = json!("wave");
    // every reader a synth, so there are many to link
    for r in j["readers"].as_array_mut().unwrap() { r["voice"] = v.clone(); }
    let mut w = World::from_str(&j.to_string()).unwrap();
    w.set_paused(true);
    let readers: Vec<u32> = w.readers.iter().map(|r| r.id).collect();
    assert!(readers.len() >= 2, "the default world has {} readers", readers.len());
    let c = w.cogs[0].clone();
    let cam = w.add_cam(c.x, c.y).unwrap();
    let from = MotionFrom::Cam(cam);

    // linking again adds a macro rather than moving the link
    let s1 = w.add_macro_link(from, Part::Reader(readers[0])).unwrap();
    let s2 = w.add_macro_link(from, Part::Reader(readers[0])).unwrap();
    let s3 = w.add_macro_link(from, Part::Reader(readers[1])).unwrap();
    assert_ne!(s1, s2, "a second macro on the same synth");
    let mut got: Vec<(Part, u8)> = Vec::new();
    w.motions(&mut |m| if m.from == from { got.push((m.target, m.slot)) });
    println!("cam turns {got:?}");
    assert_eq!(got.len(), 3);
    assert!(got.contains(&(Part::Reader(readers[0]), (s1 - 1) as u8)) && got.contains(&(Part::Reader(readers[0]), (s2 - 1) as u8)) && got.contains(&(Part::Reader(readers[1]), (s3 - 1) as u8)));

    // saved and read back, the links stay
    let back = World::from_str(&w.to_string()).unwrap();
    let m = back.cams.iter().find(|m| m.id == cam).unwrap();
    assert_eq!((m.to.is_some() as usize) + m.more.len(), 3, "the save keeps all three");

    // a synth's 8 macros fill; past that it says no, and the cap of 64 holds across synths
    let mut n = 3;
    for &r in &readers { while w.add_macro_link(from, Part::Reader(r)).is_ok() { n += 1; } }
    let m = w.cams.iter().find(|m| m.id == cam).unwrap();
    println!("after filling: {n} links over {} synths (cap {MACRO_LINKS})", readers.len());
    assert_eq!(1 + m.more.len(), n);
    assert!(n <= MACRO_LINKS && n == (8 * readers.len()).min(MACRO_LINKS));

    // taking the first away promotes the next; a reader removed takes its links with it
    w.remove_macro_link(from, 0);
    assert!(w.cams.iter().find(|m| m.id == cam).unwrap().to.is_some());
    w.remove(cogs_sim::PartRef::Reader(readers[1]));
    let m = w.cams.iter().find(|m| m.id == cam).unwrap();
    assert!(m.more.iter().all(|l| l.to != Part::Reader(readers[1])) && m.to != Some(readers[1]));

    // a probe too
    let c = w.cogs[0].clone();
    let p = w.add_probe(c.x + c.radius() * 0.5, c.y, 1.0).expect("a probe on the first cog");
    let pf = MotionFrom::Probe(p);
    let r0 = Part::Reader(readers[0]);
    let free: Vec<i64> = (0..3).filter_map(|_| w.add_macro_link(pf, r0).ok()).collect();
    let mut got = 0;
    w.motions(&mut |m| if m.from == pf { got += 1 });
    println!("probe links {free:?}, {got} sent");
    assert_eq!(got, free.len());
    println!("ok");
}
