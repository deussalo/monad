//! Android's sound output: AAudio, called directly, so that every stream property can be asked for and read back (and
//! the calls newer than the app's minimum API are looked up at run time).
//!
//! The phone decides what a stream gets; a request for low latency can come back as a plain mixer track with ten times
//! the delay. So `open_best` tries a list of requests, keeps the best grant and logs each (tag "cogs"). The launch can
//! steer this, for comparing on a real phone:
//!
//!     adb shell am start -S -n help.techmate.cogs/.MainActivity --es audio probe
//!         every variant is opened and run silently for a moment, at launch and again with the app in front
//!     --es audio i16,shared,48000,game,sonification,stats
//!         this one stream only. Words: float i16 | shared exclusive | auto or a rate | media game | music movie speech
//!         sonification | session | perfnone | cap=N (buffer capacity) | cb=N (frames per callback) | bursts=N (buffer,
//!         in bursts; default 2)
//!     --es audio stats
//!         the normal choice, with a line of audio-thread figures every 2 s
use crate::play::{Render, Shared};
use std::ffi::{c_char, c_void, CStr};
use std::sync::atomic::{AtomicI32, AtomicU32, AtomicU64, Ordering::Relaxed};
use std::sync::Arc;

type B = *mut c_void;
type S = *mut c_void;
type DataCb = extern "C" fn(S, *mut c_void, *mut c_void, i32) -> i32;
type ErrorCb = extern "C" fn(S, *mut c_void, i32);

#[link(name = "aaudio")]
extern "C" {
    fn AAudio_createStreamBuilder(b: *mut B) -> i32;
    fn AAudio_convertResultToText(r: i32) -> *const c_char;
    fn AAudioStreamBuilder_delete(b: B) -> i32;
    fn AAudioStreamBuilder_setDirection(b: B, x: i32);
    fn AAudioStreamBuilder_setFormat(b: B, x: i32);
    fn AAudioStreamBuilder_setPerformanceMode(b: B, x: i32);
    fn AAudioStreamBuilder_setSharingMode(b: B, x: i32);
    fn AAudioStreamBuilder_setSampleRate(b: B, x: i32);
    fn AAudioStreamBuilder_setChannelCount(b: B, x: i32);
    fn AAudioStreamBuilder_setBufferCapacityInFrames(b: B, x: i32);
    fn AAudioStreamBuilder_setFramesPerDataCallback(b: B, x: i32);
    fn AAudioStreamBuilder_setDataCallback(b: B, f: DataCb, u: *mut c_void);
    fn AAudioStreamBuilder_setErrorCallback(b: B, f: ErrorCb, u: *mut c_void);
    fn AAudioStreamBuilder_openStream(b: B, s: *mut S) -> i32;
    fn AAudioStream_close(s: S) -> i32;
    fn AAudioStream_requestStart(s: S) -> i32;
    fn AAudioStream_requestStop(s: S) -> i32;
    fn AAudioStream_getSampleRate(s: S) -> i32;
    fn AAudioStream_getChannelCount(s: S) -> i32;
    fn AAudioStream_getFormat(s: S) -> i32;
    fn AAudioStream_getSharingMode(s: S) -> i32;
    fn AAudioStream_getPerformanceMode(s: S) -> i32;
    fn AAudioStream_getFramesPerBurst(s: S) -> i32;
    fn AAudioStream_getBufferSizeInFrames(s: S) -> i32;
    fn AAudioStream_getBufferCapacityInFrames(s: S) -> i32;
    fn AAudioStream_setBufferSizeInFrames(s: S, n: i32) -> i32;
    fn AAudioStream_getXRunCount(s: S) -> i32;
    fn AAudioStream_getFramesPerDataCallback(s: S) -> i32;
    fn AAudioStream_getDeviceId(s: S) -> i32;
    fn AAudioStream_getTimestamp(s: S, clock: i32, pos: *mut i64, ns: *mut i64) -> i32;
}

