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
//!     --es audio fast,stats          the fast path, fed by the render thread (below)
//!     --es audio fast,direct,stats   the fast path with the machine rendered in the callback itself
//!     --es audio bench               time the machine on each kind of CPU core, at 4 ms and 20 ms blocks, then run
//!     more words: world=N (start on shipped world N), ring=N and chunk=N (frames), look=N (machine lookahead, frames),
//!     nohint (no performance hint session)
//!
//! A fast track's callback must return within about 4 ms and is scheduled by the system as a real-time thread, which
//! need not run on a fast core. So the machine does not render there: a render thread makes audio in 5 ms chunks into a
//! lock-free ring a little ahead, and the callback only copies. The ring's depth is the added latency. (`direct` renders
//! in the callback instead.)
//!
//! Why a fast track can be refused: AudioFlinger will not make one on an output whose *global* effect chain (session 0,
//! on Motorola phones Dolby's DAP) holds a software effect. AudioPolicy keeps that chain on the output that plays music,
//! and moves it to the deep-buffer output while a stream plays there. So we briefly play a silent ordinary stream, which
//! goes to deep buffer and takes the chain along, and open the fast track meanwhile; it then keeps running after the
//! chain has come back. The price: global effects are not applied to fast tracks, so DAP does not process our sound.
//!
//! `supervise` owns the stream while the app runs: it keeps trying for the fast path on a timer and after a return to the
//! app, moves the machine across without a break when it gets it (out.rs), and opens a new stream when the old one is
//! disconnected. There is only ever one stream sounding.
//!
//! Out of sight the machine keeps playing (under PlaybackService). Latency no longer matters there and scheduling is
//! worse, so from pause until two seconds after resume the stream buffer is at its deepest and the ring holds 150 ms,
//! rendered in bursts so the thread wakes rarely. Coming back, the ring plays down to its normal depth by itself:
//! nothing is skipped or repeated. Out of sight and paused, the stream is closed and everything idles until play or return.
use crate::out::{Handoff, Out, Source};
use crate::play::{self, Render, Shared};
use rtrb::{Consumer, Producer, RingBuffer};
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
pub struct Options { pub probe: bool, pub stats: bool, pub bursts: i32, forced: Option<Spec>, nokick: bool, delay_ms: Option<u64>, fast: bool, direct: bool, nohint: bool, pub bench: bool, pub world: Option<usize>, ring: Option<u32>, chunk: Option<u32>, look: Option<u64> }
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
                "direct" => { o.direct = true; force = false }
                "nohint" => { o.nohint = true; force = false }
                "bench" => { o.bench = true; force = false }
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
                    else if let Some(n) = num("world=") { o.world = Some(n.max(0) as usize); force = false }
                    else if let Some(n) = num("ring=") { o.ring = Some(n.max(0) as u32); force = false }
                    else if let Some(n) = num("chunk=") { o.chunk = Some(n.max(32) as u32); force = false }
                    else if let Some(n) = num("look=") { o.look = Some(n.max(0) as u64); force = false }
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
    min_frames: AtomicU32, max_frames: AtomicU32, worst_pct: AtomicU32, worst_us: AtomicU32, ahead_us: AtomicU32, xruns: AtomicI32, buffer: AtomicI32, want: AtomicI32, hold: AtomicBool, pad: AtomicU64, dead: AtomicBool, cores: AtomicU32, mute: AtomicBool,
}
static CURRENT: std::sync::Mutex<Option<Arc<Stats>>> = std::sync::Mutex::new(None);

