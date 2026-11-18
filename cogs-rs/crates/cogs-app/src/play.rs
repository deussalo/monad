//! The machine and the sound, together on the audio thread.
//!
//! Each audio block the callback advances the world to the block's end and turns every sound it makes into an engine
//! command at the exact frame it falls on: no lookahead, so a turned knob or a played key is heard in the next block.
//! The world sits behind a mutex the UI edits under; the callback only `try_lock`s it, and if the UI holds it, the
//! machine catches up the next block. Everything slow (compiling a patch, parsing JSON) happens on the sync thread,
//! which watches the world and hands the audio thread ready-made values through a ring.
use crate::midi::Midi;
use cogs_dsp::{fields::FieldParams, ClimaxParams, CompiledPatch, Cmd, DrumVoice, Engine, ModalVoice, MotionProg, Sends, Strike, WavePatch};
use cogs_sim::{Event, VoiceKind, World};
use rtrb::{Consumer, Producer, RingBuffer};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex};

/// What the UI reads back from the audio thread.
#[derive(Default)]
pub struct Meters {
    pub sample_rate: AtomicU32,
    pub block: AtomicU32,
    pub buffer: AtomicU32,
    pub burst: AtomicU32,
    /// Measured output latency, microseconds (frames queued ahead of the speaker).
    pub latency_us: AtomicU32,
    pub notes: AtomicU64,
    pub last_note: AtomicU32,
    pub xruns: AtomicU32,
    /// The sim time now coming out of the speaker, as f64 bits: what the stage draws.
    pub heard: AtomicU64,
    pub events: AtomicU64,
    pub busy: AtomicU32,
}

/// Where the camera is, for the listener: a sound pans across the open stage and darkens off it (stage.js heard()).
#[derive(Clone, Copy, Debug)]
pub struct Listen { pub vx: f64, pub vy: f64, pub zoom: f64, pub w: f64, pub h: f64, pub e: f32 }
impl Default for Listen { fn default() -> Self { Listen { vx: 0., vy: 0., zoom: 1., w: 400., h: 800., e: 0. } } }
impl Listen {
    fn heard(&self, x: f64, y: f64) -> (f32, f32, f32) {
        let (sx, sy) = ((x - self.vx) * self.zoom, (y - self.vy) * self.zoom);
        let pan = (sx / (self.w * 0.6)).clamp(-0.85, 0.85);
        let out = ((-self.w / 2. - sx).max(sx - self.w / 2.).max(-self.h / 2. - sy).max(sy - self.h / 2.).max(0.)) / self.w.max(self.h).max(1.);
        let dark = 1. - (-out * 1.6).exp();
        (pan as f32, (1. - 0.6 * dark) as f32, dark as f32)
    }
}

/// How a part sounds, ready for the audio thread.
#[derive(Clone, Copy, Debug)]
pub enum Prep {
    Wave { level: f32, vel_sense: f32 },
    Modal { voice: ModalVoice, level: f32, vel_sense: f32 },
    Drum { voice: DrumVoice, level: f32, vel_sense: f32 },
    /// A string: its own modal strike (pluckSound).
    Str { bar: bool, decay: f64, damp: f64, hard: f64, level: f32, vel_sense: f32 },
}

enum Ctl {
    Patch(u32, Arc<CompiledPatch>),
    Prep(u32, Prep),
    Remove(u32),
    Cmd(Cmd),
    /// A motion whose start is in sim seconds (in `prog.f0`), turned into a frame on the audio thread.
    Motion(u32, u8, Option<MotionProg>),
}

/// Before a whole world replaces another: start the new voices' versions past every old one, so the sync thread
/// recompiles each patch even where a part id is reused.
pub fn bump_versions(old: &World, new: &mut World) {
    let base = old.pads.iter().map(|o| o.voice.ver).chain(old.readers.iter().map(|o| o.voice.ver)).chain(old.pits.iter().map(|o| o.voice.ver)).max().unwrap_or(0).wrapping_add(1);
    for v in new.pads.iter_mut().map(|o| &mut o.voice).chain(new.readers.iter_mut().map(|o| &mut o.voice)).chain(new.pits.iter_mut().map(|o| &mut o.voice)) { v.ver = v.ver.wrapping_add(base) }
}