const FORMAT_I16: i32 = 1;
const FORMAT_FLOAT: i32 = 2;
const PERF_NONE: i32 = 10;
const PERF_LOW: i32 = 12;
const EXCLUSIVE: i32 = 0;
const SHARED: i32 = 1;
const USAGE_MEDIA: i32 = 1;
const USAGE_GAME: i32 = 14;
const SPEECH: i32 = 1;
const MUSIC: i32 = 2;
const MOVIE: i32 = 3;
const SONIFICATION: i32 = 4;
const SESSION_ALLOCATE: i32 = 0;
const CLOCK_MONOTONIC: i32 = 1;
const MAX_BLOCK: usize = 4096;

fn err_text(r: i32) -> String { unsafe { CStr::from_ptr(AAudio_convertResultToText(r)) }.to_string_lossy().into_owned() }
/// A function the phone may not have (API 28+), by name.
fn optional(name: &CStr) -> *mut c_void {
    unsafe {
        let h = libc::dlopen(c"libaaudio.so".as_ptr(), libc::RTLD_NOW);
        if h.is_null() { std::ptr::null_mut() } else { libc::dlsym(h, name.as_ptr()) }
    }
}
fn set_optional(b: B, name: &CStr, x: i32) {
    let f = optional(name);
    if !f.is_null() { let f: extern "C" fn(B, i32) = unsafe { std::mem::transmute(f) }; f(b, x) }
}
fn mmap_used(s: S) -> Option<bool> {
    let f = optional(c"AAudioStream_isMMapUsed");
    if f.is_null() { return None }
    let f: extern "C" fn(S) -> bool = unsafe { std::mem::transmute(f) };
    Some(f(s))
}
fn now(clock: libc::clockid_t) -> u64 {
    let mut t = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    unsafe { libc::clock_gettime(clock, &mut t) };
    t.tv_sec as u64 * 1_000_000_000 + t.tv_nsec as u64
}

/// What to ask for. 0 is "leave it to the phone".
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spec { format: i32, sharing: i32, rate: i32, usage: i32, content: i32, session: bool, perf: i32, capacity: i32, per_callback: i32 }
impl Spec {
    const fn new(format: i32, sharing: i32, rate: i32, usage: i32) -> Spec {
        Spec { format, sharing, rate, usage, content: if usage != 0 { MUSIC } else { 0 }, session: false, perf: PERF_LOW, capacity: 0, per_callback: 0 }
    }
    fn label(&self) -> String {
        let mut s = format!("{} {} {}", if self.format == FORMAT_I16 { "i16" } else { "float" }, if self.sharing == EXCLUSIVE { "exclusive" } else { "shared" },
            if self.rate == 0 { "auto".to_string() } else { self.rate.to_string() });
        s += match self.usage { USAGE_GAME => " game", USAGE_MEDIA => " media", _ => " default" };
        s += match self.content { MUSIC => "/music", MOVIE => "/movie", SPEECH => "/speech", SONIFICATION => "/sonification", _ => "" };
        if self.session { s += " session" }
        if self.perf != PERF_LOW { s += " perfnone" }
        if self.capacity > 0 { s += &format!(" cap={}", self.capacity) }
        if self.per_callback > 0 { s += &format!(" cb={}", self.per_callback) }
        s
    }
}

/// The order `open_best` tries, stopping at the first good grant. The plain media attributes come first, so nothing about
/// volume or routing changes on a phone that gives them the fast path.
const ORDER: [Spec; 8] = [
    Spec::new(FORMAT_FLOAT, EXCLUSIVE, 0, USAGE_MEDIA),
    Spec::new(FORMAT_FLOAT, SHARED, 0, USAGE_MEDIA),
    Spec::new(FORMAT_I16, SHARED, 0, USAGE_MEDIA),
    Spec::new(FORMAT_FLOAT, SHARED, 0, USAGE_GAME),
    Spec::new(FORMAT_I16, SHARED, 0, USAGE_GAME),
    Spec::new(FORMAT_FLOAT, SHARED, 48000, USAGE_MEDIA),
    Spec::new(FORMAT_I16, SHARED, 48000, USAGE_MEDIA),
    Spec::new(FORMAT_FLOAT, SHARED, 48000, USAGE_GAME),
];

