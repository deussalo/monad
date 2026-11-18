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
    /// Within the last `render`: where this output's last frame went (handing over), or its first (taking over).
    pub mark: Option<usize>,
    joining: bool,
    sr: f64, l: Vec<f32>, r: Vec<f32>, fade: usize, fade_in: usize, fade_out: usize, pad: u64,
}
impl<T: Source> Out<T> {
    pub fn new(sr: f64) -> Self {
        Out { src: None, handoff: Arc::new(Handoff::new()), incoming: None, latency: 0.02, last_pad: 0, mark: None, joining: false, sr, l: vec![0.; MAX_BLOCK], r: vec![0.; MAX_BLOCK],
            fade: (sr * FADE_S) as usize, fade_in: 0, fade_out: 0, pad: 0 }
    }
    /// Produce `n` frames into `sink(frame, left, right)`. `ahead`: seconds until the first of them reaches the speaker.
    pub fn render(&mut self, n: usize, ahead: f64, now_ns: u64, mut sink: impl FnMut(usize, f32, f32)) {
        self.mark = None;
        if self.src.is_none() {
            if let Some((s, end)) = self.incoming.as_ref().and_then(|h| h.take()) {
                // start where the old output's fade starts
                let first = now_ns as f64 + ahead * 1e9;
                let target = end as f64 - self.fade as f64 / self.sr * 1e9;
                self.pad = (((target - first) * 1e-9 * self.sr).max(0.)) as u64;
                self.last_pad = self.pad;
                self.src = Some(s); self.fade_in = self.fade; self.incoming = None; self.joining = true;
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
            if self.joining { self.joining = false; self.mark = Some(i) }
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
                self.mark = Some(i);
                if let Some(s) = self.src.take() { self.handoff.give(s, end as u64) }
            }
        }
    }
}

/// When a buffer that grew after a glitch comes back down. After a clean stretch it steps down; a glitch shortly after
/// such a step shows that level was too low, and the level it then grows to becomes the floor. Glitches while a stream is
/// settling (a start, a hand-over) or while the app is away say nothing about the normal depth and do not count.
#[derive(Clone, Copy, Debug)]
pub struct Ladder { pub floor: u32, step_at: f64, glitch_at: f64, settle_until: f64, stepped: bool }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Move { Stay, Up, Down }
impl Ladder {
    pub const CLEAN: f64 = 10.;
    pub const BETWEEN: f64 = 5.;
    pub const BLAME: f64 = 15.;
    pub const SETTLE: f64 = 1.5;
    pub fn new(floor: u32, t: f64) -> Self { Ladder { floor, step_at: t, glitch_at: t, settle_until: t + Self::SETTLE, stepped: false } }
    pub fn settle(&mut self, t: f64) { self.settle_until = t + Self::SETTLE; self.glitch_at = t }
    /// One look at `value`, which moves by `step`. On `Up` the caller grows it, then reports the new value with `grew`.
    pub fn tick(&mut self, t: f64, glitch: bool, value: u32, step: u32, away: bool) -> Move {
        if away || t < self.settle_until { self.glitch_at = self.glitch_at.max(t.min(self.settle_until)); return Move::Stay }
        if glitch { self.glitch_at = t; return Move::Up }
        if t - self.glitch_at > Self::CLEAN && t - self.step_at > Self::BETWEEN && value >= self.floor + step { self.step_at = t; self.stepped = true; return Move::Down }
        Move::Stay
    }
    pub fn grew(&mut self, t: f64, value: u32) { if self.stepped && t - self.step_at < Self::BLAME { self.floor = self.floor.max(value) } }
}
