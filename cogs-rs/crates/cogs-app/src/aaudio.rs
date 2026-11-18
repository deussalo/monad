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
//!     --es audio delay=3000,stats
//!         start on whatever is granted without help, and try for the fast path only after 3 s (and then on the timer)
//!     --es audio fast,stats
//!         try for the fast path. OFF by default: on the Moto G57 the engine cannot keep up with 4 ms callbacks yet
//!         (about 93% of a core, hundreds of xruns), so a normal launch stays on the normal stream
//!     --es audio nokick,stats
//!         never use the deep-buffer stream described below
//!
//! Why a fast track can be refused: AudioFlinger will not make one on an output whose *global* effect chain (session 0,
//! on Motorola phones Dolby's DAP) holds a software effect. AudioPolicy keeps that chain on the output that plays music,
//! and moves it to the deep-buffer output while a stream plays there. So we briefly play a silent ordinary stream, which
//! goes to deep buffer and takes the chain along, and open the fast track meanwhile; it then keeps running after the
//! chain has come back. The price: global effects are not applied to fast tracks, so DAP does not process our sound.
//!
//! `supervise` owns the stream while the app runs: it keeps trying for the fast path on a timer and after a return to the
//! app, moves the machine across without a break when it gets it (out.rs), and opens a new stream when the old one is
//! disconnected. There is only ever one stream sounding. The machine keeps playing while the app is away; since the
//! system then gives the audio thread less CPU, and the transitions themselves are busy, the buffer is held at its
//! deepest from pause until two seconds after resume.
use crate::out::{Handoff, Out, Source};
use crate::play::{Render, Shared};
use std::ffi::{c_char, c_void, CStr};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicU64, Ordering::Relaxed};
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
pub struct Options { pub probe: bool, pub stats: bool, pub bursts: i32, forced: Option<Spec>, nokick: bool, delay_ms: Option<u64>, fast: bool }
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
                "nokick" => { o.nokick = true; force = false }
                "fast" => { o.fast = true; force = false }
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
                    else if let Some(n) = num("delay=") { o.delay_ms = Some(n.max(0) as u64); force = false }
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
    min_frames: AtomicU32, max_frames: AtomicU32, worst_pct: AtomicU32, worst_us: AtomicU32, ahead_us: AtomicU32, xruns: AtomicI32, buffer: AtomicI32, want: AtomicI32, hold: AtomicBool, pad: AtomicU64, dead: AtomicBool,
}
static CURRENT: std::sync::Mutex<Option<Arc<Stats>>> = std::sync::Mutex::new(None);

pub struct Live { render: Render, sh: Arc<Shared> }
impl Source for Live { fn block(&mut self, l: &mut [f32], r: &mut [f32], latency: f64) { self.render.block(l, r, latency) } }

struct Cb {
    out: Out<Live>, stats: Arc<Stats>, written: i64, sr: f64, format: i32, channels: usize,
    burst: i32, capacity: i32, buffer: i32, xruns: i32, win_wall: u64, win_cpu: u64, win_peak: u32,
}

extern "C" fn on_error(_s: S, user: *mut c_void, e: i32) {
    // only the shared counters: the data callback may be running
    let stats = unsafe { &*(user as *const Stats) };
    stats.dead.store(true, Relaxed);
    log::warn!("audio stream error: {}", err_text(e));
}