#[derive(Clone, Debug, Default)]
pub struct Options { pub probe: bool, pub stats: bool, pub bursts: i32, forced: Option<Spec> }
impl Options {
    pub fn parse(text: &str) -> Options {
        let mut o = Options { bursts: 2, ..Default::default() };
        let mut s = Spec::new(FORMAT_FLOAT, SHARED, 0, 0);
        let mut forced = false;
        for w in text.split(',').map(str::trim).filter(|w| !w.is_empty()) {
            let mut force = true;
            match w {
                "probe" => { o.probe = true; force = false }
                "stats" => { o.stats = true; force = false }
                "float" => s.format = FORMAT_FLOAT, "i16" => s.format = FORMAT_I16,
                "shared" => s.sharing = SHARED, "exclusive" => s.sharing = EXCLUSIVE,
                "auto" => s.rate = 0,
                "media" => s.usage = USAGE_MEDIA, "game" => s.usage = USAGE_GAME,
                "music" => s.content = MUSIC, "movie" => s.content = MOVIE, "speech" => s.content = SPEECH, "sonification" => s.content = SONIFICATION,
                "session" => s.session = true, "perfnone" => s.perf = PERF_NONE,
                _ => {
                    let num = |p: &str| w.strip_prefix(p).and_then(|x| x.parse::<i32>().ok());
                    if let Some(n) = num("cap=") { s.capacity = n }
                    else if let Some(n) = num("cb=") { s.per_callback = n }
                    else if let Some(n) = num("bursts=") { o.bursts = n.max(1); force = false }
                    else if let Ok(n) = w.parse::<i32>() { s.rate = n }
                    else { log::warn!("audio option not understood: {w}"); force = false }
                }
            }
            forced |= force;
        }
        if forced { o.forced = Some(s) }
        o
    }
}

/// What a stream was actually given.
#[derive(Clone, Copy, Debug)]
pub struct Info { pub rate: i32, channels: i32, format: i32, sharing: i32, perf: i32, burst: i32, buffer: i32, capacity: i32, per_callback: i32, device: i32, mmap: Option<bool> }
impl Info {
    fn line(&self) -> String {
        format!("perf={} sharing={} mmap={} format={} rate={} ch={} burst={} ({:.1} ms) buffer={} capacity={} per_callback={} device={}",
            match self.perf { PERF_LOW => "LOW_LATENCY", PERF_NONE => "NONE", 11 => "POWER_SAVING", _ => "?" }, if self.sharing == EXCLUSIVE { "EXCLUSIVE" } else { "SHARED" },
            self.mmap.map_or("?", |m| if m { "yes" } else { "no" }), match self.format { FORMAT_I16 => "i16", FORMAT_FLOAT => "float", _ => "?" },
            self.rate, self.channels, self.burst, self.burst_ms(), self.buffer, self.capacity, self.per_callback, self.device)
    }
    fn burst_ms(&self) -> f64 { self.burst as f64 * 1000. / self.rate.max(1) as f64 }
    /// Lower is better: what two bursts of buffer delay, with a refused fast path or an oversampled rate far behind.
    fn cost(&self) -> f64 { 2. * self.burst_ms() + if self.perf == PERF_LOW { 0. } else { 1000. } + if self.rate > 50000 { 500. } else { 0. } }
    fn good(&self) -> bool { self.perf == PERF_LOW && self.rate <= 50000 && self.burst_ms() <= 6. }
}

