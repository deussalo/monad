//! A small stand-in voice until cogs-dsp's wave synth is wired in: a detuned saw pair through a resonant lowpass with
//! its own envelope, the shape of the 'pluck' preset. It is only here for the first latency test.
use std::f32::consts::TAU;

#[derive(Clone, Copy, Default)]
struct Voice { note: u8, on: bool, gate: bool, ph: [f32; 2], inc: [f32; 2], env: f32, fenv: f32, amp: f32, lp: [f32; 2], age: u32 }

pub struct TestSynth { sr: f32, v: [Voice; 16], clock: u32 }

impl TestSynth {
    pub fn new(sr: f32) -> Self { TestSynth { sr, v: [Voice::default(); 16], clock: 0 } }
    pub fn note_on(&mut self, note: u8, vel: u8) {
        self.clock += 1;
        let i = self.v.iter().position(|v| !v.on).unwrap_or_else(|| (0..16).min_by_key(|&i| self.v[i].age).unwrap());
        let f = 440.0 * 2f32.powf((note as f32 - 69.0) / 12.0);
        self.v[i] = Voice { note, on: true, gate: true, ph: [0.0, 0.3], inc: [f / self.sr, f * 1.004 / self.sr], env: 0.0, fenv: 1.0,
            amp: 0.25 * (0.3 + 0.7 * vel as f32 / 127.0), lp: [0.0; 2], age: self.clock };
    }
    pub fn note_off(&mut self, note: u8) { for v in &mut self.v { if v.on && v.note == note { v.gate = false } } }
    pub fn panic(&mut self) { for v in &mut self.v { v.on = false } }
    pub fn render(&mut self, l: &mut [f32], r: &mut [f32]) {
        let sr = self.sr;
        let (att, rel, fdec) = (1.0 / (0.002 * sr), (-4.6 / (0.25 * sr)).exp(), (-4.6 / (0.3 * sr)).exp());
        for v in self.v.iter_mut().filter(|v| v.on) {
            for i in 0..l.len() {
                if v.gate { v.env = (v.env + att).min(1.0) } else { v.env *= rel; if v.env < 1e-4 { v.on = false; break } }
                v.fenv *= fdec;
                let mut s = 0.0;
                for k in 0..2 { v.ph[k] += v.inc[k]; if v.ph[k] >= 1.0 { v.ph[k] -= 1.0 } s += 2.0 * v.ph[k] - 1.0 }
                let cut = (400.0 + 6000.0 * v.fenv).min(sr * 0.45);
                let g = 1.0 - (-TAU * cut / sr).exp();
                v.lp[0] += (s - v.lp[0]) * g; v.lp[1] += (v.lp[0] - v.lp[1]) * g;
                let y = v.lp[1] * v.env * v.amp;
                l[i] += y; r[i] += y;
            }
        }
    }
}