/// What the render thread and the callback share.
#[derive(Default)]
struct Feed {
    /// keep this many frames queued (the ring's depth, the latency it adds)
    target: AtomicU32, seq: AtomicU32, sleeping: AtomicBool,
    /// wake the render thread when the ring falls below this; it then fills up to `target`
    low: AtomicU32,
    /// the target in normal running, and whether the thread may sleep long (nothing is being consumed)
    normal: AtomicU32, parked: AtomicBool,
    starved: AtomicU64, min_fill: AtomicU32, ahead_us: AtomicU32,
    cpu_ns: AtomicU64, chunks: AtomicU64, worst_us: AtomicU32, cores: AtomicU32,
}
static CURRENT_FEED: std::sync::Mutex<Option<Arc<Feed>>> = std::sync::Mutex::new(None);
const FUTEX_WAIT_PRIVATE: i32 = 128;
const FUTEX_WAKE_PRIVATE: i32 = 129;
fn core_bit() -> u32 { 1u32 << (unsafe { libc::sched_getcpu() }.clamp(0, 31)) }

enum Machine { Here(Render), Ring { rx: Consumer<f32>, feed: Arc<Feed> } }
pub struct Live { machine: Machine, sh: Arc<Shared> }
impl Source for Live {
    fn block(&mut self, l: &mut [f32], r: &mut [f32], latency: f64) {
        match &mut self.machine {
            Machine::Here(render) => render.block(l, r, latency),
            Machine::Ring { rx, feed } => {
                let n = l.len();
                let have = rx.slots() / 2;
                let take = have.min(n);
                if let Ok(c) = rx.read_chunk(take * 2) {
                    let (a, b) = c.as_slices();
                    for (i, x) in a.iter().chain(b).enumerate() { if i % 2 == 0 { l[i / 2] = *x } else { r[i / 2] = *x } }
                    c.commit_all();
                }
                if take < n { l[take..].fill(0.); r[take..].fill(0.); feed.starved.fetch_add(1, Relaxed); }
                let left = (have - take) as u32;
                feed.min_fill.fetch_min(left, Relaxed);
                feed.ahead_us.store((latency * 1e6) as u32, Relaxed);
                if left < feed.low.load(Relaxed) {
                    feed.seq.fetch_add(1, Relaxed);
                    if feed.sleeping.load(Relaxed) { unsafe { libc::syscall(libc::SYS_futex, feed.seq.as_ptr(), FUTEX_WAKE_PRIVATE, 1) }; }
                }
            }
        }
    }
}

impl Feed {
    fn set_normal(&self, v: u32) { self.normal.store(v, Relaxed); self.target.store(v, Relaxed); self.low.store(v, Relaxed) }
    /// A callback takes `burst` frames at a time: keep that much queued beyond the usual margin.
    fn fit(&self, burst: u32, chunk: u32, asked: Option<u32>) { let v = asked.unwrap_or(if burst > chunk { burst + chunk * 2 } else { chunk * 2 }).max(chunk); self.set_normal(v) }
    /// Out of sight: 150 ms queued, topped up in bursts of 50 ms. Otherwise the normal depth.
    fn deep(&self, deep: bool, rate: i32) {
        if deep { let t = (rate as u32 * 3 / 20).max(self.normal.load(Relaxed)); self.target.store(t, Relaxed); self.low.store(t.saturating_sub(rate as u32 / 20), Relaxed) }
        else { let n = self.normal.load(Relaxed); self.target.store(n, Relaxed); self.low.store(n, Relaxed) }
    }
}

/// Android's hint to the scheduler that this thread has deadlines (API 33+): it then gets the cores and clocks to meet them.
struct Hint { session: *mut c_void, report: extern "C" fn(*mut c_void, i64) -> i32 }
impl Hint {
    fn new(target_ns: i64) -> Option<Hint> {
        unsafe {
            let h = libc::dlopen(c"libandroid.so".as_ptr(), libc::RTLD_NOW);
            if h.is_null() { return None }
            let sym = |n: &CStr| libc::dlsym(h, n.as_ptr());
            let (m, c, r) = (sym(c"APerformanceHint_getManager"), sym(c"APerformanceHint_createSession"), sym(c"APerformanceHint_reportActualWorkDuration"));
            if m.is_null() || c.is_null() || r.is_null() { return None }
            let get: extern "C" fn() -> *mut c_void = std::mem::transmute(m);
            let create: extern "C" fn(*mut c_void, *const i32, usize, i64) -> *mut c_void = std::mem::transmute(c);
            let tid = libc::gettid();
            let mgr = get();
            if mgr.is_null() { return None }
            let session = create(mgr, &tid, 1, target_ns);
            if session.is_null() { None } else { Some(Hint { session, report: std::mem::transmute(r) }) }
        }
    }
}