/// What the callback keeps count of, read from elsewhere.
#[derive(Default)]
struct Stats {
    callbacks: AtomicU64, frames: AtomicU64, in_cpu_ns: AtomicU64, in_wall_ns: AtomicU64, thread_cpu_ns: AtomicU64,
    min_frames: AtomicU32, max_frames: AtomicU32, worst_pct: AtomicU32, ahead_us: AtomicU32, xruns: AtomicI32,
}
static CURRENT: std::sync::Mutex<Option<Arc<Stats>>> = std::sync::Mutex::new(None);

struct Live { render: Render, sh: Arc<Shared> }
struct Cb {
    live: Option<Live>, stats: Arc<Stats>, l: Vec<f32>, r: Vec<f32>, written: i64, latency: f64, sr: f64, format: i32, channels: usize,
    burst: i32, capacity: i32, buffer: i32, xruns: i32, win_wall: u64, win_cpu: u64, win_peak: u32,
}

extern "C" fn on_error(_s: S, _u: *mut c_void, e: i32) { log::error!("audio stream error: {}", err_text(e)) }

extern "C" fn on_data(s: S, user: *mut c_void, data: *mut c_void, frames: i32) -> i32 {
    let c = unsafe { &mut *(user as *mut Cb) };
    let (wall0, cpu0) = (now(libc::CLOCK_MONOTONIC), now(libc::CLOCK_THREAD_CPUTIME_ID));
    let n = frames.max(0) as usize;
    let ch = c.channels.max(1);
    let mut done = 0;
    while done < n {
        let m = (n - done).min(MAX_BLOCK);
        let (l, r) = (&mut c.l[..m], &mut c.r[..m]);
        match &mut c.live { Some(x) => x.render.block(l, r, c.latency), None => { l.fill(0.); r.fill(0.) } }
        if c.format == FORMAT_I16 {
            let out = unsafe { std::slice::from_raw_parts_mut((data as *mut i16).add(done * ch), m * ch) };
            for (i, f) in out.chunks_exact_mut(ch).enumerate() { for (k, x) in f.iter_mut().enumerate() { let v = if k % 2 == 0 { l[i] } else { r[i] }; *x = (v.clamp(-1., 1.) * 32767.).round() as i16 } }
        } else {
            let out = unsafe { std::slice::from_raw_parts_mut((data as *mut f32).add(done * ch), m * ch) };
            for (i, f) in out.chunks_exact_mut(ch).enumerate() { for (k, x) in f.iter_mut().enumerate() { *x = if k % 2 == 0 { l[i] } else { r[i] } } }
        }
        done += m;
    }
    c.written += n as i64;
    // What is queued ahead of the speaker now: the timestamp says which frame played when, so carry it forward to now.
    let (mut pos, mut ts) = (0i64, 0i64);
    let wall1 = now(libc::CLOCK_MONOTONIC);
    if unsafe { AAudioStream_getTimestamp(s, CLOCK_MONOTONIC, &mut pos, &mut ts) } == 0 {
        let played = pos as f64 + (wall1 as f64 - ts as f64) * 1e-9 * c.sr;
        let ahead = c.written as f64 - played;
        if ahead > 0. { c.latency = ahead / c.sr; c.stats.ahead_us.store((c.latency * 1e6) as u32, Relaxed) }
    }
    // A glitch: one more burst of buffer, up to what the stream has.
    let x = unsafe { AAudioStream_getXRunCount(s) };
    if x > c.xruns && c.buffer + c.burst <= c.capacity {
        c.buffer = unsafe { AAudioStream_setBufferSizeInFrames(s, c.buffer + c.burst) }.max(c.buffer);
    }
    c.xruns = x;
    let cpu1 = now(libc::CLOCK_THREAD_CPUTIME_ID);
    let st = &c.stats;
    st.callbacks.fetch_add(1, Relaxed); st.frames.fetch_add(n as u64, Relaxed); st.in_cpu_ns.fetch_add(cpu1 - cpu0, Relaxed); st.in_wall_ns.fetch_add(wall1 - wall0, Relaxed);
    st.thread_cpu_ns.store(cpu1, Relaxed); st.xruns.store(x, Relaxed);
    st.max_frames.fetch_max(n as u32, Relaxed);
    let mn = st.min_frames.load(Relaxed); if mn == 0 || (n as u32) < mn { st.min_frames.store(n as u32, Relaxed) }
    let pct = ((wall1 - wall0) as f64 / (n.max(1) as f64 / c.sr * 1e9) * 100.) as u32;
    st.worst_pct.fetch_max(pct, Relaxed);
    c.win_peak = c.win_peak.max(pct);
    // The meters: the thread's whole CPU over half a second (AAudio's own work between callbacks included).
    if c.win_wall == 0 { c.win_wall = wall1; c.win_cpu = cpu1 }
    else if wall1 - c.win_wall >= 500_000_000 {
        if let Some(l) = &c.live {
            let m = &l.sh.meters;
            m.busy.store(((cpu1 - c.win_cpu) as f64 / (wall1 - c.win_wall) as f64 * 100.).round() as u32, Relaxed);
            m.busy_peak.store(c.win_peak, Relaxed);
            m.latency_us.store((c.latency * 1e6) as u32, Relaxed);
            m.xruns.store(x.max(0) as u32, Relaxed);
            m.buffer.store(c.buffer as u32, Relaxed);
        }
        c.win_wall = wall1; c.win_cpu = cpu1; c.win_peak = 0;
    }
    0
}

