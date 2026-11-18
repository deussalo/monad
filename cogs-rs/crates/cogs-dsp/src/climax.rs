//! The master's line amp (climax.js), after Pulsar Modular's P42 Climax, in spirit: an input drive into a transformer
//! whose core saturates the lows, a Class A stage that adds even harmonics, a tube-like saturation, boost-only low and
//! high shelves, and the Air band with an exciter beside it. The drive is compensated after the nonlinearities, so
//! turning it changes the colour and not the level. Everything nonlinear and the mix run at twice the sample rate.

use std::f64::consts::PI;

/// [key, label, min, max, step]
pub const CLIMAX_DEFS: [(&str, &str, f64, f64, f64); 13] = [("on", "line amp", 0., 1., 1.), ("drive", "drive dB", -6., 12., 0.1), ("sat", "saturation %", 0., 150., 1.),
    ("classA", "class A", 0., 1., 1.), ("excite", "excite", 0., 10., 0.1), ("air", "air", 0., 10., 0.1), ("airFreq", "air Hz", 5000., 16000., 100.),
    ("low", "low boost", 0., 10., 0.1), ("lowFreq", "low Hz", 40., 500., 1.), ("high", "high boost", 0., 10., 0.1), ("highFreq", "high Hz", 150., 5000., 10.),
    ("mix", "mix", 0., 1., 0.01), ("out", "out dB", -24., 6., 0.1)];
pub const CLIMAX_DEFAULT: [f64; 13] = [1., 2., 30., 1., 3., 2., 10000., 0., 80., 0., 3000., 1., 0.];
const ON: usize = 0; const DRIVE: usize = 1; const SAT: usize = 2; const CLASSA: usize = 3; const EXCITE: usize = 4; const AIR: usize = 5; const AIRF: usize = 6;
const LOW: usize = 7; const LOWF: usize = 8; const HIGH: usize = 9; const HIGHF: usize = 10; const MIX: usize = 11; const OUT: usize = 12;

/// The line amp's knobs, by CLIMAX_DEFS index.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClimaxParams(pub [f64; 13]);
impl Default for ClimaxParams { fn default() -> Self { ClimaxParams(CLIMAX_DEFAULT) } }
impl ClimaxParams {
    pub fn set(&mut self, key: &str, x: f64) { if let Some(i) = CLIMAX_DEFS.iter().position(|d| d.0 == key) { if x.is_finite() { self.0[i] = x.clamp(CLIMAX_DEFS[i].2, CLIMAX_DEFS[i].3); } } }
    /// climaxIn(j): a saved line amp, clamped to its ranges; a world from before it takes the defaults.
    pub fn from_json(j: &serde_json::Value) -> Self {
        let mut p = Self::default();
        if let Some(o) = j.as_object() { for (k, v) in o { p.set(k, crate::patch::js_num(v)); } }
        p
    }
}

/// RBJ biquads, transposed direct form II; state per channel.
#[derive(Clone, Copy, Default)]
pub(crate) struct Biquad { b0: f64, b1: f64, b2: f64, a1: f64, a2: f64, z: [f64; 4] }
#[derive(Clone, Copy)]
pub(crate) enum Bq { Lp, Hp, Ls, Hs }
impl Biquad {
    pub fn new() -> Self { Biquad { b0: 1., ..Default::default() } }
    pub fn set(&mut self, ty: Bq, fs: f64, f: f64, q: f64, db: f64) {
        let w = 2. * PI * f.min(0.49 * fs) / fs; let (c, s) = (w.cos(), w.sin()); let a = 10f64.powf(db / 40.);
        let (b0, b1, b2, a0, a1, a2);
        match ty {
            Bq::Lp => { let al = s / (2. * q); b0 = (1. - c) / 2.; b1 = 1. - c; b2 = b0; a0 = 1. + al; a1 = -2. * c; a2 = 1. - al; }
            Bq::Hp => { let al = s / (2. * q); b0 = (1. + c) / 2.; b1 = -(1. + c); b2 = b0; a0 = 1. + al; a1 = -2. * c; a2 = 1. - al; }
            _ => {
                let al = s / 2. * ((a + 1. / a) * (1. / q - 1.) + 2.).sqrt(); let sq = 2. * a.sqrt() * al;
                if let Bq::Ls = ty {
                    b0 = a * ((a + 1.) - (a - 1.) * c + sq); b1 = 2. * a * ((a - 1.) - (a + 1.) * c); b2 = a * ((a + 1.) - (a - 1.) * c - sq);
                    a0 = (a + 1.) + (a - 1.) * c + sq; a1 = -2. * ((a - 1.) + (a + 1.) * c); a2 = (a + 1.) + (a - 1.) * c - sq;
                } else {
                    b0 = a * ((a + 1.) + (a - 1.) * c + sq); b1 = -2. * a * ((a - 1.) + (a + 1.) * c); b2 = a * ((a + 1.) + (a - 1.) * c - sq);
                    a0 = (a + 1.) - (a - 1.) * c + sq; a1 = 2. * ((a - 1.) - (a + 1.) * c); a2 = (a + 1.) - (a - 1.) * c - sq;
                }
            }
        }
        self.b0 = b0 / a0; self.b1 = b1 / a0; self.b2 = b2 / a0; self.a1 = a1 / a0; self.a2 = a2 / a0;
    }
    #[inline]
    pub fn run(&mut self, x: f64, ch: usize) -> f64 {
        let i = ch * 2; let y = self.b0 * x + self.z[i];
        self.z[i] = self.b1 * x - self.a1 * y + self.z[i + 1]; self.z[i + 1] = self.b2 * x - self.a2 * y; y
    }
}
const SHELF: f64 = 1.;
/// tanh, as a rational: within 2% and a fraction of the cost, flat at ±1 past 3.
#[inline]
fn ctanh(x: f64) -> f64 { if x > 3. { 1. } else if x < -3. { -1. } else { x * (27. + x * x) / (27. + 9. * x * x) } }

