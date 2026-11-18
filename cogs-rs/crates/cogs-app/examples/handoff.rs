//! Moving the machine from a slow output to a fast one while it plays, on simulated outputs: what reaches the speaker
//! must be one continuous signal, with every source frame used once.
//!   cargo run -p cogs-app --release --example handoff
use cogs::out::{Out, Source};
use std::f64::consts::TAU;
const SR: f64 = 48000.;
struct Tone { frame: u64 }
impl Source for Tone {
    fn block(&mut self, l: &mut [f32], r: &mut [f32], _latency: f64) {
        for i in 0..l.len() { let x = (TAU * 110. * self.frame as f64 / SR).sin() as f32 * 0.5; l[i] = x; r[i] = x; self.frame += 1 }
    }
}
struct Sim { out: Out<Tone>, burst: usize, delay: usize, written: usize, start: usize }
fn main() {
    for (err_ms, a_burst, a_delay, b_burst, b_delay) in [(0., 960, 4800, 192, 1152), (0., 960, 4800, 96, 576), (2., 960, 4800, 192, 1152), (-2., 960, 4800, 192, 1152)] {
        let total = 48000 * 3;
        let mut heard = vec![0f32; total + 20000];
        let mut a = Sim { out: Out::new(SR), burst: a_burst, delay: a_delay, written: 0, start: 0 };
        a.out.src = Some(Box::new(Tone { frame: 0 }));
        let mut b: Option<Sim> = None;
        for now in 0..total {
            if now == 24000 { let mut o = Out::new(SR); o.incoming = Some(a.out.handoff.clone()); b = Some(Sim { out: o, burst: b_burst, delay: b_delay, written: 0, start: now }) }
            if now == 24000 + 4800 { a.out.handoff.request() }
            for (s, skew) in [(Some(&mut a), 0.), (b.as_mut(), err_ms / 1000.)] {
                let Some(s) = s else { continue };
                // pulled a burst at a time, as a device does; `delay` frames from pull to speaker
                if now >= s.start + s.written {
                    let at = s.start + s.written + s.delay;
                    let h = &mut heard;
                    s.out.render(s.burst, s.delay as f64 / SR + skew, (now as f64 / SR * 1e9) as u64, |i, l, _| h[at + i] += l);
                    s.written += s.burst;
                }
            }
        }
        let b = b.unwrap();
        let used = b.out.src.as_ref().map_or(0, |t| t.frame);
        // from a settled point to the end of what was played: the tone's envelope and its largest step
        let (from, to) = (12000, total);
        let w = (SR / 110.).ceil() as usize;
        let (mut lo, mut hi, mut step) = (f32::MAX, 0f32, 0f32);
        let mut i = from;
        while i + w < to { let p = heard[i..i + w].iter().fold(0f32, |m, x| m.max(x.abs())); lo = lo.min(p); hi = hi.max(p); i += w / 4 }
        for i in from + 1..to { step = step.max((heard[i] - heard[i - 1]).abs()) }
        let plain = (TAU * 110. / SR).sin() as f32 * 0.5;
        // where the last source frame lands against where it would have with no change of output, in ms
        let lag = ((b.start + b.written + b.delay) as f64 - used as f64 - a_delay as f64) / SR * 1000.;
        println!("old {a_burst}/{a_delay} new {b_burst}/{b_delay} clock error {err_ms:+} ms: pad {} frames, peak per cycle {lo:.3}..{hi:.3} (0.5 is the tone), largest step {step:.4} (tone's own {plain:.4}), musical time moved {lag:+.1} ms", b.out.last_pad);
    }
}