/// An open stream and what its callback owns.
pub struct Stream { s: S, cb: Box<Cb>, pub info: Info }
unsafe impl Send for Stream {}
impl Drop for Stream { fn drop(&mut self) { unsafe { AAudioStream_requestStop(self.s); AAudioStream_close(self.s); } } }

fn open(spec: &Spec) -> Result<Stream, String> {
    let stats = Arc::new(Stats::default());
    let mut cb = Box::new(Cb { live: None, stats, l: vec![0.; MAX_BLOCK], r: vec![0.; MAX_BLOCK], written: 0, latency: 0.02, sr: 48000., format: 0, channels: 2,
        burst: 0, capacity: 0, buffer: 0, xruns: 0, win_wall: 0, win_cpu: 0, win_peak: 0 });
    unsafe {
        let mut b: B = std::ptr::null_mut();
        let r = AAudio_createStreamBuilder(&mut b);
        if r != 0 { return Err(err_text(r)) }
        AAudioStreamBuilder_setDirection(b, 0);
        AAudioStreamBuilder_setFormat(b, spec.format);
        AAudioStreamBuilder_setPerformanceMode(b, spec.perf);
        AAudioStreamBuilder_setSharingMode(b, spec.sharing);
        AAudioStreamBuilder_setChannelCount(b, 2);
        if spec.rate > 0 { AAudioStreamBuilder_setSampleRate(b, spec.rate) }
        if spec.capacity > 0 { AAudioStreamBuilder_setBufferCapacityInFrames(b, spec.capacity) }
        if spec.per_callback > 0 { AAudioStreamBuilder_setFramesPerDataCallback(b, spec.per_callback) }
        if spec.usage != 0 { set_optional(b, c"AAudioStreamBuilder_setUsage", spec.usage) }
        if spec.content != 0 { set_optional(b, c"AAudioStreamBuilder_setContentType", spec.content) }
        if spec.session { set_optional(b, c"AAudioStreamBuilder_setSessionId", SESSION_ALLOCATE) }
        let user = &mut *cb as *mut Cb as *mut c_void;
        AAudioStreamBuilder_setDataCallback(b, on_data, user);
        AAudioStreamBuilder_setErrorCallback(b, on_error, user);
        let mut s: S = std::ptr::null_mut();
        let r = AAudioStreamBuilder_openStream(b, &mut s);
        AAudioStreamBuilder_delete(b);
        if r != 0 { return Err(err_text(r)) }
        let info = Info { rate: AAudioStream_getSampleRate(s), channels: AAudioStream_getChannelCount(s), format: AAudioStream_getFormat(s), sharing: AAudioStream_getSharingMode(s),
            perf: AAudioStream_getPerformanceMode(s), burst: AAudioStream_getFramesPerBurst(s), buffer: AAudioStream_getBufferSizeInFrames(s),
            capacity: AAudioStream_getBufferCapacityInFrames(s), per_callback: AAudioStream_getFramesPerDataCallback(s), device: AAudioStream_getDeviceId(s), mmap: mmap_used(s) };
        cb.sr = info.rate as f64; cb.format = info.format; cb.channels = info.channels.max(1) as usize; cb.burst = info.burst; cb.capacity = info.capacity; cb.buffer = info.buffer;
        Ok(Stream { s, cb, info })
    }
}