/// A key the audio thread heard while recording, stamped with the sim time heard with it (see rec.rs).
#[derive(Clone, Copy, Debug)]
pub struct RecEv { pub on: bool, pub note: u8, pub vel: u8, pub at: f64 }

/// Everything the threads share.
pub struct Shared {
    pub world: Mutex<World>,
    pub listen: Mutex<Listen>,
    pub meters: Meters,
    /// The part the keyboard plays (0: the first synth reader).
    pub midi_part: AtomicU32,
    pub velcurve: AtomicU32,
    midi_tx: Mutex<Producer<Midi>>,
    pub running: AtomicBool,
    /// Keys go to `rec` too, stamped with the sim time heard with them, while this is set.
    pub recording: AtomicBool,
    pub rec: Mutex<Consumer<RecEv>>,
}
impl Shared {
    pub fn midi(&self, m: Midi) { if let Ok(mut p) = self.midi_tx.lock() { let _ = p.push(m); } }
    pub fn heard(&self) -> f64 { f64::from_bits(self.meters.heard.load(Relaxed)) }
}

pub struct Render {
    sh: Arc<Shared>,
    engine: Engine,
    midi: Consumer<Midi>,
    ctl: Consumer<Ctl>,
    dead: Producer<Arc<CompiledPatch>>,
    parts: HashMap<u32, Prep>,
    sr: f64,
    owed: f64,
    held: [u32; 128],
    sustain: bool,
    sustained: [bool; 128],
    listen: Listen,
    velcurve: f64,
    rec: Producer<RecEv>,
    /// The sim time this block starts on: heard together with a key played now.
    t_now: f64,
}

const MIDI_KEY: u32 = 0x4000_0000;

/// Make the threads' plumbing and the engine (slow: wavetables and buffers; call it off the audio thread), and start
/// the sync thread.
#[cfg(not(target_arch = "wasm32"))]
pub fn start(world: World, sr: f32) -> (Arc<Shared>, Render) {
    let (sh, render, mut sync) = parts(world, sr);
    let s2 = sh.clone();
    std::thread::Builder::new().name("cogs-sync".into()).spawn(move || {
        while s2.running.load(Relaxed) { sync.step(&s2); std::thread::sleep(std::time::Duration::from_millis(20)); }
    }).ok();
    (sh, render)
}

/// The plumbing, the engine and the sync step, without a thread: where there are no threads (the web's audio
/// worklet) the audio side calls `Sync::step` itself every 20 ms or so.
pub fn parts(world: World, sr: f32) -> (Arc<Shared>, Render, Sync) {
    let (mp, mc) = RingBuffer::new(1024);
    let (cp, cc) = RingBuffer::new(4096);
    let (dp, dc) = RingBuffer::new(256);
    let (rp, rc) = RingBuffer::new(512);
    let velcurve = world.s.vel_curve;
    let sh = Arc::new(Shared { world: Mutex::new(world), listen: Mutex::new(Listen::default()), meters: Meters::default(), midi_part: AtomicU32::new(0),
        velcurve: AtomicU32::new((velcurve as f32).to_bits()), midi_tx: Mutex::new(mp), running: AtomicBool::new(true),
        recording: AtomicBool::new(false), rec: Mutex::new(rc) });
    // Room for the busiest shipped world: every field with its reverb, and plenty of synth racks.
    let engine = Engine::with_config(sr, cogs_dsp::EngineConfig { parts: 96, rack_delays: 24, rack_verbs: 12, field_verbs: 8, modal_voices: 64, drum_voices: 64 });
    let render = Render { sh: sh.clone(), engine, midi: mc, ctl: cc, dead: dp, parts: HashMap::with_capacity(512), sr: sr as f64, owed: 0., held: [0; 128],
        sustain: false, sustained: [false; 128], listen: Listen::default(), velcurve, rec: rp, t_now: 0. };
    (sh, render, Sync::new(cp, dc))
}