/// The render thread: keeps the ring at its target, a chunk at a time.
fn render_thread(mut render: Render, mut tx: Producer<f32>, feed: Arc<Feed>, sh: Arc<Shared>, chunk: usize, sr: f64, hint: bool) {
    let nice = unsafe { libc::setpriority(libc::PRIO_PROCESS, 0, -19) };
    let h = if hint { Hint::new((chunk as f64 / sr * 0.5e9) as i64) } else { None };
    log::info!("audio render thread: {chunk} frames a chunk, priority {}, performance hint {}", if nice == 0 { "-19" } else { "unchanged" }, if h.is_some() { "on" } else { "off" });
    let (mut l, mut r) = (vec![0f32; chunk], vec![0f32; chunk]);
    let period = chunk as f64 / sr;
    let mut filling = true;
    while sh.running.load(Relaxed) {
        let seq = feed.seq.load(Relaxed);
        let queued = (tx.buffer().capacity() - tx.slots()) / 2;
        let goal = if filling { feed.target.load(Relaxed) } else { feed.low.load(Relaxed) } as usize;
        if queued >= goal || tx.slots() < chunk * 2 {
            filling = false;
            feed.sleeping.store(true, Relaxed);
            let ts = if feed.parked.load(Relaxed) { libc::timespec { tv_sec: 0, tv_nsec: 500_000_000 } } else { libc::timespec { tv_sec: 0, tv_nsec: 20_000_000 } };
            unsafe { libc::syscall(libc::SYS_futex, feed.seq.as_ptr(), FUTEX_WAIT_PRIVATE, seq, &ts as *const libc::timespec) };
            feed.sleeping.store(false, Relaxed);
            continue;
        }
        filling = true;
        let (w0, c0) = (now(libc::CLOCK_MONOTONIC), now(libc::CLOCK_THREAD_CPUTIME_ID));
        // these frames reach the speaker after what is queued and what the stream holds
        let latency = feed.ahead_us.load(Relaxed) as f64 / 1e6 + queued as f64 / sr + period;
        render.block(&mut l, &mut r, latency);
        if let Ok(mut c) = tx.write_chunk(chunk * 2) {
            let (a, b) = c.as_mut_slices();
            for (i, x) in a.iter_mut().chain(b).enumerate() { *x = if i % 2 == 0 { l[i / 2] } else { r[i / 2] } }
            c.commit_all();
        }
        let (w1, c1) = (now(libc::CLOCK_MONOTONIC), now(libc::CLOCK_THREAD_CPUTIME_ID));
        feed.cpu_ns.fetch_add(c1 - c0, Relaxed); feed.chunks.fetch_add(1, Relaxed); feed.worst_us.fetch_max(((w1 - w0) / 1000) as u32, Relaxed); feed.cores.fetch_or(core_bit(), Relaxed);
        if let Some(h) = &h { (h.report)(h.session, (w1 - w0) as i64); }
    }
}

