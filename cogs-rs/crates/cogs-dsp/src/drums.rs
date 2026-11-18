//! The drums: in the HTML build each hit is a WebAudio graph on the drum bus (`drum()` in sim.js): tuned oscillators
//! and filtered noise under one exponentially decaying gain, an optional darkening low-pass and an equal-power panner.
//! Here each hit is a voice from a pool that renders exactly that graph: band-limited oscillators with their
//! exponential sweeps, looped white noise through WebAudio's biquads, and the same gain automation.

use crate::{Aux, Rng, Sends, FIELD_SLOTS};
use serde_json::Value;
use std::f64::consts::{PI, TAU};

pub const DRUM_KINDS: [&str; 10] = ["kick", "snare", "hat", "tom", "clap", "rim", "cowbell", "wood", "cymbal", "unused"];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrumKind { Kick, Snare, Hat, Tom, Clap, Rim, Cowbell, Wood, Cymbal }
impl DrumKind {
    pub fn from_name(s: &str) -> Option<Self> {
        use DrumKind::*;
        Some(match s { "kick" => Kick, "snare" => Snare, "hat" => Hat, "tom" => Tom, "clap" => Clap, "rim" => Rim, "cowbell" => Cowbell, "wood" => Wood, "cymbal" => Cymbal, _ => return None })
    }
    pub fn name(self) -> &'static str { DRUM_KINDS[self as usize] }
    /// [key, label, min, max, step]: VOICE_DEFS for this kind.
    pub fn defs(self) -> &'static [(&'static str, &'static str, f64, f64, f64)] {
        use DrumKind::*;
        match self {
            Kick => &[("tune", "tune Hz", 20., 200., 1.), ("sweep", "sweep x", 1., 10., 0.1), ("pdecay", "p decay s", 0.005, 0.3, 0.001), ("decay", "decay s", 0.02, 2., 0.01)],
            Snare => &[("tune", "tune Hz", 60., 600., 1.), ("tone", "tone", 0., 1., 0.01), ("cutoff", "cutoff Hz", 200., 12000., 10.), ("decay", "decay s", 0.02, 1., 0.005)],
            Hat => &[("cutoff", "cutoff Hz", 1000., 16000., 10.), ("decay", "decay s", 0.005, 1., 0.005)],
            Tom => &[("tune", "tune Hz", 40., 400., 1.), ("sweep", "sweep x", 1., 4., 0.1), ("pdecay", "p decay s", 0.005, 0.3, 0.001), ("tone", "tone", 0., 1., 0.01), ("decay", "decay s", 0.02, 2., 0.01)],
            Clap => &[("cutoff", "cutoff Hz", 300., 6000., 10.), ("spread", "spread s", 0.003, 0.04, 0.001), ("decay", "decay s", 0.05, 1., 0.005)],
            Rim => &[("tune", "tune Hz", 400., 4000., 10.), ("decay", "decay s", 0.01, 0.5, 0.005)],
            Cowbell => &[("tune", "tune Hz", 200., 1200., 1.), ("decay", "decay s", 0.02, 2., 0.01)],
            Wood => &[("tune", "tune Hz", 500., 5000., 10.), ("decay", "decay s", 0.01, 0.5, 0.005)],
            Cymbal => &[("cutoff", "cutoff Hz", 1000., 12000., 10.), ("tone", "tone", 0., 1., 0.01), ("decay", "decay s", 0.1, 4., 0.01)],
        }
    }
}