/// The patch a wave voice means. A save from the HTML build carries the whole patch; a new part carries only its preset.
/// The part's own octave and semitones fold into the patch's tune, as waveVoice does.
pub fn wave_patch(raw: &serde_json::Map<String, Value>) -> WavePatch {
    let mut j = Value::Object(raw.clone());
    if raw.get("osc").is_none() {
        let name = raw.get("preset").and_then(|v| v.as_str()).unwrap_or("pluck");
        let mut p = WavePatch::preset(name).or_else(|| WavePatch::preset("pluck")).unwrap();
        let mut pj = p.to_json();
        if let (Some(o), Value::Object(m)) = (pj.as_object_mut(), &j) { for k in ["note", "octave", "semi"] { if let Some(v) = m.get(k) { o.insert(k.into(), v.clone()); } } }
        j = pj; p = WavePatch::from_json(&j); let _ = p;
    }
    let (oct, semi) = (num(&j, "octave"), num(&j, "semi"));
    if oct != 0. || semi != 0. {
        if let Some(t) = j.get_mut("tune").and_then(|t| t.as_object_mut()) {
            let o = (num_m(t, "oct") + oct).clamp(-4., 4.); let s = (num_m(t, "semi") + semi).clamp(-12., 12.);
            t.insert("oct".into(), o.into()); t.insert("semi".into(), s.into());
        }
    }
    WavePatch::from_json(&j)
}
fn num(j: &Value, k: &str) -> f64 { j.get(k).and_then(|v| v.as_f64()).unwrap_or(0.) }
fn num_m(j: &serde_json::Map<String, Value>, k: &str) -> f64 { j.get(k).and_then(|v| v.as_f64()).unwrap_or(0.) }