/// Try the requests in order and keep the best grant, stopping at a good one. `rate`: insist on this rate.
pub fn open_best(o: &Options, rate: Option<i32>) -> Option<Stream> {
    let list: Vec<Spec> = match o.forced { Some(s) => vec![s], None => ORDER.to_vec() };
    // each trial is closed before the next, so that one held open cannot change what the next is granted
    let mut best: Option<(Spec, f64)> = None;
    for mut spec in list {
        if let Some(r) = rate { spec.rate = r }
        match open(&spec) {
            Err(e) => log::info!("audio open [{}]: failed: {e}", spec.label()),
            Ok(s) => {
                log::info!("audio open [{}]: {}", spec.label(), s.info.line());
                if s.info.good() { log::info!("audio chosen [{}]", spec.label()); return Some(s) }
                let cost = s.info.cost();
                if best.map_or(true, |b| cost < b.1) { best = Some((spec, cost)) }
            }
        }
    }
    let (spec, _) = best?;
    let s = open(&spec).map_err(|e| log::error!("audio reopen [{}]: {e}", spec.label())).ok()?;
    log::info!("audio chosen [{}]: {}", spec.label(), s.info.line());
    Some(s)
}

impl Stream {
    /// Hand it the machine and start it. `bursts`: the buffer, in bursts (it grows by itself on a glitch).
    pub fn start(mut self, render: Render, sh: Arc<Shared>, bursts: i32) -> Option<Stream> {
        self.cb.live = Some(Live { render, sh: sh.clone() });
        let want = self.info.burst * bursts.max(1);
        let got = unsafe { AAudioStream_setBufferSizeInFrames(self.s, want) };
        if got > 0 { self.cb.buffer = got; self.info.buffer = got }
        let m = &sh.meters;
        m.sample_rate.store(self.info.rate as u32, Relaxed);
        m.burst.store(self.info.burst as u32, Relaxed);
        m.buffer.store(self.info.buffer as u32, Relaxed);
        if let Ok(mut c) = CURRENT.lock() { *c = Some(self.cb.stats.clone()) }
        let r = unsafe { AAudioStream_requestStart(self.s) };
        if r != 0 { log::error!("audio start failed: {}", err_text(r)); return None }
        log::info!("audio started: buffer {} frames ({:.1} ms)", self.info.buffer, self.info.buffer as f64 * 1000. / self.info.rate.max(1) as f64);
        Some(self)
    }
}