extern "C" fn on_data(s: S, user: *mut c_void, data: *mut c_void, frames: i32) -> i32 {
    let c = unsafe { &mut *(user as *mut Cb) };
    let (wall0, cpu0) = (now(libc::CLOCK_MONOTONIC), now(libc::CLOCK_THREAD_CPUTIME_ID));
    let n = frames.max(0) as usize;
    let ch = c.channels.max(1);
    // What is queued ahead of the speaker: the timestamp says which frame played when; carry it forward to now.
    let (mut pos, mut ts) = (0i64, 0i64);
    let ahead = if unsafe { AAudioStream_getTimestamp(s, CLOCK_MONOTONIC, &mut pos, &mut ts) } == 0 {
        let played = pos as f64 + (wall0 as f64 - ts as f64) * 1e-9 * c.sr;
        ((c.written as f64 - played) / c.sr).max(0.)
    } else { c.buffer as f64 / c.sr };
    if c.format == FORMAT_I16 {
        let out = unsafe { std::slice::from_raw_parts_mut(data as *mut i16, n * ch) };
        let conv = |v: f32| (v.clamp(-1., 1.) * 32767.).round() as i16;
        c.out.render(n, ahead, wall0, |i, l, r| for k in 0..ch { out[i * ch + k] = conv(if k % 2 == 0 { l } else { r }) });
    } else {
        let out = unsafe { std::slice::from_raw_parts_mut(data as *mut f32, n * ch) };
        c.out.render(n, ahead, wall0, |i, l, r| for k in 0..ch { out[i * ch + k] = if k % 2 == 0 { l } else { r } });
    }
    c.written += n as i64;
    c.out.latency = ahead + n as f64 / c.sr;
    let st = &c.stats;
    st.ahead_us.store((c.out.latency * 1e6) as u32, Relaxed);
    // A glitch in normal running: one more burst of buffer, up to what the stream has.
    let x = unsafe { AAudioStream_getXRunCount(s) };
    let mut want = st.want.load(Relaxed);
    if x > c.xruns && !st.hold.load(Relaxed) && want + c.burst <= c.capacity { want += c.burst; st.want.store(want, Relaxed) }
    if want > 0 && want != c.buffer { let got = unsafe { AAudioStream_setBufferSizeInFrames(s, want) }; if got > 0 { c.buffer = got } }
    c.xruns = x;
    let (wall1, cpu1) = (now(libc::CLOCK_MONOTONIC), now(libc::CLOCK_THREAD_CPUTIME_ID));
    st.callbacks.fetch_add(1, Relaxed); st.frames.fetch_add(n as u64, Relaxed); st.in_cpu_ns.fetch_add(cpu1 - cpu0, Relaxed); st.in_wall_ns.fetch_add(wall1 - wall0, Relaxed);
    st.thread_cpu_ns.store(cpu1, Relaxed); st.xruns.store(x, Relaxed); st.buffer.store(c.buffer, Relaxed); st.pad.store(c.out.last_pad, Relaxed);
    st.max_frames.fetch_max(n as u32, Relaxed);
    let mn = st.min_frames.load(Relaxed); if mn == 0 || (n as u32) < mn { st.min_frames.store(n as u32, Relaxed) }
    let pct = ((wall1 - wall0) as f64 / (n.max(1) as f64 / c.sr * 1e9) * 100.) as u32;
    st.worst_pct.fetch_max(pct, Relaxed);
    st.worst_us.fetch_max(((wall1 - wall0) / 1000) as u32, Relaxed);
    c.win_peak = c.win_peak.max(pct);
    // The meters: the thread's whole CPU over half a second (AAudio's own work between callbacks included).
    if c.win_wall == 0 { c.win_wall = wall1; c.win_cpu = cpu1 }
    else if wall1 - c.win_wall >= 500_000_000 {
        if let Some(l) = &c.out.src {
            let m = &l.sh.meters;
            m.busy.store(((cpu1 - c.win_cpu) as f64 / (wall1 - c.win_wall) as f64 * 100.).round() as u32, Relaxed);
            m.busy_peak.store(c.win_peak, Relaxed);
            m.latency_us.store((c.out.latency * 1e6) as u32, Relaxed);
            m.xruns.store(x.max(0) as u32, Relaxed);
            m.buffer.store(c.buffer as u32, Relaxed);
        }
        c.win_wall = wall1; c.win_cpu = cpu1; c.win_peak = 0;
    }
    0
}