/// The sync step: what changed in the world since the last step goes to the audio thread, ready to use. On a thread
/// every 20 ms (`start`), or called by the audio side where there are no threads.
pub struct Sync {
    tx: Producer<Ctl>,
    dead: Consumer<Arc<CompiledPatch>>,
    vers: HashMap<u32, (u32, VoiceKind)>,
    compiled: HashMap<u32, Arc<CompiledPatch>>,
    space: String, climax: String, master: f64, bpm: f64, listen_e: f32,
    fields: HashMap<i32, String>,
    motions: HashMap<(u32, u8), (u64, f64, f64, f64, f64, f64)>,
    /// what did not fit in the ring last step
    backlog: std::collections::VecDeque<Ctl>,
}
impl Sync {
    fn new(tx: Producer<Ctl>, dead: Consumer<Arc<CompiledPatch>>) -> Self {
        Sync { tx, dead, vers: HashMap::new(), compiled: HashMap::new(), space: String::new(), climax: String::new(), master: f64::NAN, bpm: f64::NAN,
            listen_e: f32::NAN, fields: HashMap::new(), motions: HashMap::new(), backlog: Default::default() }
    }
    /// Forget what was sent, so the next step sends everything again (a world replaced whole, whose voices' `ver`
    /// counts start over).
    pub fn forget(&mut self) {
        self.vers.clear(); self.space.clear(); self.climax.clear(); self.master = f64::NAN; self.bpm = f64::NAN; self.listen_e = f32::NAN;
        self.fields.clear(); self.motions.clear();
    }
    pub fn step(&mut self, sh: &Shared) {
        let Sync { tx, dead, vers, compiled, space, climax, master, bpm, listen_e, fields, motions, backlog } = self;
        let (vers, compiled, space, climax, master, bpm, listen_e, fields, motions) = (vers, compiled, space, climax, master, bpm, listen_e, fields, motions);
        {
        while dead.pop().is_ok() {}
        // Copy what changed under the lock; compile after letting go.
        let mut voices: Vec<(u32, VoiceKind, serde_json::Map<String, Value>, f64, f64, Option<(bool, f64, f64, f64)>)> = Vec::new();
        let mut cmds: Vec<Ctl> = Vec::new();
        let mut present: Vec<u32> = Vec::new();
        if let Ok(w) = sh.world.lock() {
            let mut look = |id: u32, v: &cogs_sim::model::Voice, strs: Option<(bool, f64, f64, f64)>| {
                present.push(id);
                if vers.get(&id).map(|x| x.0) != Some(v.ver) || vers.get(&id).map(|x| x.1) != Some(v.kind) {
                    vers.insert(id, (v.ver, v.kind)); voices.push((id, v.kind, v.raw.clone(), v.level, v.vel_sense, strs));
                }
            };
            for p in &w.pads { look(p.id, &p.voice, None) }
            for r in &w.readers { look(r.id, &r.voice, None) }
            for p in &w.pits { look(p.id, &p.voice, None) }
            for s in &w.strings {
                present.push(s.id);
                let ver = (s.decay.to_bits() ^ s.damp.to_bits().rotate_left(7) ^ s.hard.to_bits().rotate_left(13) ^ s.level.to_bits().rotate_left(29) ^ (s.kind == "bar") as u64) as u32;
                if vers.get(&s.id).map(|x| x.0) != Some(ver) { vers.insert(s.id, (ver, VoiceKind::Synth));
                    cmds.push(Ctl::Prep(s.id, Prep::Str { bar: s.kind == "bar", decay: s.decay, damp: s.damp, hard: s.hard, level: s.level as f32, vel_sense: s.vel_sense as f32 })) }
            }
            let sp = serde_json::to_string(&w.space).unwrap_or_default();
            if sp != *space { cmds.push(Ctl::Cmd(Cmd::Space(cogs_dsp::SpaceParams::from_json(&Value::Object(w.space.clone()))))); *space = sp }
            let cl = serde_json::to_string(&w.climax).unwrap_or_default();
            if cl != *climax { cmds.push(Ctl::Cmd(Cmd::Climax(ClimaxParams::from_json(&Value::Object(w.climax.clone()))))); *climax = cl }
            if w.s.master != *master { *master = w.s.master; cmds.push(Ctl::Cmd(Cmd::Master { level: *master as f32 })) }
            if w.s.bpm != *bpm { *bpm = w.s.bpm; cmds.push(Ctl::Cmd(Cmd::Tempo { bpm: *bpm as f32 })) }
            for f in &w.fields {
                if f.slot < 0 { continue }
                let j = cogs_sim::field::field_out(f); let s = j.to_string();
                if fields.get(&f.slot) != Some(&s) { cmds.push(Ctl::Cmd(Cmd::Field { slot: f.slot as u8, params: Some(FieldParams::from_json(&j)) })); fields.insert(f.slot, s); }
            }
            let mut seen = Vec::new();
            w.motions(&mut |m| {
                let k = (m.target.id(), m.slot); seen.push(k);
                let sig = (m.sig, m.t0, m.ph0, m.rate, m.lo, m.hi);
                if motions.get(&k) == Some(&sig) { return }
                motions.insert(k, sig);
                let mut prog = MotionProg { table: [0.; cogs_dsp::wave::MOTION_POINTS], len: 0, step: m.step, f0: m.t0, ph0: m.ph0, rate: m.rate, lo: m.lo, hi: m.hi, glide: m.glide };
                if m.table.is_empty() { prog.table[0] = m.value as f32; prog.len = 1 } else {
                    let n = m.table.len().min(prog.table.len()); for i in 0..n { prog.table[i] = m.table[i] as f32 } prog.len = n }
                cmds.push(Ctl::Motion(k.0, k.1, Some(prog)));
            });
            motions.retain(|k, _| { let keep = seen.contains(k); if !keep { cmds.push(Ctl::Motion(k.0, k.1, None)) } keep });
        }
        if let Ok(l) = sh.listen.lock() { if l.e != *listen_e { *listen_e = l.e; cmds.push(Ctl::Cmd(Cmd::Listen { e: l.e })) } }
        let gone: Vec<u32> = vers.keys().copied().filter(|id| !present.contains(id)).collect();
        for id in gone { vers.remove(&id); compiled.remove(&id); cmds.push(Ctl::Remove(id)) }
        for (id, kind, raw, level, vs, _) in voices {
            let (level, vel_sense) = (level as f32, vs as f32);
            let j = Value::Object(raw.clone());
            match kind {
                VoiceKind::Wave => {
                    let p = wave_patch(&raw);
                    let cp = Arc::new(CompiledPatch::compile(&p, compiled.get(&id).map(|a| &**a)));
                    compiled.insert(id, cp.clone());
                    cmds.push(Ctl::Patch(id, cp)); cmds.push(Ctl::Prep(id, Prep::Wave { level, vel_sense }));
                }
                VoiceKind::Synth => cmds.push(Ctl::Prep(id, Prep::Modal { voice: ModalVoice::from_json(&j), level, vel_sense })),
                _ => if let Some(d) = DrumVoice::from_json(&j) { cmds.push(Ctl::Prep(id, Prep::Drum { voice: d, level, vel_sense })) },
            }
        }
        backlog.extend(cmds);
        while let Some(c) = backlog.pop_front() {
            if let Err(rtrb::PushError::Full(back)) = tx.push(c) { backlog.push_front(back); break }
        }
        }
    }
}