/// How the machine runs on the phone: `live` for the stream, and the render thread if it has one.
fn machine(mut render: Render, sh: &Arc<Shared>, o: &Options, rate: i32) -> (Box<Live>, Option<(std::thread::JoinHandle<()>, Arc<Feed>, u32)>) {
    let sr = rate as f64;
    if o.direct {
        render.set_lookahead(o.look.unwrap_or((sr * 0.02) as u64));
        return (Box::new(Live { machine: Machine::Here(render), sh: sh.clone() }), None);
    }
    let chunk = o.chunk.unwrap_or((sr * 0.005) as u32);
    let target = o.ring.unwrap_or(chunk * 2).max(chunk);
    render.set_lookahead(o.look.unwrap_or((sr * 0.016) as u64));
    let (tx, rx) = RingBuffer::<f32>::new(2 * (sr * 0.2) as usize);
    let feed = Arc::new(Feed::default());
    feed.set_normal(target); feed.min_fill.store(u32::MAX, Relaxed);
    if let Ok(mut c) = CURRENT_FEED.lock() { *c = Some(feed.clone()) }
    let (f, s, hint) = (feed.clone(), sh.clone(), !o.nohint);
    let j = std::thread::Builder::new().name("cogs-render".into()).spawn(move || render_thread(render, tx, f, s, chunk as usize, sr, hint)).expect("render thread");
    (Box::new(Live { machine: Machine::Ring { rx, feed: feed.clone() }, sh: sh.clone() }), Some((j, feed, chunk)))
}

/// Time the machine and sound on each kind of core, paced like the real thing, so the clocks behave as they do in use.
pub fn bench() {
    let max_freq = |c: usize| std::fs::read_to_string(format!("/sys/devices/system/cpu/cpu{c}/cpufreq/cpuinfo_max_freq")).ok().and_then(|s| s.trim().parse::<u64>().ok());
    let n = unsafe { libc::sysconf(libc::_SC_NPROCESSORS_CONF) }.max(1) as usize;
    let mut picks: Vec<(usize, u64)> = Vec::new();
    for c in (0..n).rev() { let f = max_freq(c).unwrap_or(c as u64); if !picks.iter().any(|p| p.1 == f) { picks.push((c, f)) } }
    log::info!("audio bench: {n} cores, one of each kind (core, max kHz): {picks:?}");
    let t = std::thread::spawn(move || for &(core, khz) in &picks {
        unsafe { let mut s: libc::cpu_set_t = std::mem::zeroed(); libc::CPU_SET(core, &mut s); libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &s); }
        for (name, src) in crate::WORLDS {
            for frames in [192usize, 960] {
                let Ok(mut w) = cogs_sim::World::from_str(src) else { continue };
                w.set_paused(false);
                let (sh, mut render, mut sync) = play::parts(w, 48000.);
                sync.step(&sh);
                let (mut l, mut r) = (vec![0f32; frames], vec![0f32; frames]);
                let period = (frames as f64 / 48000. * 1e9) as u64;
                let (mut next, c0, w0, mut worst, mut count) = (now(libc::CLOCK_MONOTONIC), now(libc::CLOCK_THREAD_CPUTIME_ID), now(libc::CLOCK_MONOTONIC), 0u64, 0u32);
                while now(libc::CLOCK_MONOTONIC) - w0 < 1_500_000_000 {
                    let b0 = now(libc::CLOCK_MONOTONIC);
                    render.block(&mut l, &mut r, 0.01);
                    worst = worst.max(now(libc::CLOCK_MONOTONIC) - b0); count += 1;
                    if count % 5 == 0 { sync.step(&sh) }
                    next += period;
                    let d = next.saturating_sub(now(libc::CLOCK_MONOTONIC));
                    if d > 0 { std::thread::sleep(std::time::Duration::from_nanos(d)) }
                }
                let wall = now(libc::CLOCK_MONOTONIC) - w0;
                log::info!("audio bench: core {core} ({khz} kHz) {name}, {frames}-frame blocks: {:.1}% of the core, worst block {:.2} ms of {:.1}",
                    (now(libc::CLOCK_THREAD_CPUTIME_ID) - c0) as f64 / wall as f64 * 100., worst as f64 / 1e6, period as f64 / 1e6);
            }
        }
    });
    let _ = t.join();
}

