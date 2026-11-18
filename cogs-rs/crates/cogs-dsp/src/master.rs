//! The master chain of initAudio(): the master gain, the line amp (climax.rs), the 'distant' low-pass the listener's
//! distance sweeps, and a DynamicsCompressor as the limiter (threshold -10 dB, knee 8, ratio 3.5, attack 15 ms,
//! release 300 ms). The compressor is Chrome's (Blink's DynamicsCompressor), ported, makeup gain and 6 ms lookahead
//! included, since that is what the HTML build is heard through.

use std::f64::consts::PI;

const PRE_DELAY: f64 = 0.006;
const MAX_PRE: usize = 1024;
const RZ: [f64; 4] = [0.09, 0.16, 0.42, 0.98];
const SAT_RELEASE: f64 = 0.0025;
fn db2lin(db: f64) -> f64 { 10f64.powf(0.05 * db) }
fn lin2db(x: f64) -> f64 { 20. * x.log10() }

pub struct Compressor {
    fs: f64, threshold: f64, knee: f64, ratio: f64, attack: f64, release: f64,
    lin_threshold: f64, slope: f64, knee_threshold: f64, db_knee_threshold: f64, db_yknee_threshold: f64, k: f64,
    pre: [Vec<f64>; 2], pr: usize, pw: usize, detector: f64, gain: f64, max_attack_diff: f64,
    /// the gain reduction now, dB (≤ 0)
    pub reduction: f64, meter_k: f64,
}
impl Compressor {
    pub fn new(fs: f64, threshold: f64, knee: f64, ratio: f64, attack: f64, release: f64) -> Self {
        let pre_frames = ((PRE_DELAY * fs) as usize).min(MAX_PRE - 1);
        let mut c = Compressor { fs, threshold, knee, ratio, attack, release, lin_threshold: 0., slope: 0., knee_threshold: 0., db_knee_threshold: 0., db_yknee_threshold: 0., k: 0.,
            pre: [vec![0.; MAX_PRE], vec![0.; MAX_PRE]], pr: 0, pw: pre_frames, detector: 0., gain: 1., max_attack_diff: -1., reduction: 0.,
            meter_k: 1. - (-1. / (0.325 * fs)).exp() };
        c.curve();
        c
    }
    fn knee_curve(&self, x: f64, k: f64) -> f64 { if x < self.lin_threshold { x } else { self.lin_threshold + (1. - (-k * (x - self.lin_threshold)).exp()) / k } }
    fn saturate(&self, x: f64, k: f64) -> f64 {
        if x < self.knee_threshold { return self.knee_curve(x, k); }
        db2lin(self.db_yknee_threshold + self.slope * (lin2db(x) - self.db_knee_threshold))
    }
    fn k_at_slope(&self, desired: f64) -> f64 {
        let db_x = self.threshold + self.knee; let x = db2lin(db_x);
        let (mut x2, mut db_x2) = (1., 0.);
        if x >= self.lin_threshold { x2 = x * 1.001; db_x2 = lin2db(x2); }
        let (mut min_k, mut max_k, mut k) = (0.1f64, 10000f64, 5f64);
        let mut slope = 1.;
        for _ in 0..15 {
            if x >= self.lin_threshold { let db_y = lin2db(self.knee_curve(x, k)); let db_y2 = lin2db(self.knee_curve(x2, k)); slope = (db_y2 - db_y) / (db_x2 - db_x); }
            if slope < desired { max_k = k; } else { min_k = k; }
            k = (min_k * max_k).sqrt();
        }
        k
    }
    fn curve(&mut self) {
        self.lin_threshold = db2lin(self.threshold); self.slope = 1. / self.ratio;
        let k = self.k_at_slope(1. / self.ratio);
        self.db_knee_threshold = self.threshold + self.knee; self.knee_threshold = db2lin(self.db_knee_threshold);
        self.db_yknee_threshold = lin2db(self.knee_curve(self.knee_threshold, k));
        self.k = k;
    }
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        let n = l.len(); let k = self.k; let fs = self.fs;
        let post = (1. / self.saturate(1., k)).powf(0.6);
        let attack_frames = self.attack.max(0.001) * fs; let release_frames = fs * self.release; let sat_release_frames = SAT_RELEASE * fs;
        let base = |c: [f64; 4]| c[0] * RZ[0] + c[1] * RZ[1] + c[2] * RZ[2] + c[3] * RZ[3];
        let a = release_frames * base([0.9999999999999998, 1.8432219684323923e-16, -1.9373394351676423e-16, 8.824516011816245e-18]);
        let b = release_frames * base([-1.5788320352845888, 2.3305837032074286, -0.9141194204840429, 0.1623677525612032]);
        let c = release_frames * base([0.5334142869106424, -1.272736789213631, 0.9258856042207512, -0.18656310191776226]);
        let d = release_frames * base([0.08783463138207234, -0.1694162967925622, 0.08588057951595272, -0.00429891410546283]);
        let e = release_frames * base([-0.042416883008123074, 0.1115693827987602, -0.09764676325265872, 0.028494263462021576]);
        let mut fi = 0;
        while fi < n {
            if !self.detector.is_finite() { self.detector = 1.; }
            let desired = self.detector;
            let scaled = desired.clamp(-1., 1.).asin() / (PI / 2.);
            let releasing = scaled > self.gain;
            let mut diff = if scaled == 0. { if releasing { -1. } else { 1. } } else { lin2db(self.gain / scaled) };
            let rate;
            if releasing {
                self.max_attack_diff = -1.;
                if !diff.is_finite() { diff = -1.; }
                let x = 0.25 * (diff.clamp(-12., 0.) + 12.);
                let (x2, x3, x4) = (x * x, x * x * x, x * x * x * x);
                let frames = a + b * x + c * x2 + d * x3 + e * x4;
                rate = db2lin(5. / frames);
            } else {
                if !diff.is_finite() { diff = 1.; }
                if self.max_attack_diff == -1. || self.max_attack_diff < diff { self.max_attack_diff = diff; }
                let eff = self.max_attack_diff.max(0.5);
                rate = 1. - (0.25 / eff).powf(1. / attack_frames);
            }
            let end = (fi + 32).min(n);
            while fi < end {
                let (xl, xr) = (l[fi] as f64, r[fi] as f64);
                self.pre[0][self.pw] = xl; self.pre[1][self.pw] = xr;
                let abs_in = xl.abs().max(xr.abs());
                let shaped = self.saturate(abs_in, k);
                let atten = if abs_in <= 0.0001 { 1. } else { shaped / abs_in };
                let db_atten = (-lin2db(atten)).max(2.);
                let sat_rate = db2lin(db_atten / sat_release_frames) - 1.;
                let rr = if atten > self.detector { sat_rate } else { 1. };
                self.detector += (atten - self.detector) * rr; self.detector = self.detector.min(1.);
                if !self.detector.is_finite() { self.detector = 1.; }
                if rate < 1. { self.gain += (scaled - self.gain) * rate; } else { self.gain = (self.gain * rate).min(1.); }
                let warp = (PI / 2. * self.gain).sin();
                let total = post * warp;
                let db_real = lin2db(warp);
                if db_real < self.reduction { self.reduction = db_real; } else { self.reduction += (db_real - self.reduction) * self.meter_k; }
                l[fi] = (self.pre[0][self.pr] * total) as f32; r[fi] = (self.pre[1][self.pr] * total) as f32;
                fi += 1; self.pr = (self.pr + 1) & (MAX_PRE - 1); self.pw = (self.pw + 1) & (MAX_PRE - 1);
            }
            if self.detector.abs() < 1e-30 { self.detector = 0.; }
            if self.gain.abs() < 1e-30 { self.gain = 0.; }
        }
    }
}