impl Render {
    fn sends(e: &Event) -> Sends {
        let mut s = Sends { dry: e.dry as f32, ..Sends::default() };
        for x in e.sends() { if (x.slot as usize) < s.amt.len() { s.amt[x.slot as usize] = x.amount } }
        s
    }

    fn play(&mut self, e: &Event, at: u64) {
        let Some(prep) = self.parts.get(&e.part.id()).copied() else { return };
        let (pan, gain, dark) = self.listen.heard(e.x, e.y);
        let amp = (e.amp as f32) * gain;
        let sends = Self::sends(e);
        for x in e.sends() { self.engine.command(Cmd::FieldHit { at, slot: x.slot, q: x.q, amt: x.amount }) }
        match prep {
            Prep::Wave { .. } => self.engine.command(Cmd::Note { at, part: e.part.id(), key: e.key, midi: e.midi as f32, amp, vel: e.vel as f32, pan, hold: e.hold as f32, dark, sends }),
            Prep::Modal { voice, .. } => self.engine.command(Cmd::Modal { at, key: e.key, voice, midi: e.midi as f32, amp, pan, x: e.vel as f32, hold: e.hold as f32, dark, sends }),
            Prep::Drum { voice, .. } => self.engine.command(Cmd::Drum { at, voice, amp, pan, dark, sends }),
            Prep::Str { bar, decay, damp, hard, .. } => self.engine.command(Cmd::Strike(pluck(at, e.key, e.midi, e.u, amp as f64, pan as f64, e.vel, dark as f64, bar, decay, damp, hard, sends))),
        }
    }

    /// The part the keyboard plays now.
    fn midi_target(&self) -> Option<(u32, Prep)> {
        let id = self.sh.midi_part.load(Relaxed);
        if id != 0 { if let Some(p) = self.parts.get(&id) { return Some((id, *p)) } }
        // nothing chosen: the lowest-numbered synth part
        self.parts.iter().filter(|(_, p)| matches!(p, Prep::Wave { .. })).min_by_key(|(id, _)| **id).map(|(id, p)| (*id, *p))
    }