struct Cb {
    out: Out<Live>, stats: Arc<Stats>, gain: f32, written: i64, sr: f64, format: i32, channels: usize,
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
    // Stop: down to silence in 10 ms, so it is not a click
    let (mute, step) = (c.stats.mute.load(Relaxed), 1. / (c.sr as f32 * 0.01));
    let mut g = c.gain;
    let mut gain = |v: f32| { if mute { g = (g - step / ch as f32 / 2.).max(0.) } v * g };
    if c.format == FORMAT_I16 {
        let out = unsafe { std::slice::from_raw_parts_mut(data as *mut i16, n * ch) };
        let conv = |v: f32| (v.clamp(-1., 1.) * 32767.).round() as i16;
        c.out.render(n, ahead, wall0, |i, l, r| for k in 0..ch { out[i * ch + k] = conv(gain(if k % 2 == 0 { l } else { r })) });
    } else {
        let out = unsafe { std::slice::from_raw_parts_mut(data as *mut f32, n * ch) };
        c.out.render(n, ahead, wall0, |i, l, r| for k in 0..ch { out[i * ch + k] = gain(if k % 2 == 0 { l } else { r }) });
    }
    c.gain = g;
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
    st.max_frames.fetch_max(n as u32, Relaxed); st.cores.fetch_or(core_bit(), Relaxed);
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
    let mut cb = Box::new(Cb { out: Out::new(sr_guess), stats: stats.clone(), gain: 1., written: 0, sr: sr_guess, format: 0, channels: 2,
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
        if let Ok(mut w) = WAKER.lock() { *w = Some(std::thread::current()) }
        let (first_live, worker) = machine(render, &sh, &o, rate);
        let mut live = Some(first_live);
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
            if let Some((_, f, chunk)) = &worker { f.fit(s.info.burst as u32, *chunk, o.ring); f.parked.store(false, Relaxed) }
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
        // after a clean stretch the buffers come back down, a step at a time; a glitch soon after a step makes that the floor
        let mut tune = Tune::default();
        // out of sight and paused: the stream closed, everything idle
        let (mut parked, mut paused, mut stopping, mut paused_at, mut quiet_at, mut deep) = (false, false, false, 0f64, 0f64, false);
        while sh.running.load(Relaxed) {
            std::thread::park_timeout(std::time::Duration::from_millis(if parked { 1000 } else { 50 }));
            let t = since();
            // the notification's buttons
            let c = CONTROL.swap(0, Relaxed);
            if c != 0 {
                if let Ok(mut w) = sh.world.lock() { w.set_paused(c != 1); }
                if c == 3 { stopping = true; sh.midi(crate::midi::Midi::Panic) } else { stopping = false }
                log::info!("audio control at {t:.2} s: {}", ["", "play", "pause", "stop"][c as usize]);
            }
            if let Ok(w) = sh.world.try_lock() { let p = w.clock.paused; if p && !paused { paused_at = t } paused = p }
            let visible = VISIBLE.load(Relaxed);
            if sh.meters.peak_milli.load(Relaxed) > 0 { quiet_at = t }
            // park when out of sight and paused, once the tail has rung out (at once for Stop); wake on play or return
            let park = !visible && paused && (stopping || t - quiet_at > 0.3 || t - paused_at > 5.);
            if park && !parked {
                if let Some(s) = running.take() {
                    if stopping { s.stats.mute.store(true, Relaxed); std::thread::sleep(std::time::Duration::from_millis(30)) }
                    if let Some(l) = s.take_live() { live = Some(l) }
                }
                parked = true; sh.idle.store(true, Relaxed);
                if let Some((_, f, _)) = &worker { f.parked.store(true, Relaxed) }
                log::info!("audio parked at {t:.2} s");
            } else if parked && (visible || !paused) {
                parked = false; stopping = false; sh.idle.store(false, Relaxed); reopen_at = 0.;
                log::info!("audio waking at {t:.2} s");
            }
            if parked { continue }
            // gone (headphones, Bluetooth, a route change), or never opened: open again, with the machine where it was
            if running.as_ref().map_or(true, |s| s.stats.dead.load(Relaxed)) {
                if let Some(s) = running.take() { log::warn!("audio stream lost at {t:.2} s, reopening"); if let Some(l) = s.take_live() { live = Some(l) } }
                if t < reopen_at { continue }
                reopen_at = t + 0.5;
                if let (Some(s), Some(l)) = (open_best(&o, Some(rate)), live.take()) {
                    match start(s, l) { Ok(s) => { running = Some(s); deep = false; tries = vec![t + 1., t + 2., t + 4.] } Err(l) => live = Some(l) }
                }
                continue;
            }
            // away and just back: the deep buffer, and no stream changes while the system is busy with the transition
            let away = AWAY.load(Relaxed);
            if away { settle_until = t + 2. }
            // `delay` holds back every try, this one too
            if RESUMED.swap(false, Relaxed) { tries.push((t + 1.).max(earliest)); tries.sort_by(f64::total_cmp) }
            let Some(cur) = running.as_mut() else { continue };
            let holding = away || t < settle_until;
            cur.set_hold(holding);
            if holding != deep { deep = holding; if let Some((_, f, _)) = &worker { f.deep(holding, cur.info.rate) } }
            tune.step(t, cur, worker.as_ref().map(|w| (&*w.1, w.2)), holding);
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
            if let Some((_, f, chunk)) = &worker { f.fit(next.info.burst as u32, *chunk, o.ring) }
            log::info!("audio moved to the fast path at {:.2} s: {} frames of silence waited for the old stream; {}", since(), next.stats.pad.load(Relaxed), next.info.line());
            running = Some(next);
            // the old stream plays out what it has queued, then goes
            let wait = end.saturating_sub(now(libc::CLOCK_MONOTONIC)) / 1_000_000 + 80;
            std::thread::sleep(std::time::Duration::from_millis(wait.min(1000)));
            drop(old);
        }
        drop(running);
        if let Some((j, ..)) = worker { let _ = j.join(); }
    }).expect("audio thread")
}
#[derive(Default)]
struct Tune { stream: usize, xruns: i32, starved: u64, glitch_at: f64, step_at: f64, floor_buffer: i32, floor_target: u32 }
impl Tune {
    fn step(&mut self, t: f64, s: &Stream, feed: Option<(&Feed, u32)>, holding: bool) {
        let id = Arc::as_ptr(&s.stats) as usize;
        if id != self.stream { *self = Tune { stream: id, xruns: s.stats.xruns.load(Relaxed), glitch_at: t, step_at: t, floor_buffer: s.normal_buffer, floor_target: feed.map_or(0, |f| f.0.normal.load(Relaxed)), ..Tune::default() }; }
        let (x, starved) = (s.stats.xruns.load(Relaxed), feed.map_or(0, |f| f.0.starved.load(Relaxed)));
        let glitch = x > self.xruns || starved > self.starved;
        let starving = starved > self.starved;
        self.xruns = x; self.starved = starved;
        if holding { self.glitch_at = t; return }
        let want = s.stats.want.load(Relaxed);
        if glitch {
            if let (true, Some((f, chunk))) = (starving, feed) { let v = (f.normal.load(Relaxed) + chunk).min(chunk * 24); f.set_normal(v); log::info!("audio ring target {v} frames (starved)") }
            if t - self.step_at < 15. {
                self.floor_buffer = self.floor_buffer.max(want);
                if let Some((f, _)) = feed { self.floor_target = self.floor_target.max(f.normal.load(Relaxed)) }
                log::info!("audio buffers stay at {} frames, ring {} (a glitch soon after coming down)", self.floor_buffer, self.floor_target);
            }
            self.glitch_at = t;
        } else if t - self.glitch_at > 20. && t - self.step_at > 10. {
            self.step_at = t;
            if want - s.info.burst >= self.floor_buffer { s.stats.want.store(want - s.info.burst, Relaxed); log::info!("audio buffer back down to {} frames", want - s.info.burst) }
            else if let Some((f, chunk)) = feed { let v = f.normal.load(Relaxed); if v >= self.floor_target + chunk { f.set_normal(v - chunk); log::info!("audio ring back down to {} frames", v - chunk) } }
        }
    }
}
static RESUMED: AtomicBool = AtomicBool::new(false);
static AWAY: AtomicBool = AtomicBool::new(false);
static VISIBLE: AtomicBool = AtomicBool::new(true);
static CONTROL: AtomicI32 = AtomicI32::new(0);
static WAKER: std::sync::Mutex<Option<std::thread::Thread>> = std::sync::Mutex::new(None);
fn wake_supervisor() { if let Some(t) = WAKER.lock().ok().and_then(|w| w.clone()) { t.unpark() } }
/// The notification's and media session's buttons: 1 play, 2 pause, 3 stop.
pub fn control(what: i32) { CONTROL.store(what, Relaxed); wake_supervisor() }
/// The activity's lifecycle: 1 resume, 2 pause, 3 start, 4 stop.
pub fn lifecycle(what: i32) {
    match what {
        1 => { AWAY.store(false, Relaxed); RESUMED.store(true, Relaxed) }
        2 => AWAY.store(true, Relaxed),
        3 => VISIBLE.store(true, Relaxed),
        4 => VISIBLE.store(false, Relaxed),
        _ => {}
    }
    wake_supervisor();
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
        let mut fl = (0u64, 0u64);
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
                log::info!("audio stats: cpu {:.1}% (callback {:.1}%, wall {:.1}%) | {} cbs, {} frames ({}..{}) | worst {:.2} ms = {}% | ahead {:.1} ms, buffer {} | xruns {} | world locked {} of {} blocks, held back max {:.1} ms, late notes {} | callback cores {:#x}{}",
                    pc(cur.cpu, p.cpu), pc(cur.in_cpu, p.in_cpu), pc(cur.in_wall, p.in_wall), cur.cbs - p.cbs, (cur.frames - p.frames) / cbs, st.min_frames.swap(0, Relaxed), st.max_frames.swap(0, Relaxed),
                    st.worst_us.swap(0, Relaxed) as f64 / 1000., st.worst_pct.swap(0, Relaxed), st.ahead_us.load(Relaxed) as f64 / 1000., st.buffer.load(Relaxed), st.xruns.load(Relaxed),
                    cur.misses - p.misses, cur.blocks - p.blocks, m.owed_max_us.swap(0, Relaxed) as f64 / 1000., m.late_notes.load(Relaxed), st.cores.swap(0, Relaxed), feed_line(&mut fl, ns));
            }
            last = Some(cur);
        }
    });
}

