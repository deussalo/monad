//! The climax and the reverb alone on a fixed signal, to compare with the same JS classes (levels printed).
use cogs_dsp::{climax::Climax, verb::Verb, ClimaxParams, VerbParams};
fn stat(name: &str, l: &[f32], r: &[f32]) {
    let pk = l.iter().chain(r).fold(0f32, |a, x| a.max(x.abs()));
    let rms = (l.iter().chain(r).map(|x| (*x as f64).powi(2)).sum::<f64>() / (2 * l.len()) as f64).sqrt();
    println!("{name:<22} peak {pk:.4}  rms {rms:.5} ({:.2} dB)", 20. * rms.log10());
}
fn main() {
    let fs = 48000.; let n = 48000 * 3;
    let sig = |i: usize| { let t = i as f64 / fs; let e = if t < 1. { 1. } else { 0. }; (e * (0.3 * (6.2832 * 110. * t).sin() + 0.2 * (6.2832 * 440. * t).sin() + 0.1 * (6.2832 * 3000. * t).sin())) as f32 };
    for (name, set) in [("climax-default", vec![]), ("climax-hot", vec![("drive", 12.), ("sat", 150.), ("excite", 10.), ("air", 10.), ("low", 10.), ("high", 10.)])] {
        let mut c = Climax::new(fs); let mut p = ClimaxParams::default(); for (k, x) in set { p.set(k, x); } c.set(&p);
        let (mut l, mut r): (Vec<f32>, Vec<f32>) = ((0..n).map(sig).collect(), (0..n).map(sig).collect());
        for b in (0..n).step_by(128) { let e = (b + 128).min(n); c.run(&mut l[b..e], &mut r[b..e]); }
        stat(name, &l, &r);
    }
    for (mode, color) in [(0., 1.), (2., 2.), (5., 0.), (14., 1.)] {
        let mut p = VerbParams::default(); p.0[0] = mode; p.0[1] = color; p.0[2] = 1.;
        let mut v = Verb::new(fs, &p);
        // a different stretch of the random modulation each run, as Math.random gives the JS
        let warm: usize = std::env::args().nth(1).and_then(|a| a.parse().ok()).unwrap_or(0);
        { let z = vec![0f32; 128]; let (mut a, mut b) = (vec![0f32; 128], vec![0f32; 128]); for _ in 0..warm { v.process(&z, &z, &mut a, &mut b, 128); } }
        let (mut l, mut r) = (vec![0f32; n], vec![0f32; n]);
        let inl: Vec<f32> = (0..n).map(|i| if i < 4800 { sig(i) } else { 0. }).collect();
        for b in (0..n).step_by(128) { let e = (b + 128).min(n); v.process(&inl[b..e], &inl[b..e], &mut l[b..e], &mut r[b..e], e - b); }
        stat(&format!("verb-m{mode}-c{color}"), &l, &r);
    }
}
