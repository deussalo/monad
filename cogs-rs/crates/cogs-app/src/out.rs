//! One audio output's rendering, and moving the machine from one output to another while it plays. Nothing here touches
//! the platform, so the hand-over can be checked anywhere (examples/handoff.rs).
//!
//! A new output usually sits closer to the speaker than the old one. The old one has audio queued that is still to be
//! heard, so the new one stays silent until that has nearly played out (`pad`), then carries on from the very next
//! sample: nothing is repeated or skipped, and the machine simply waits out the difference. The two meet in a 5 ms
//! equal-power crossfade, so a small error in the timing is not a click.
use std::f32::consts::FRAC_PI_2;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU64, Ordering::{Acquire, Relaxed, Release}};
use std::sync::Arc;

pub const MAX_BLOCK: usize = 4096;
const FADE_S: f64 = 0.005;

pub trait Source: Send { fn block(&mut self, l: &mut [f32], r: &mut [f32], latency: f64); }

/// Where the machine passes from an output to its successor, without a lock.
pub struct Handoff<T> { slot: AtomicPtr<T>, request: AtomicBool, end_ns: AtomicU64, taken: AtomicBool }
impl<T> Handoff<T> {
    pub fn new() -> Self { Handoff { slot: AtomicPtr::new(std::ptr::null_mut()), request: AtomicBool::new(false), end_ns: AtomicU64::new(0), taken: AtomicBool::new(false) } }
    pub fn request(&self) { self.request.store(true, Release) }
    pub fn taken(&self) -> bool { self.taken.load(Acquire) }
    /// When the outgoing output's last frame reaches the speaker (valid once taken).
    pub fn end_ns(&self) -> u64 { self.end_ns.load(Relaxed) }
    fn give(&self, x: Box<T>, end_ns: u64) { self.end_ns.store(end_ns, Relaxed); self.slot.store(Box::into_raw(x), Release) }
    pub fn take(&self) -> Option<(Box<T>, u64)> {
        let p = self.slot.swap(std::ptr::null_mut(), Acquire);
        if p.is_null() { return None }
        self.taken.store(true, Release);
        Some((unsafe { Box::from_raw(p) }, self.end_ns.load(Relaxed)))
    }
}
impl<T> Default for Handoff<T> { fn default() -> Self { Self::new() } }
impl<T> Drop for Handoff<T> { fn drop(&mut self) { let p = *self.slot.get_mut(); if !p.is_null() { drop(unsafe { Box::from_raw(p) }) } } }

pub struct Out<T> {
    pub src: Option<Box<T>>,
    /// This output's way out, to its successor.
    pub handoff: Arc<Handoff<T>>,
    /// Its predecessor's, while waiting to take over.
    pub incoming: Option<Arc<Handoff<T>>>,
    pub latency: f64,
    /// The silence the last take-over waited, frames.
    pub last_pad: u64,
    sr: f64, l: Vec<f32>, r: Vec<f32>, fade: usize, fade_in: usize, fade_out: usize, pad: u64,
}
impl<T: Source> Out<T> {
    pub fn new(sr: f64) -> Self {
        Out { src: None, handoff: Arc::new(Handoff::new()), incoming: None, latency: 0.02, last_pad: 0, sr, l: vec![0.; MAX_BLOCK], r: vec![0.; MAX_BLOCK],
            fade: (sr * FADE_S) as usize, fade_in: 0, fade_out: 0, pad: 0 }
    }
    /// Produce `n` frames into `sink(frame, left, right)`. `ahead`: seconds until the first of them reaches the speaker.
    pub fn render(&mut self, n: usize, ahead: f64, now_ns: u64, mut sink: impl FnMut(usize, f32, f32)) {
        if self.src.is_none() {
            if let Some((s, end)) = self.incoming.as_ref().and_then(|h| h.take()) {
                // start where the old output's fade starts
                let first = now_ns as f64 + ahead * 1e9;
                let target = end as f64 - self.fade as f64 / self.sr * 1e9;
                self.pad = (((target - first) * 1e-9 * self.sr).max(0.)) as u64;
                self.last_pad = self.pad;
                self.src = Some(s); self.fade_in = self.fade; self.incoming = None;
            }
        }
        let f = self.fade.max(1) as f32;
        let mut i = 0;
        while i < n {
            if self.pad > 0 {
                let m = (self.pad as usize).min(n - i);
                for k in 0..m { sink(i + k, 0., 0.) }
                i += m; self.pad -= m as u64;
                continue;
            }
            let Some(src) = self.src.as_mut() else { for k in i..n { sink(k, 0., 0.) } return };
            if self.fade_out == 0 && self.handoff.request.load(Acquire) { self.fade_out = self.fade.max(1) }
            let leaving = self.fade_out > 0;
            let mut m = (n - i).min(MAX_BLOCK);
            if leaving { m = m.min(self.fade_out) }
            src.block(&mut self.l[..m], &mut self.r[..m], self.latency);
            for k in 0..m {
                let mut g = 1f32;
                if self.fade_in > 0 { g *= ((f - self.fade_in as f32 + 1.) / f * FRAC_PI_2).sin(); self.fade_in -= 1 }
                if self.fade_out > 0 { self.fade_out -= 1; g *= (self.fade_out as f32 / f * FRAC_PI_2).sin() }
                sink(i + k, self.l[k] * g, self.r[k] * g);
            }
            i += m;
            if leaving && self.fade_out == 0 {
                let end = now_ns as f64 + (ahead + i as f64 / self.sr) * 1e9;
                if let Some(s) = self.src.take() { self.handoff.give(s, end as u64) }
            }
        }
    }
}