pub struct Climax {
    f2: f64, up: [Biquad; 2], dn: [Biquad; 2], low_s: Biquad, high_s: Biquad, air_s: Biquad, ex1: Biquad, ex2: Biquad, ex3: Biquad,
    tf_a: f64, tf: [f64; 2], env_a: f64, env_r: f64, env: [f64; 2], dc_a: f64, dc: [f64; 2], gc_a: f64, ms_in: f64, ms_out: f64, gc: f64,
    p: [f64; 13], cur: [f64; 13], started: bool, byp: f64,
}
impl Climax {
    pub fn new(fs: f64) -> Self {
        let f2 = fs * 2.;
        let mut up = [Biquad::new(); 2]; let mut dn = [Biquad::new(); 2];
        // 2x: a 4th-order Butterworth low-pass at 0.45 of the base rate, each way
        for (k, q) in [(0, 0.5412), (1, 1.3066)] { up[k].set(Bq::Lp, f2, 0.45 * fs, q, 0.); dn[k].set(Bq::Lp, f2, 0.45 * fs, q, 0.); }
        Climax { f2, up, dn, low_s: Biquad::new(), high_s: Biquad::new(), air_s: Biquad::new(), ex1: Biquad::new(), ex2: Biquad::new(), ex3: Biquad::new(),
            tf_a: 1. - (-2. * PI * 150. / f2).exp(), tf: [0.; 2], env_a: 1. - (-1. / (0.004 * f2)).exp(), env_r: 1. - (-1. / (0.06 * f2)).exp(), env: [0.; 2],
            dc_a: 1. - (-2. * PI * 8. / fs).exp(), dc: [0.; 2], gc_a: 1. - (-1. / (0.3 * f2)).exp(), ms_in: 1e-9, ms_out: 1e-9, gc: 1.,
            p: CLIMAX_DEFAULT, cur: [0.; 13], started: false, byp: 1. }
    }
    pub fn set(&mut self, p: &ClimaxParams) { for i in 0..13 { if p.0[i].is_finite() { self.p[i] = p.0[i].clamp(CLIMAX_DEFS[i].2, CLIMAX_DEFS[i].3); } } }
    /// Gains glide a block at a time toward their targets; the filters follow the same glide.
    fn glide(&mut self, n: usize) {
        let p = &self.p;
        let mut t = *p; t[DRIVE] = 10f64.powf(p[DRIVE] / 20.); t[SAT] = p[SAT] / 100.; t[OUT] = 10f64.powf(p[OUT] / 20.); t[ON] = if p[ON] != 0. { 1. } else { 0. };
        let first = !self.started;
        if first { self.started = true; self.cur = t; self.byp = t[ON]; }
        let k = 1. - 0.75f64.powf(n as f64 / 128.);
        let mut moved = false;
        for i in 0..13 { let d = t[i] - self.cur[i]; if d.abs() > 1e-6 { self.cur[i] += if d.abs() < 1e-4 { d } else { d * k }; moved = true; } }
        if moved || first {
            let (f2, c) = (self.f2, self.cur);
            // boost-only 0..10 as 0..+12 dB for the shelves, 0..+9 dB for Air
            self.low_s.set(Bq::Ls, f2, c[LOWF], SHELF, c[LOW] * 1.2); self.high_s.set(Bq::Hs, f2, c[HIGHF], SHELF, c[HIGH] * 1.2);
            self.air_s.set(Bq::Hs, f2, c[AIRF], SHELF, c[AIR] * 0.9);
            self.ex1.set(Bq::Hp, f2, c[AIRF] * 0.5, 0.707, 0.); self.ex2.set(Bq::Hp, f2, c[AIRF] * 0.5, 0.707, 0.); self.ex3.set(Bq::Hp, f2, c[AIRF] * 0.7, 0.707, 0.);
        }
    }
    /// One sample of the wet path at 2x, channel ch.
    #[inline]
    fn wet(&mut self, x: f64, ch: usize) -> f64 {
        let c = &self.cur;
        let g = c[DRIVE]; let x0 = x; let mut y = x * g;
        // the transformer: its low band saturates first and hardest
        self.tf[ch] += (y - self.tf[ch]) * self.tf_a; let lo = self.tf[ch];
        y += ctanh(lo * 1.4) / 1.4 - lo;
        // Class A: a little second harmonic, more as it is driven
        if c[CLASSA] > 0.001 { y += c[CLASSA] * 0.12 * y * y / (1. + 0.25 * y * y); }
        // saturation: tanh(k y)/k is unity when quiet and rounds the peaks
        if c[SAT] > 0.001 { let k = c[SAT] * 2.2; y = ctanh(k * y) / k; }
        y /= g;
        // matched in level to what came in, so drive and saturation change the colour and not the loudness
        self.ms_in += (x0 * x0 - self.ms_in) * self.gc_a; self.ms_out += (y * y - self.ms_out) * self.gc_a; y *= self.gc;
        if c[LOW] > 0.01 { y = self.low_s.run(y, ch); }
        if c[HIGH] > 0.01 { y = self.high_s.run(y, ch); }
        // the exciter: the band above Air, at unit level against its own envelope, bent, then only what is new kept
        if c[EXCITE] > 0.001 {
            let u = self.ex1.run(y, ch); let h = self.ex2.run(u, ch); let a = h.abs();
            self.env[ch] += (a - self.env[ch]) * if a > self.env[ch] { self.env_a } else { self.env_r };
            let u = (h / (self.env[ch] + 1e-5)).clamp(-4., 4.);
            let bent = (ctanh(1.8 * u) / 1.8 - u + 0.3 * u * u) * self.env[ch];
            y += self.ex3.run(bent, ch) * c[EXCITE] * 0.25;
        }
        if c[AIR] > 0.01 { self.air_s.run(y, ch) } else { y }
    }
    /// In place, on a stereo block.
    pub fn run(&mut self, l: &mut [f32], r: &mut [f32]) {
        let n = l.len();
        self.glide(n);
        let on = self.cur[ON];
        if on < 0.001 && self.byp < 0.001 { return; }
        let (mix, out) = (self.cur[MIX], self.cur[OUT]);
        for s in 0..n {
            self.byp += (on - self.byp) * 0.004; let b = self.byp;
            if s & 15 == 0 { let m = if self.ms_in > 1e-8 { (self.ms_in / self.ms_out.max(1e-12)).sqrt() } else { 1. }; self.gc += (m.clamp(0.25, 4.) - self.gc) * 0.05; }
            for ch in 0..2 {
                let x = if ch == 1 { r[s] } else { l[s] } as f64;
                let mut o = 0.;
                for k in 0..2 {
                    let u = self.up[0].run(if k == 1 { 0. } else { 2. * x }, ch); let v = self.up[1].run(u, ch);
                    let m = v + (self.wet(v, ch) - v) * mix;
                    let u = self.dn[0].run(m, ch); let d = self.dn[1].run(u, ch); if k == 0 { o = d; }
                }
                // the DC the even harmonics leave, taken off
                self.dc[ch] += (o - self.dc[ch]) * self.dc_a; o = (o - self.dc[ch]) * out;
                let y = (x + (o - x) * b) as f32;
                if ch == 1 { r[s] = y } else { l[s] = y }
            }
        }
    }
}