/// An open stream and what its callback owns.
pub struct Stream { s: S, cb: Box<Cb>, stats: Arc<Stats>, handoff: Arc<Handoff<Live>>, pub info: Info, open: bool, normal_buffer: i32, held: bool }
unsafe impl Send for Stream {}
impl Stream {
    fn close(&mut self) { if self.open { self.open = false; unsafe { AAudioStream_requestStop(self.s); AAudioStream_close(self.s); } } }
    fn fast(&self) -> bool { self.info.perf == PERF_LOW }
    /// Closed, and the machine back out of it, wherever it had got to.
    fn take_live(mut self) -> Option<Box<Live>> {
        self.close();
        self.cb.out.src.take().or_else(|| self.cb.out.handoff.take().map(|x| x.0)).or_else(|| self.cb.out.incoming.take().and_then(|h| h.take()).map(|x| x.0))
    }
    fn publish(&self, sh: &Shared) {
        let m = &sh.meters;
        m.sample_rate.store(self.info.rate as u32, Relaxed); m.burst.store(self.info.burst as u32, Relaxed); m.buffer.store(self.info.buffer as u32, Relaxed);
        if let Ok(mut c) = CURRENT.lock() { *c = Some(self.stats.clone()) }
    }
    fn set_bursts(&mut self, bursts: i32) {
        let got = unsafe { AAudioStream_setBufferSizeInFrames(self.s, self.info.burst * bursts.max(1)) };
        if got > 0 { self.cb.buffer = got; self.info.buffer = got }
        self.normal_buffer = self.info.buffer;
        self.stats.want.store(self.info.buffer, Relaxed);
    }
    /// The deepest buffer while the app is away or just back; the normal one (with any growth it earned) otherwise.
    fn set_hold(&mut self, hold: bool) {
        if hold == self.held { return }
        self.held = hold;
        let st = &self.stats;
        if hold {
            self.normal_buffer = st.want.load(Relaxed).max(self.info.buffer);
            st.hold.store(true, Relaxed);
            st.want.store(self.info.capacity.min((self.info.rate / 10).max(self.normal_buffer)), Relaxed);
        } else {
            st.want.store(self.normal_buffer, Relaxed);
            st.hold.store(false, Relaxed);
        }
        log::info!("audio buffer {} frames ({})", st.want.load(Relaxed), if hold { "held deep" } else { "normal" });
    }
    fn begin(&mut self) -> bool {
        let r = unsafe { AAudioStream_requestStart(self.s) };
        if r != 0 { log::error!("audio start failed: {}", err_text(r)) }
        r == 0
    }
}
impl Drop for Stream { fn drop(&mut self) { self.close() } }

