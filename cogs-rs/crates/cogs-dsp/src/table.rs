//! Wavetables, built additively (wave.worklet.js): a spectrum per frame, an inverse FFT per mip level (each level halves
//! the harmonics, so a note reads the level that stays under Nyquist). Building is heavy: the built-in tables are made
//! once (`builtin()`), a `harmonics` table when its patch is compiled, never on the audio thread.

use std::f64::consts::PI;
use std::sync::{Arc, OnceLock};

pub const WN: usize = 2048;
pub const WMIPS: usize = 10;
const H: usize = 512;

pub struct WaveTable {
    pub frames: usize,
    /// frames × mips × (WN+1) samples; the extra sample repeats the first, for interpolation
    data: Vec<f32>,
}
impl WaveTable {
    #[inline]
    pub fn frame(&self, mip: usize, f: usize) -> &[f32] { let o = (f * WMIPS + mip) * (WN + 1); &self.data[o..o + WN + 1] }
}

fn fft_inverse(re: &mut [f64], im: &mut [f64]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 { j ^= bit; bit >>= 1; }
        j ^= bit;
        if i < j { re.swap(i, j); im.swap(i, j); }
    }
    let mut len = 2;
    while len <= n {
        let ang = 2. * PI / len as f64;
        let (wr, wi) = (ang.cos(), ang.sin());
        let h = len >> 1;
        let mut i = 0;
        while i < n {
            let (mut cr, mut ci) = (1., 0.);
            for j in 0..h {
                let a = i + j; let b = a + h;
                let xr = re[b] * cr - im[b] * ci; let xi = re[b] * ci + im[b] * cr;
                re[b] = re[a] - xr; im[b] = im[a] - xi; re[a] += xr; im[a] += xi;
                let t = cr * wr - ci * wi; ci = cr * wi + ci * wr; cr = t;
            }
            i += len;
        }
        len <<= 1;
    }
}

pub fn build_table(spectra: &[Vec<f64>]) -> WaveTable {
    let mut re = vec![0.; WN];
    let mut im = vec![0.; WN];
    let mut data = vec![0f32; spectra.len() * WMIPS * (WN + 1)];
    for (f, a) in spectra.iter().enumerate() {
        let mut peak = 0f64;
        for m in 0..WMIPS {
            let hn = a.len().min((WN / 2 - 1) >> m);
            re.fill(0.); im.fill(0.);
            for k in 1..=hn { let x = a[k - 1]; if x != 0. { im[k] = -0.5 * x; im[WN - k] = 0.5 * x; } }
            fft_inverse(&mut re, &mut im);
            let o = (f * WMIPS + m) * (WN + 1);
            for i in 0..WN { data[o + i] = re[i] as f32; }
            data[o + WN] = data[o];
            if m == 0 { for i in 0..WN { peak = peak.max((data[o + i] as f64).abs()); } }
        }
        let g = if peak > 1e-9 { 1. / peak } else { 0. };
        for m in 0..WMIPS { let o = (f * WMIPS + m) * (WN + 1); for x in &mut data[o..o + WN + 1] { *x = (*x as f64 * g) as f32; } }
    }
    WaveTable { frames: spectra.len(), data }
}

fn frame(f: impl Fn(usize) -> f64) -> Vec<f64> { (1..=H).map(f).collect() }
/// Harmonic amplitudes (index k is harmonic k+1) for each frame of a built-in table; `user` is the harmonics table's.
pub fn wave_spectra(name: &str, user: &[f64]) -> Vec<Vec<f64>> {
    match name {
        "basic" => vec![frame(|k| if k == 1 { 1. } else { 0. }),
            frame(|k| if k % 2 == 1 { (if ((k - 1) / 2) % 2 == 1 { -1. } else { 1. }) / (k * k) as f64 } else { 0. }),
            frame(|k| 1. / k as f64), frame(|k| if k % 2 == 1 { 1. / k as f64 } else { 0. })],
        "sweep" => (0..8).map(|f| frame(|k| if k <= 1 << f { 1. / k as f64 } else { 0. })).collect(),
        "pulse" => (0..8).map(|f| { let w = 0.5 - 0.46 * f as f64 / 7.; frame(|k| (PI * k as f64 * w).sin() / k as f64) }).collect(),
        "organ" => {
            let d: [&[f64]; 3] = [&[1., 0.5, 0., 0.3], &[1., 1., 0.6, 0.6, 0., 0.4, 0., 0.3], &[1., 0.8, 0.8, 0.5, 0.5, 0.6, 0.2, 0.4, 0.1, 0.3, 0.1, 0.2]];
            d.iter().map(|d| frame(|k| d.get(k - 1).copied().unwrap_or(0.))).collect()
        }
        "vocal" => {
            const F: [[f64; 3]; 5] = [[800., 1150., 2900.], [400., 1600., 2700.], [350., 2300., 3000.], [450., 800., 2830.], [325., 700., 2530.]];
            let f0 = 110.;
            F.iter().map(|fs| frame(|k| {
                let h = k as f64 * f0;
                let mut a = 0.;
                for j in 0..3 { a += (-(((h - fs[j]) / (90. + 40. * j as f64)).powi(2))).exp() * [1., 0.6, 0.3][j]; }
                a / (k as f64).sqrt() + if k == 1 { 0.05 } else { 0. }
            })).collect()
        }
        _ => {
            let one = [1.];
            let u: &[f64] = if user.is_empty() { &one } else { user };
            vec![frame(|k| u.get(k - 1).copied().unwrap_or(0.)), frame(|k| if k % 2 == 1 { u.get(k - 1).copied().unwrap_or(0.) } else { 0. })]
        }
    }
}

/// The built-in tables, by WAVE_TABLES index (harmonics: its default, a sine). Made on first use: call it (or
/// `Engine::new`) off the audio thread.
pub fn builtin() -> &'static [Arc<WaveTable>; 6] {
    static T: OnceLock<[Arc<WaveTable>; 6]> = OnceLock::new();
    T.get_or_init(|| std::array::from_fn(|i| Arc::new(build_table(&wave_spectra(crate::params::WAVE_TABLES[i], &[])))))
}