/// A drum part's knobs; a kind uses those its VOICE_DEFS name.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrumVoice { pub kind: DrumKind, pub tune: f64, pub sweep: f64, pub pdecay: f64, pub decay: f64, pub tone: f64, pub cutoff: f64, pub spread: f64 }
impl DrumVoice {
    /// The kind's defaults (VOICES in sim.js).
    pub fn new(kind: DrumKind) -> Self {
        use DrumKind::*;
        let z = DrumVoice { kind, tune: 0., sweep: 1., pdecay: 0.05, decay: 0.3, tone: 0., cutoff: 1000., spread: 0.011 };
        match kind {
            Kick => DrumVoice { tune: 52., sweep: 3., pdecay: 0.045, decay: 0.35, ..z },
            Snare => DrumVoice { tune: 190., tone: 0.35, cutoff: 1800., decay: 0.16, ..z },
            Hat => DrumVoice { cutoff: 7500., decay: 0.045, ..z },
            Tom => DrumVoice { tune: 110., sweep: 1.6, pdecay: 0.08, tone: 0.15, decay: 0.4, ..z },
            Clap => DrumVoice { cutoff: 1300., spread: 0.011, decay: 0.25, ..z },
            Rim => DrumVoice { tune: 1700., decay: 0.06, ..z },
            Cowbell => DrumVoice { tune: 560., decay: 0.3, ..z },
            Wood => DrumVoice { tune: 2200., decay: 0.04, ..z },
            Cymbal => DrumVoice { cutoff: 5500., tone: 0.4, decay: 1.4, ..z },
        }
    }
    /// A save's drum voice (`kind` and its knobs), clamped to the knobs' ranges; None if the kind is not a drum.
    pub fn from_json(j: &Value) -> Option<Self> {
        let kind = DrumKind::from_name(j.get("kind")?.as_str()?)?;
        let mut v = Self::new(kind);
        for &(k, _, lo, hi, _) in kind.defs() {
            let Some(x) = j.get(k).map(crate::patch::js_num).filter(|x| x.is_finite()) else { continue };
            let x = x.clamp(lo, hi);
            match k { "tune" => v.tune = x, "sweep" => v.sweep = x, "pdecay" => v.pdecay = x, "decay" => v.decay = x, "tone" => v.tone = x, "cutoff" => v.cutoff = x, _ => v.spread = x }
        }
        Some(v)
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Wave { Sine, Tri, Square }
#[derive(Clone, Copy, PartialEq)]
enum Filt { Bp, Hp }
#[derive(Clone, Copy)]
enum Src {
    Off,
    Osc { wave: Wave, f0: f64, f1: f64, sweep: f64, gain: f64, at: f64, stop: f64, ph: f64 },
    Hiss { gain: f64, at: f64, to: f64, stop: f64, c: [f64; 5], z: [f64; 4] },
}

#[derive(Clone, Copy)]
struct Hit {
    on: bool, at: f64, end: f64, stop: f64, amp: f64, gl: f64, gr: f64, dark: Option<([f64; 5], [f64; 4])>, src: [Src; 5], sends: Sends,
}

/// WebAudio's biquads (Chrome's): bandpass Q is linear; highpass and lowpass Q is resonance in dB.
fn wa_biquad(kind: u8, f: f64, q: f64, fs: f64) -> [f64; 5] {
    let nf = f / (fs * 0.5);
    let w = PI * nf.clamp(0., 1.); let (cs, sn) = (w.cos(), w.sin());
    let (b0, b1, b2, a0, a1, a2);
    match kind {
        0 | 1 if nf >= 1. => return if kind == 0 { [1., 0., 0., 0., 0.] } else { [0., 0., 0., 0., 0.] },
        0 | 1 => {
            let g = 10f64.powf(-0.05 * q); let al = 0.5 * sn * g;
            if kind == 0 { b0 = (1. - cs) / 2.; b1 = 1. - cs; b2 = b0; } else { b0 = (1. + cs) / 2.; b1 = -(1. + cs); b2 = b0; }
            a0 = 1. + al; a1 = -2. * cs; a2 = 1. - al;
        }
        _ => {
            if nf >= 1. || nf <= 0. { return [0.; 5]; }
            let al = sn / (2. * q.max(1e-4));
            b0 = al; b1 = 0.; b2 = -al; a0 = 1. + al; a1 = -2. * cs; a2 = 1. - al;
        }
    }
    [b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0]
}
#[inline]
fn bq(c: &[f64; 5], z: &mut [f64; 4], x: f64) -> f64 { let y = c[0] * x + c[1] * z[0] + c[2] * z[1] - c[3] * z[2] - c[4] * z[3]; z[1] = z[0]; z[0] = x; z[3] = z[2]; z[2] = y; y }
/// exponentialRampToValueAtTime from v0 at t0 to v1 at t1, read at t (held at v1 after)
#[inline]
fn ramp(v0: f64, v1: f64, t0: f64, t1: f64, t: f64) -> f64 { if t >= t1 { v1 } else if t <= t0 { v0 } else { v0 * (v1 / v0).powf((t - t0) / (t1 - t0)) } }
#[inline]
fn blep(t: f64, dt: f64) -> f64 {
    if t < dt { let x = t / dt; x + x - x * x - 1. } else if t > 1. - dt { let x = (t - 1.) / dt; x * x + x + x + 1. } else { 0. }
}
/// Chrome normalises its built-in square to a peak of 1, Gibbs overshoot included.
const SQUARE_NORM: f64 = 0.85;

pub struct Drums { fs: f64, hits: Vec<Hit>, buf: Vec<f64>, rng: Rng, pub dropped: u32 }
impl Drums {
    pub fn new(fs: f64, voices: usize, max_block: usize) -> Self {
        Drums { fs, hits: vec![Hit { on: false, at: 0., end: 0., stop: 0., amp: 0., gl: 0., gr: 0., dark: None, src: [Src::Off; 5], sends: Sends::default() }; voices],
            buf: vec![0.; max_block], rng: Rng::new(0xd0d0_cafe), dropped: 0 }
    }
    pub fn panic(&mut self) { for h in &mut self.hits { h.on = false; } }
    pub fn active(&self) -> usize { self.hits.iter().filter(|h| h.on).count() }
    /// A hit at frame `at`: the graph drum() builds, as a voice.
    pub fn hit(&mut self, at: u64, v: &DrumVoice, amp: f64, pan: f64, dark: f64, sends: Sends) {
        let fs = self.fs;
        let i = match self.hits.iter().position(|h| !h.on) {
            Some(i) => i,
            None => { self.dropped += 1; self.hits.iter().enumerate().min_by(|a, b| a.1.at.total_cmp(&b.1.at)).map(|x| x.0).unwrap_or(0) }
        };
        let w = at as f64 / fs; let end = w + v.decay;
        let x = (pan.clamp(-1., 1.) + 1.) / 2.;
        let mut h = Hit { on: true, at: w, end, stop: 0., amp, gl: (x * PI / 2.).cos(), gr: (x * PI / 2.).sin(), dark: None, src: [Src::Off; 5], sends };
        if dark > 0.02 { h.dark = Some((wa_biquad(0, 16000. * (-dark * 3.2).exp(), 1., fs), [0.; 4])); }
        let mut n = 0;
        let mut osc = |h: &mut Hit, wave, f0: f64, gain, f1: f64, sweep: f64| { h.src[n] = Src::Osc { wave, f0, f1, sweep, gain, at: w, stop: end + 0.02, ph: 0. }; n += 1; };
        use DrumKind::*;
        let mut hisses: [(Filt, f64, f64, f64, f64); 4] = [(Filt::Bp, 0., 0., 0., 0.); 4];
        let mut nh = 0;
        let mut hiss = |filt, cutoff, gain, a: f64, to: f64| { hisses[nh] = (filt, cutoff, gain, a, to); nh += 1; };
        match v.kind {
            Kick => osc(&mut h, Wave::Sine, v.tune * v.sweep, 1., v.tune, v.pdecay),
            Snare => { hiss(Filt::Bp, v.cutoff, 1. - v.tone, w, end); osc(&mut h, Wave::Tri, v.tune, v.tone, v.tune, 0.); }
            Hat => hiss(Filt::Hp, v.cutoff, 1., w, end),
            Tom => { osc(&mut h, Wave::Sine, v.tune * v.sweep, 1. - v.tone, v.tune, v.pdecay); hiss(Filt::Bp, v.tune * 8., v.tone, w, w + 0.03); }
            Clap => { for i in 0..3 { let a = w + i as f64 * v.spread; hiss(Filt::Bp, v.cutoff, 0.8, a, a + v.spread * 0.9); } hiss(Filt::Bp, v.cutoff, 0.6, w + 3. * v.spread, end); }
            Rim => { osc(&mut h, Wave::Tri, v.tune, 0.6, v.tune, 0.); osc(&mut h, Wave::Square, v.tune * 1.78, 0.3, v.tune * 1.78, 0.); hiss(Filt::Hp, v.tune * 2., 0.5, w, w + 0.008); }
            Cowbell => { osc(&mut h, Wave::Square, v.tune, 0.5, v.tune, 0.); osc(&mut h, Wave::Square, v.tune * 1.48, 0.5, v.tune * 1.48, 0.); }
            Wood => osc(&mut h, Wave::Sine, v.tune, 1., v.tune * 0.9, v.decay),
            Cymbal => { hiss(Filt::Hp, v.cutoff, 1. - v.tone, w, end); for r in [1., 1.34, 1.71, 2.26] { osc(&mut h, Wave::Square, v.cutoff * 0.11 * r, v.tone * 0.18, v.cutoff * 0.11 * r, 0.); } }
        }
        for &(filt, cutoff, gain, a, to) in &hisses[..nh] {
            let c = if filt == Filt::Bp { wa_biquad(2, cutoff, 0.8, fs) } else { wa_biquad(1, cutoff, 0.8, fs) };
            h.src[n] = Src::Hiss { gain, at: a, to, stop: to + 0.02, c, z: [0.; 4] }; n += 1;
        }
        h.stop = h.src.iter().map(|s| match s { Src::Osc { stop, .. } | Src::Hiss { stop, .. } => *stop, Src::Off => 0. }).fold(0., f64::max);
        self.hits[i] = h;
    }
    /// One block of every sounding hit into dry (added, at each hit's dry level) and the field slots by its sends.
    pub(crate) fn process(&mut self, frame: u64, n: usize, dry_l: &mut [f32], dry_r: &mut [f32], aux: &mut Aux) {
        let fs = self.fs; let dt = 1. / fs;
        for h in self.hits.iter_mut() {
            if !h.on { continue; }
            let t0 = frame as f64 / fs;
            if t0 > h.stop { h.on = false; continue; }
            if t0 + n as f64 * dt <= h.at { continue; }
            let b = &mut self.buf[..n];
            b.fill(0.);
            for src in h.src.iter_mut() {
                match src {
                    Src::Off => {}
                    Src::Osc { wave, f0, f1, sweep, gain, at, stop, ph } => {
                        for s in 0..n {
                            let t = t0 + s as f64 * dt;
                            if t < *at || t >= *stop { continue; }
                            let f = if f0 != f1 && *sweep > 0. { ramp(*f0, *f1, *at, *at + *sweep, t) } else { *f1 };
                            let inc = f / fs;
                            let y = match wave {
                                Wave::Sine => (TAU * *ph).sin(),
                                Wave::Tri => { let p = *ph; if p < 0.25 { 4. * p } else if p < 0.75 { 2. - 4. * p } else { 4. * p - 4. } }
                                Wave::Square => {
                                    let p = *ph; let mut y = if p < 0.5 { 1. } else { -1. };
                                    y += blep(p, inc); let q = (p + 0.5) % 1.; y -= blep(q, inc); y * SQUARE_NORM
                                }
                            };
                            b[s] += y * *gain;
                            *ph += inc; if *ph >= 1. { *ph -= 1.; }
                        }
                    }
                    Src::Hiss { gain, at, to, stop, c, z, .. } => {
                        let ramped = *to < h.end;
                        for s in 0..n {
                            let t = t0 + s as f64 * dt;
                            if t < *at || t >= *stop { continue; }
                            let x = self.rng.next() * 2. - 1.;
                            let y = bq(c, z, x);
                            let g = if ramped { ramp(*gain, 1e-4, *at, *to, t) } else { *gain };
                            b[s] += y * g;
                        }
                    }
                }
            }
            // the hit's gain: amp at its start, an exponential ramp to 1e-4 at its end; then dark, then the panner
            let sd = h.sends;
            for s in 0..n {
                let t = t0 + s as f64 * dt;
                if t < h.at { b[s] = 0.; continue; }
                let mut y = b[s] * ramp(h.amp.max(1e-9), 1e-4, h.at, h.end, t);
                if let Some((c, z)) = &mut h.dark { y = bq(c, z, y); }
                b[s] = y;
                let (l, r) = ((y * h.gl) as f32, (y * h.gr) as f32);
                dry_l[s] += l * sd.dry; dry_r[s] += r * sd.dry;
            }
            if sd.any() {
                for k in 0..FIELD_SLOTS {
                    let a = sd.amt[k]; if a == 0. { continue; }
                    let (al, ar) = aux.take(k);
                    for s in 0..n { al[s] += (b[s] * h.gl) as f32 * a; ar[s] += (b[s] * h.gr) as f32 * a; }
                }
            }
        }
    }
}