fn open(spec: &Spec) -> Result<Stream, String> { open_at(spec, 48000.) }
fn open_at(spec: &Spec, sr_guess: f64) -> Result<Stream, String> {
    let stats = Arc::new(Stats::default());
    let mut cb = Box::new(Cb { out: Out::new(sr_guess), stats: stats.clone(), written: 0, sr: sr_guess, format: 0, channels: 2,
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
        AAudioStreamBuilder_setDataCallback(b, on_data, &mut *cb as *mut Cb as *mut c_void);
        AAudioStreamBuilder_setErrorCallback(b, on_error, Arc::as_ptr(&stats) as *mut c_void);
        let mut s: S = std::ptr::null_mut();
        let r = AAudioStreamBuilder_openStream(b, &mut s);
        AAudioStreamBuilder_delete(b);
        if r != 0 { return Err(err_text(r)) }
        let info = Info { rate: AAudioStream_getSampleRate(s), channels: AAudioStream_getChannelCount(s), format: AAudioStream_getFormat(s), sharing: AAudioStream_getSharingMode(s),
            perf: AAudioStream_getPerformanceMode(s), burst: AAudioStream_getFramesPerBurst(s), buffer: AAudioStream_getBufferSizeInFrames(s),
            capacity: AAudioStream_getBufferCapacityInFrames(s), per_callback: AAudioStream_getFramesPerDataCallback(s), device: AAudioStream_getDeviceId(s), mmap: mmap_used(s) };
        if info.rate as f64 != sr_guess { cb.out = Out::new(info.rate as f64) }
        cb.sr = info.rate as f64; cb.format = info.format; cb.channels = info.channels.max(1) as usize; cb.burst = info.burst; cb.capacity = info.capacity; cb.buffer = info.buffer;
        let handoff = cb.out.handoff.clone();
        Ok(Stream { s, cb, stats, handoff, info, open: true, normal_buffer: 0, held: false })
    }
}

/// Try for a fast track: asked for plainly, then while a silent deep-buffer stream holds the global effect chain away
/// from the fast output (see the top of this file).
fn open_fast(rate: i32, kick: bool, why: &str) -> Option<Stream> {
    let spec = Spec::new(FORMAT_FLOAT, SHARED, rate, USAGE_MEDIA);
    let t = std::time::Instant::now();
    let attempt = |how: &str| match open(&spec) {
        Ok(s) if s.fast() => { log::info!("audio fast path ({why}, {how}, {:.0} ms): {}", t.elapsed().as_secs_f64() * 1000., s.info.line()); Some(s) }
        Ok(_) => { log::info!("audio fast path ({why}, {how}): refused"); None }
        Err(e) => { log::info!("audio fast path ({why}, {how}): {e}"); None }
    };
    if let Some(s) = attempt("plain") { return Some(s) }
    if !kick { return None }
    let mut holder = open(&Spec { perf: PERF_NONE, ..Spec::new(FORMAT_FLOAT, SHARED, rate, USAGE_MEDIA) }).ok()?;
    if !holder.begin() { return None }
    for wait in [40, 100, 250] {
        std::thread::sleep(std::time::Duration::from_millis(wait));
        if let Some(s) = attempt(&format!("deep-buffer stream up {wait} ms more")) { return Some(s) }
    }
    None
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
    if o.fast && o.forced.is_none() && !o.nokick && o.delay_ms.is_none() {
        if let Some(s) = open_fast(rate.unwrap_or(0), true, "launch") { return Some(s) }
    }
    let (spec, _) = best?;
    let s = open(&spec).map_err(|e| log::error!("audio reopen [{}]: {e}", spec.label())).ok()?;
    log::info!("audio chosen [{}]: {}", spec.label(), s.info.line());
    Some(s)
}

/// Runs the output for as long as the app does. Started with what `open_best` found (none while probing).
pub fn supervise(first: Option<Stream>, render: Render, sh: Arc<Shared>, o: Options, rate: i32, on_fail: impl Fn() + Send + 'static) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new().name("cogs-audio".into()).spawn(move || {
        let launch = std::time::Instant::now();
        let since = || launch.elapsed().as_secs_f64();
        let mut live = Some(Box::new(Live { render, sh: sh.clone() }));
        let mut cur = first;
        if o.probe {
            std::thread::sleep(std::time::Duration::from_secs(3));
            probe("late", &o);
            log::info!("audio probe done");
            cur = open_best(&o, Some(rate));
        }
        let start = |mut s: Stream, l: Box<Live>| -> Result<Stream, Box<Live>> {
            s.cb.out.src = Some(l);
            s.set_bursts(o.bursts); s.publish(&sh);
            if s.begin() { log::info!("audio started at {:.2} s: buffer {} frames ({:.1} ms) on {}", since(), s.info.buffer, s.info.buffer as f64 * 1000. / s.info.rate.max(1) as f64, s.info.line()); Ok(s) }
            else { Err(s.take_live().expect("the machine was just put here")) }
        };
        let mut running: Option<Stream> = None;
        if let Some(s) = cur.take() { match start(s, live.take().unwrap()) { Ok(s) => running = Some(s), Err(l) => live = Some(l) } }
        if running.is_none() { on_fail() }
        // when to try for the fast path again, seconds after launch
        let mut tries: Vec<f64> = o.delay_ms.iter().map(|d| *d as f64 / 1000.).chain([1., 2., 4., 8.]).collect();
        tries.sort_by(f64::total_cmp); tries.dedup();
        let (mut reopen_at, mut settle_until) = (0f64, 0f64);
        let earliest = o.delay_ms.map_or(0., |d| d as f64 / 1000.);
        while sh.running.load(Relaxed) {
            std::thread::sleep(std::time::Duration::from_millis(50));
            let t = since();
            // gone (headphones, Bluetooth, a route change), or never opened: open again, with the machine where it was
            if running.as_ref().map_or(true, |s| s.stats.dead.load(Relaxed)) {
                if let Some(s) = running.take() { log::warn!("audio stream lost at {t:.2} s, reopening"); if let Some(l) = s.take_live() { live = Some(l) } }
                if t < reopen_at { continue }
                reopen_at = t + 0.5;
                if let (Some(s), Some(l)) = (open_best(&o, Some(rate)), live.take()) {
                    match start(s, l) { Ok(s) => { running = Some(s); tries = vec![t + 1., t + 2., t + 4.] } Err(l) => live = Some(l) }
                }
                continue;
            }
            // away and just back: the deep buffer, and no stream changes while the system is busy with the transition
            let away = AWAY.load(Relaxed);
            if away { settle_until = t + 2. }
            // `delay` holds back every try, this one too
            if RESUMED.swap(false, Relaxed) { tries.push((t + 1.).max(earliest)); tries.sort_by(f64::total_cmp) }
            let Some(cur) = running.as_mut() else { continue };
            cur.set_hold(away || t < settle_until);
            if !o.fast || o.forced.is_some() || o.probe || cur.fast() { tries.clear(); continue }
            if away || tries.first().is_none_or(|x| *x > t) { continue }
            tries.remove(0);
            let Some(mut next) = open_fast(cur.info.rate, !o.nokick, &format!("retry at {t:.2} s")) else { continue };
            // the new stream runs silent until the old one hands over at the right moment
            let old = running.take().unwrap();
            next.cb.out.incoming = Some(old.handoff.clone());
            next.set_bursts(o.bursts);
            if !next.begin() { running = Some(old); continue }
            std::thread::sleep(std::time::Duration::from_millis(150));
            old.handoff.request();
            let t1 = std::time::Instant::now();
            while !old.handoff.taken() && t1.elapsed().as_millis() < 1500 && !old.stats.dead.load(Relaxed) && !next.stats.dead.load(Relaxed) { std::thread::sleep(std::time::Duration::from_millis(5)) }
            if !old.handoff.taken() {
                log::warn!("audio hand-over did not complete");
                // the machine is in one of the two: keep it, on a fresh start of whichever is alive
                let l = next.take_live().or_else(|| old.take_live());
                live = l; continue;
            }
            let end = old.handoff.end_ns();
            next.publish(&sh);
            log::info!("audio moved to the fast path at {:.2} s: {} frames of silence waited for the old stream; {}", since(), next.stats.pad.load(Relaxed), next.info.line());
            running = Some(next);
            // the old stream plays out what it has queued, then goes
            let wait = end.saturating_sub(now(libc::CLOCK_MONOTONIC)) / 1_000_000 + 80;
            std::thread::sleep(std::time::Duration::from_millis(wait.min(1000)));
            drop(old);
        }
    }).expect("audio thread")
}
static RESUMED: AtomicBool = AtomicBool::new(false);
static AWAY: AtomicBool = AtomicBool::new(false);
/// The activity's lifecycle: 1 resume, 2 pause, 3 start, 4 stop.
pub fn lifecycle(what: i32) {
    match what { 1 => { AWAY.store(false, Relaxed); RESUMED.store(true, Relaxed) } 2 => AWAY.store(true, Relaxed), _ => {} }
    let x = CURRENT.lock().ok().and_then(|c| c.clone()).map_or(0, |s| s.xruns.load(Relaxed));
    log::info!("audio lifecycle: {} (xruns so far {x})", ["?", "resume", "pause", "start", "stop"][what.clamp(0, 4) as usize]);
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
        let mut s = s;
        if !s.begin() { log::info!("audio probe {when} {i:2} [{}]: {} / start failed", spec.label(), s.info.line()); continue }
        std::thread::sleep(std::time::Duration::from_millis(250));
        let st = &s.stats;
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
        struct P { st: Arc<Stats>, t: std::time::Instant, cpu: u64, in_cpu: u64, in_wall: u64, cbs: u64, frames: u64, blocks: u64, misses: u64 }
        let mut last: Option<P> = None;
        while sh.running.load(Relaxed) {
            std::thread::sleep(std::time::Duration::from_secs(2));
            let Some(st) = CURRENT.lock().ok().and_then(|c| c.clone()) else { continue };
            let m = &sh.meters;
            let cur = P { st: st.clone(), t: std::time::Instant::now(), cpu: st.thread_cpu_ns.load(Relaxed), in_cpu: st.in_cpu_ns.load(Relaxed), in_wall: st.in_wall_ns.load(Relaxed),
                cbs: st.callbacks.load(Relaxed), frames: st.frames.load(Relaxed), blocks: m.blocks.load(Relaxed), misses: m.lock_misses.load(Relaxed) };
            if let Some(p) = last.as_ref().filter(|p| Arc::ptr_eq(&p.st, &st)) {
                let ns = (cur.t - p.t).as_secs_f64() * 1e9;
                let pc = |a: u64, b: u64| a.saturating_sub(b) as f64 / ns * 100.;
                let cbs = (cur.cbs - p.cbs).max(1);
                log::info!("audio stats: cpu {:.1}% (callback {:.1}%, wall {:.1}%) | {} cbs, {} frames ({}..{}) | worst {:.2} ms = {}% | ahead {:.1} ms, buffer {} | xruns {} | world locked {} of {} blocks, owed max {:.1} ms",
                    pc(cur.cpu, p.cpu), pc(cur.in_cpu, p.in_cpu), pc(cur.in_wall, p.in_wall), cur.cbs - p.cbs, (cur.frames - p.frames) / cbs, st.min_frames.swap(0, Relaxed), st.max_frames.swap(0, Relaxed),
                    st.worst_us.swap(0, Relaxed) as f64 / 1000., st.worst_pct.swap(0, Relaxed), st.ahead_us.load(Relaxed) as f64 / 1000., st.buffer.load(Relaxed), st.xruns.load(Relaxed),
                    cur.misses - p.misses, cur.blocks - p.blocks, m.owed_max_us.swap(0, Relaxed) as f64 / 1000.);
            }
            last = Some(cur);
        }
    });
}