/// Every variant, opened and run silently for a moment; one log line each.
pub fn probe(when: &str, _o: &Options) {
    let mut list = Vec::new();
    for usage in [0, USAGE_GAME] { for format in [FORMAT_FLOAT, FORMAT_I16] { for rate in [0, 48000] { for sharing in [EXCLUSIVE, SHARED] { list.push(Spec::new(format, sharing, rate, usage)) } } } }
    let base = Spec::new(FORMAT_FLOAT, SHARED, 0, 0);
    list.push(Spec { session: true, ..base });
    list.push(Spec { usage: USAGE_GAME, content: SONIFICATION, ..base });
    list.push(Spec { usage: USAGE_MEDIA, content: MOVIE, ..base });
    list.push(Spec { per_callback: 192, ..base });
    list.push(Spec { perf: PERF_NONE, ..base });
    for (i, spec) in list.iter().enumerate() {
        let t = std::time::Instant::now();
        let s = match open(spec) { Ok(s) => s, Err(e) => { log::info!("audio probe {when} {i:2} [{}]: open failed: {e}", spec.label()); continue } };
        let opened = t.elapsed().as_secs_f64() * 1000.;
        let r = unsafe { AAudioStream_requestStart(s.s) };
        if r != 0 { log::info!("audio probe {when} {i:2} [{}]: {} / start failed: {}", spec.label(), s.info.line(), err_text(r)); continue }
        std::thread::sleep(std::time::Duration::from_millis(250));
        let st = &s.cb.stats;
        let (c0, f0, w0, t0) = (st.callbacks.load(Relaxed), st.thread_cpu_ns.load(Relaxed), st.xruns.load(Relaxed), std::time::Instant::now());
        std::thread::sleep(std::time::Duration::from_millis(250));
        let (cbs, cpu) = (st.callbacks.load(Relaxed) - c0, st.thread_cpu_ns.load(Relaxed).saturating_sub(f0));
        let pct = cpu as f64 / t0.elapsed().as_secs_f64() / 1e7;
        log::info!("audio probe {when} {i:2} [{}]: {} | open {opened:.0} ms, callbacks {cbs} of {}..{} frames, ahead {:.1} ms, idle thread cpu {pct:.1}%, xruns {}",
            spec.label(), s.info.line(), st.min_frames.load(Relaxed), st.max_frames.load(Relaxed), st.ahead_us.load(Relaxed) as f64 / 1000., st.xruns.load(Relaxed) - w0);
    }
}

/// A line every 2 s about the audio thread: its whole CPU (what `top` counts), the part inside our callback, how close
/// the worst callback came to its deadline, and how often the UI held the world when audio wanted it.
pub fn log_stats(sh: Arc<Shared>) {
    std::thread::spawn(move || {
        let mut last: Option<(std::time::Instant, u64, u64, u64, u64, u64, u64, u64)> = None;
        while sh.running.load(Relaxed) {
            std::thread::sleep(std::time::Duration::from_secs(2));
            let Some(st) = CURRENT.lock().ok().and_then(|c| c.clone()) else { continue };
            let m = &sh.meters;
            let cur = (std::time::Instant::now(), st.thread_cpu_ns.load(Relaxed), st.in_cpu_ns.load(Relaxed), st.in_wall_ns.load(Relaxed), st.callbacks.load(Relaxed), st.frames.load(Relaxed),
                m.blocks.load(Relaxed), m.lock_misses.load(Relaxed));
            if let Some(p) = last {
                let ns = (cur.0 - p.0).as_secs_f64() * 1e9;
                let pc = |a: u64, b: u64| a.saturating_sub(b) as f64 / ns * 100.;
                let cbs = (cur.4 - p.4).max(1);
                log::info!("audio stats: thread cpu {:.1}% (in callback cpu {:.1}%, wall {:.1}%) | {} callbacks, {} frames each ({}..{}) | worst callback {}% of its time | ahead {:.1} ms | xruns {} | world locked by UI: {} of {} blocks, most owed {:.1} ms",
                    pc(cur.1, p.1), pc(cur.2, p.2), pc(cur.3, p.3), cur.4 - p.4, (cur.5 - p.5) / cbs, st.min_frames.swap(0, Relaxed), st.max_frames.swap(0, Relaxed),
                    st.worst_pct.swap(0, Relaxed), st.ahead_us.load(Relaxed) as f64 / 1000., st.xruns.load(Relaxed), cur.7 - p.7, cur.6 - p.6, m.owed_max_us.swap(0, Relaxed) as f64 / 1000.);
            }
            last = Some(cur);
        }
    });
}