    fn key_on(&mut self, note: u8, vel: u8, at: u64) {
        self.key_off(note, at, true);
        let Some((id, prep)) = self.midi_target() else { return };
        let vx = vel as f64 / 127.;
        let g = |level: f32, vs: f32| level as f64 * (1. - vs as f64 + vs as f64 * vx.powf(self.velcurve));
        let key = MIDI_KEY + id * 128 + note as u32;
        let s = Sends::default();
        match prep {
            Prep::Wave { level, vel_sense } => self.engine.command(Cmd::Note { at, part: id, key, midi: note as f32, amp: g(level, vel_sense) as f32, vel: vx as f32, pan: 0., hold: 0., dark: 0., sends: s }),
            Prep::Modal { voice, level, vel_sense } => self.engine.command(Cmd::Modal { at, key, voice, midi: note as f32, amp: g(level, vel_sense) as f32, pan: 0., x: vx as f32, hold: 30., dark: 0., sends: s }),
            Prep::Drum { voice, level, vel_sense } => self.engine.command(Cmd::Drum { at, voice, amp: g(level, vel_sense) as f32, pan: 0., dark: 0., sends: s }),
            Prep::Str { bar, decay, damp, hard, level, vel_sense } => self.engine.command(Cmd::Strike(pluck(at, key, note as f64, 0.3, g(level, vel_sense), 0., vx, 0., bar, decay, damp, hard, s))),
        }
        self.held[note as usize] = key;
        if self.sh.recording.load(Relaxed) { let _ = self.rec.push(RecEv { on: true, note, vel, at: self.t_now }); }
        let m = &self.sh.meters; m.notes.fetch_add(1, Relaxed); m.last_note.store(note as u32, Relaxed);
    }
    /// stage.js sound(): now, at a middling velocity, its own part's voice; a short note for the synths.
    fn audition(&mut self, part: u32, midi: f32, key: u32, at: u64) {
        let target = if part != 0 { self.parts.get(&part).map(|p| (part, *p)) } else { None }.or_else(|| self.midi_target());
        let Some((id, prep)) = target else { return };
        let s = Sends::default();
        match prep {
            Prep::Wave { level, .. } => { self.engine.command(Cmd::Note { at, part: id, key, midi, amp: 0.85 * level, vel: 0.7, pan: 0., hold: 0.3, dark: 0., sends: s }) }
            Prep::Modal { voice, level, .. } => self.engine.command(Cmd::Modal { at, key, voice, midi, amp: 0.85 * level, pan: 0., x: 0.7, hold: 0., dark: 0., sends: s }),
            Prep::Drum { voice, level, .. } => self.engine.command(Cmd::Drum { at, voice, amp: 0.85 * level, pan: 0., dark: 0., sends: s }),
            Prep::Str { bar, decay, damp, hard, level, .. } => self.engine.command(Cmd::Strike(pluck(at, key, midi as f64, 0.3, 0.85 * level as f64, 0., 0.7, 0., bar, decay, damp, hard, s))),
        }
    }
    fn key_off(&mut self, note: u8, at: u64, now: bool) {
        let k = self.held[note as usize]; if k == 0 { return }
        if self.sustain && !now { self.sustained[note as usize] = true; return }
        self.held[note as usize] = 0; self.sustained[note as usize] = false;
        self.engine.command(Cmd::Off { at, key: k });
        if self.sh.recording.load(Relaxed) { let _ = self.rec.push(RecEv { on: false, note, vel: 0, at: self.t_now }); }
    }

    /// Voices sounding now, every kind.
    pub fn voices(&self) -> usize { let s = self.engine.stats(); s.wave_voices + s.modal_voices + s.drum_voices }