/// The render thread's part of the stats line, and the cores' clocks.
fn feed_line(last: &mut (u64, u64), ns: f64) -> String {
    let mut s = String::new();
    if let Some(f) = CURRENT_FEED.lock().ok().and_then(|c| c.clone()) {
        let (cpu, chunks) = (f.cpu_ns.load(Relaxed), f.chunks.load(Relaxed));
        let min = f.min_fill.swap(u32::MAX, Relaxed);
        s = format!(" | render cpu {:.1}%, {} chunks, worst {:.2} ms, cores {:#x} | ring target {}, lowest {}, starved {}", cpu.saturating_sub(last.0) as f64 / ns * 100., chunks.saturating_sub(last.1),
            f.worst_us.swap(0, Relaxed) as f64 / 1000., f.cores.swap(0, Relaxed), f.target.load(Relaxed), if min == u32::MAX { 0 } else { min }, f.starved.load(Relaxed));
        *last = (cpu, chunks);
    }
    let mut mhz = Vec::new();
    for c in 0..8 { if let Some(k) = std::fs::read_to_string(format!("/sys/devices/system/cpu/cpu{c}/cpufreq/scaling_cur_freq")).ok().and_then(|x| x.trim().parse::<u64>().ok()) { mhz.push((k / 1000).to_string()) } }
    if !mhz.is_empty() { s += &format!(" | MHz {}", mhz.join(",")) }
    s
}