/// A WebAudio lowpass (Chrome's: Q as resonance in dB), its frequency gliding as setTargetAtTime does.
pub struct Distant { fs: f64, pub target: f64, f: f64, q_db: f64, c: [f64; 5], z: [[f64; 4]; 2], glide: f64, set_f: f64 }
impl Distant {
    pub fn new(fs: f64) -> Self { let mut d = Distant { fs, target: 20000., f: 20000., q_db: 0.5, c: [0.; 5], z: [[0.; 4]; 2], glide: 1. - (-1. / (0.08 * fs)).exp(), set_f: -1. }; d.coefs(); d }
    fn coefs(&mut self) {
        self.set_f = self.f;
        let nf = self.f / (self.fs * 0.5);
        if nf >= 1. { self.c = [1., 0., 0., 0., 0.]; return; }
        let w = PI * nf; let (cs, sn) = (w.cos(), w.sin()); let g = 10f64.powf(-0.05 * self.q_db); let al = 0.5 * sn * g;
        let a0 = 1. + al;
        self.c = [(1. - cs) / 2. / a0, (1. - cs) / a0, (1. - cs) / 2. / a0, -2. * cs / a0, (1. - al) / a0];
    }
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        for s in 0..l.len() {
            if self.f != self.target {
                self.f += (self.target - self.f) * self.glide;
                if (self.f - self.target).abs() < 1e-3 { self.f = self.target; }
                if s & 15 == 0 || self.f == self.target { self.coefs(); }
            } else if self.set_f != self.f { self.coefs(); }
            let c = &self.c;
            for ch in 0..2 {
                let z = &mut self.z[ch]; let x = if ch == 1 { r[s] } else { l[s] } as f64;
                let y = c[0] * x + c[1] * z[0] + c[2] * z[1] - c[3] * z[2] - c[4] * z[3]; z[1] = z[0]; z[0] = x; z[3] = z[2]; z[2] = y;
                if ch == 1 { r[s] = y as f32 } else { l[s] = y as f32 }
            }
        }
    }
}