    pub fn block(&mut self, l: &mut [f32], r: &mut [f32], latency: f64) {
        let n = l.len();
        let f0 = self.engine.frame();
        while let Ok(c) = self.ctl.pop() {
            match c {
                Ctl::Patch(id, p) => self.engine.command(Cmd::Patch { part: id, patch: p }),
                Ctl::Prep(id, p) => { self.parts.insert(id, p); }
                Ctl::Remove(id) => { self.parts.remove(&id); self.engine.command(Cmd::RemovePart { part: id }) }
                Ctl::Cmd(c) => self.engine.command(c),
                Ctl::Motion(part, slot, prog) => {
                    // its start, in sim seconds, as a frame: the sim time this block starts at sits on f0
                    let t_now = self.sh.world.try_lock().map(|w| w.clock.sim_t).unwrap_or(f64::NAN);
                    let prog = prog.map(|mut p| { if t_now.is_finite() { p.f0 = f0 as f64 + (p.f0 - t_now) * self.sr } else { p.f0 = f0 as f64 } p });
                    self.engine.command(Cmd::Motion { part, slot, prog })
                }
            }
        }
        if let Ok(l) = self.sh.listen.try_lock() { self.listen = *l }
        self.velcurve = f32::from_bits(self.sh.velcurve.load(Relaxed)) as f64;
        while let Ok(m) = self.midi.pop() {
            match m {
                Midi::On { note, vel } => self.key_on(note, vel, f0),
                Midi::Off { note } => self.key_off(note, f0, false),
                Midi::Cc { cc: 64, value } => { self.sustain = value >= 64; if !self.sustain { for i in 0..128 { if self.sustained[i] { self.key_off(i as u8, f0, true) } } } }
                Midi::Cc { cc: 120 | 123, .. } => { for i in 0..128 { self.key_off(i as u8, f0, true) } }
                Midi::Audition { part, midi, key } => self.audition(part, midi, key, f0),
                Midi::Release { key } => self.engine.command(Cmd::Off { at: f0, key }),
                _ => {}
            }
        }
        // The machine: advance to the end of this block; each sound lands on its own frame.
        let dt = n as f64 / self.sr;
        let sh = self.sh.clone();
        let world = sh.world.try_lock();
        match world {
            Ok(mut w) if !w.clock.paused => {
                let t0 = w.clock.sim_t;
                let t1 = t0 + self.owed + dt;
                self.owed = 0.;
                let sr = self.sr;
                // collect into a fixed scratch to keep the borrow of `w` apart from `self.engine`
                let mut buf = std::mem::take(&mut SCRATCH.with(|s| s.take()));
                w.advance(t1, &mut |e: Event| buf.push(e));
                self.t_now = w.clock.sim_t;
                let heard = (w.clock.sim_t - latency - dt).max(0.);
                self.sh.meters.heard.store(heard.to_bits(), Relaxed);
                drop(w);
                let span = t1 - t0;
                for e in buf.drain(..) {
                    // the block covers [t0, t1) of sim time over its n frames (more, if it is catching up)
                    let frac = if span > 0. { ((e.t - t0) / span).clamp(0., 1.) } else { 0. };
                    let at = f0 + (frac * (n as f64 - 1.)).round() as u64;
                    self.play(&e, at);
                    self.sh.meters.events.fetch_add(1, Relaxed);
                }
                let _ = sr;
                SCRATCH.with(|s| s.set(buf));
            }
            Ok(w) => { let h = w.clock.view; self.t_now = w.clock.sim_t; drop(w); self.sh.meters.heard.store(h.to_bits(), Relaxed) }
            Err(_) => self.owed += dt,
        }
        self.engine.process(l, r);
        for p in self.engine.retired() { let _ = self.dead.push(p); }
        self.sh.meters.block.store(n as u32, Relaxed);
    }
}

thread_local! { static SCRATCH: std::cell::Cell<Vec<Event>> = std::cell::Cell::new(Vec::with_capacity(4096)); }

/// pluckSound: a string's modal strike, its partials weighted by where along it the arm caught it.
#[allow(clippy::too_many_arguments)]
fn pluck(at: u64, key: u32, midi: f64, u: f64, amp: f64, pan: f64, vx: f64, dark: f64, bar: bool, decay: f64, damp: f64, hard: f64, sends: Sends) -> Strike {
    const BAR: [f64; 5] = [1., 2.756, 5.404, 8.933, 13.34];
    let tune = |r: f64| if (r - r.round()).abs() < 0.02 { (r + 0.5).floor() } else { 2f64.powf(((12. * r.log2()) + 0.5).floor() / 12.) };
    let f0 = 440. * 2f64.powf((midi - 69.) / 12.);
    let mut s = Strike { at, key, amp: amp * 0.3, contact: ((0.0016 - 0.0014 * hard) * vx.max(0.02).powf(-0.35)).clamp(0.00012, 0.004), attack: 0., pan, n: 5,
        f: [0.; cogs_dsp::modal::MAXM], g: [0.; cogs_dsp::modal::MAXM], t: [0.; cogs_dsp::modal::MAXM], sends };
    for k in 0..5 {
        let r = if bar { tune(BAR[k]) } else { (k + 1) as f64 };
        s.f[k] = f0 * r; s.g[k] = ((k + 1) as f64 * std::f64::consts::PI * u).sin().abs() / (k + 1) as f64 * (-dark * k as f64).exp(); s.t[k] = decay * r.powf(-damp);
    }
    s
}
