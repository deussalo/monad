//! cogs' sound: the audio engine, ported from the HTML build's worklets (wave.worklet.js, engine.worklet.js, verb.js,
//! climax.js, field.worklet.js) and the WebAudio graph in sim.js (the drums, the master chain). Nothing allocates,
//! locks or does I/O inside `Engine::process` or `Engine::command`, so both can run in a real-time audio callback.
//! Heavy preparation (wavetables, compiling a patch) happens in `Engine::new` and `CompiledPatch::new`, off that thread.
//!
//! Time is in frames at the engine's sample rate, counted from the engine's start (`Engine::frame`). A command's `at`
//! in the past plays at once.
//!
//! The graph, as in the HTML build:
//! wave synth (per-patch FX racks) + drums ─┐
//!                                          ├─ room: dry × dry level + reverb (verb.rs) ─┐
//! modal strikes ───────────────────────────┘                                            ├─ master gain ─ climax ─ distant low-pass ─ limiter ─ out
//! field sends of all three ─ fields (delay / reverb / distortion per slot) ─────────────┘

pub mod params;
pub mod patch;
pub mod table;
pub mod wave;
pub mod verb;
pub mod climax;
pub mod modal;
pub mod drums;
pub mod fields;
pub mod master;

pub use climax::{ClimaxParams, CLIMAX_DEFAULT, CLIMAX_DEFS};
pub use drums::{DrumKind, DrumVoice};
pub use fields::FieldParams;
pub use modal::{ModalVoice, Strike};
pub use patch::{WavePatch, WAVE_PRESET_NAMES};
pub use verb::{VerbParams, VERB_COLORS, VERB_DEFAULT, VERB_DEFS, VERB_MODES};
pub use wave::{CompiledPatch, MotionProg};

use std::sync::Arc;

/// The engine renders in blocks of at most this many frames (a WebAudio render quantum).
pub const BLOCK: usize = 128;
/// FX fields that sound at once.
pub const FIELD_SLOTS: usize = 8;
pub(crate) use wave::Aux;

/// Where a sound goes besides the dry mix: its amount into each field slot, and the dry level left (a field can take
/// some of the dry away).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sends { pub amt: [f32; FIELD_SLOTS], pub dry: f32 }
impl Default for Sends { fn default() -> Self { Sends { amt: [0.; FIELD_SLOTS], dry: 1. } } }
impl Sends {
    #[inline]
    pub fn any(&self) -> bool { self.amt.iter().any(|a| *a != 0.) }
}

/// xorshift64*: Math.random's stand-in.
#[derive(Clone)]
pub struct Rng(u64);
impl Rng {
    pub fn new(seed: u64) -> Self { Rng(seed.max(1)) }
    #[inline]
    pub fn next(&mut self) -> f64 {
        let mut x = self.0; x ^= x >> 12; x ^= x << 25; x ^= x >> 27; self.0 = x;
        (x.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// The room's dry knob; SPACE_DEFS in sim.js is this, then VERB_DEFS.
pub const SPACE_DRY_DEF: (&str, &str, f64, f64, f64) = ("dry", "dry", 0., 1., 0.001);

/// The room's space (SPACE in sim.js): the reverb's knobs and the dry level beside it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpaceParams { pub verb: VerbParams, pub dry: f64 }
impl Default for SpaceParams {
    /// SPACE: the room's own defaults, over the reverb's.
    fn default() -> Self {
        let mut v = VerbParams::default();
        v.0[verb::V_WET] = 0.26; v.0[verb::V_PREDELAY] = 8.; v.0[verb::V_DECAY] = 2.; v.0[verb::V_SIZE] = 0.55; v.0[verb::V_ATTACK] = 0.15;
        SpaceParams { verb: v, dry: 1. }
    }
}
impl SpaceParams {
    /// From a save's `space` (already migrated): known keys over the defaults, clamped.
    pub fn from_json(j: &serde_json::Value) -> Self {
        let mut s = Self::default();
        if let Some(o) = j.as_object() {
            for (k, v) in o {
                let x = patch::js_num(v);
                if k == "dry" { if x.is_finite() { s.dry = x.clamp(0., 1.); } } else { s.verb.set(k, x); }
            }
        }
        s
    }
}

/// One thing for the engine to do, at a frame (a frame in the past plays at once).
#[derive(Clone, Debug)]
pub enum Cmd {
    /// A wave-synth patch for a part, compiled off the audio thread (`CompiledPatch::new`). Voices of the part keep
    /// playing, now on the new patch.
    Patch { part: u32, patch: Arc<CompiledPatch> },
    /// A part is gone: its voices stop, its rack's delay line and reverb go back to the pool.
    RemovePart { part: u32 },
    /// A note on a part's wave patch. `key` names it for `Off`; `hold` > 0 releases it by itself after that many
    /// seconds (at least the patch's gate). `midi` is the part's note (voiceMidi; the patch adds its own tune);
    /// `vel` 0..1 is the hit's velocity (the `vel` mod source); `dark` the listener's distance darkening.
    Note { at: u64, part: u32, key: u32, midi: f32, amp: f32, vel: f32, pan: f32, hold: f32, dark: f32, sends: Sends },
    /// Release the notes with this key: wave notes let go, modal strikes are damped over 60 ms (damp() in sim.js).
    Off { at: u64, key: u32 },
    /// A modal-synth note: synth() in sim.js. `x` is the hit's velocity 0..1 (it sets the contact time); `hold` > 0
    /// rings at least 1.5× that long and is damped after it.
    Modal { at: u64, key: u32, voice: ModalVoice, midi: f32, amp: f32, pan: f32, x: f32, hold: f32, dark: f32, sends: Sends },
    /// A strike built by hand (modes, gains, decays).
    Strike(Strike),
    /// A drum hit.
    Drum { at: u64, voice: DrumVoice, amp: f32, pan: f32, dark: f32, sends: Sends },
    /// A motion part (a cam, a probe) turning a part's macro `slot` (0..7); None lets go of it.
    Motion { part: u32, slot: u8, prog: Option<MotionProg> },
    /// The machine's bpm (120 without a bpm lock): LFO and delay sync, field delays.
    Tempo { bpm: f32 },
    /// The room: reverb knobs (mode and color crossfade, the rest glide) and the dry level.
    Space(SpaceParams),
    /// The listener's distance, eased 0 (at the fit or closer) .. 1 (far out): quieter, wetter, low-passed (listen()).
    Listen { e: f32 },
    /// The master fader (S.master, 0..1.5).
    Master { level: f32 },
    /// The line amp on the master.
    Climax(ClimaxParams),
    /// A field's effect for a slot (0..7), or None: the slot is free.
    Field { slot: u8, params: Option<FieldParams> },
    /// A sound struck in a field at a frame: where (q, 0 rim .. 1 centre) and how much it sent.
    FieldHit { at: u64, slot: u8, q: f32, amt: f32 },
    /// Which part's patch the editor watches (its modulated knobs: `Engine::watched`).
    Watch { part: Option<u32> },
    /// Every voice off at once.
    Panic,
}

/// Pool sizes: everything is allocated up front.
#[derive(Clone, Copy, Debug)]
pub struct EngineConfig {
    /// parts with a wave patch at once
    pub parts: usize,
    /// rack delays (2.1 s stereo lines) shared by the parts whose patch turns its delay on
    pub rack_delays: usize,
    /// rack reverbs shared by the parts whose patch turns its reverb on
    pub rack_verbs: usize,
    /// reverbs for reverb fields
    pub field_verbs: usize,
    pub modal_voices: usize,
    pub drum_voices: usize,
}
impl Default for EngineConfig {
    fn default() -> Self { EngineConfig { parts: 48, rack_delays: 12, rack_verbs: 6, field_verbs: 4, modal_voices: 64, drum_voices: 48 } }
}

/// What the engine is doing, for the UI's readouts.
#[derive(Clone, Copy, Debug, Default)]
pub struct Stats { pub wave_voices: usize, pub modal_voices: usize, pub drum_voices: usize, pub steals: u32, pub drops: u32, pub strikes: u32,
    /// the last block's peak after the limiter, before the clip at full scale (over 1: clipped)
    pub peak: f32, pub reduction_db: f32 }

pub struct Engine {
    pub sample_rate: f32,
    fs: f64,
    frame: u64,
    wave: wave::WaveSynth,
    modal: modal::Modal,
    drums: drums::Drums,
    fields: fields::Fields,
    room: verb::Verb,
    climax: climax::Climax,
    distant: master::Distant,
    limiter: master::Compressor,
    aux: Aux,
    dl: [f32; BLOCK], dr: [f32; BLOCK], ol: [f32; BLOCK], or: [f32; BLOCK],
    space: SpaceParams, dry: f64, listen: f64,
    master: f64, master_target: f64, master_glide: f64, master_level: f64,
    quiet_frames: u64,
    silent_frames: u64,
    raw: bool,
    retired: Vec<Arc<CompiledPatch>>,
    stats: Stats,
}

impl Engine {
    pub fn new(sample_rate: f32) -> Self { Self::with_config(sample_rate, EngineConfig::default()) }
    pub fn with_config(sample_rate: f32, cfg: EngineConfig) -> Self {
        let fs = sample_rate as f64;
        // the registry and the built-in tables are made here, off the audio thread
        params::wave_params();
        table::builtin();
        let space = SpaceParams::default();
        let master = 0.7;
        Engine {
            sample_rate, fs, frame: 0,
            wave: wave::WaveSynth::new(fs, cfg.parts, cfg.rack_delays, cfg.rack_verbs),
            modal: modal::Modal::new(fs, cfg.modal_voices, BLOCK),
            drums: drums::Drums::new(fs, cfg.drum_voices, BLOCK),
            fields: fields::Fields::new(fs, cfg.field_verbs, BLOCK),
            room: verb::Verb::new(fs, &space.verb),
            climax: climax::Climax::new(fs),
            distant: master::Distant::new(fs),
            limiter: master::Compressor::new(fs, -10., 8., 3.5, 0.015, 0.3),
            aux: Aux::new(),
            dl: [0.; BLOCK], dr: [0.; BLOCK], ol: [0.; BLOCK], or: [0.; BLOCK],
            space, dry: space.dry, listen: 0., master, master_target: master, master_level: master, master_glide: 1. - (-1. / (0.08 * fs)).exp(),
            quiet_frames: 0, silent_frames: 0, raw: false, retired: Vec::with_capacity(64), stats: Stats::default(),
        }
    }
    /// Frames rendered so far: the engine's clock.
    pub fn frame(&self) -> u64 { self.frame }
    /// The last block's peak.
    pub fn peak(&self) -> f32 { self.stats.peak }
    pub fn stats(&self) -> Stats {
        let mut s = self.stats;
        s.wave_voices = self.wave.voices(); s.modal_voices = self.modal.active(); s.drum_voices = self.drums.active();
        s.steals = self.modal.steals; s.drops = self.modal.drops + self.wave.dropped + self.drums.dropped; s.strikes = self.modal.strikes;
        s.reduction_db = self.limiter.reduction as f32;
        s
    }
    /// Replaced patches, kept so their memory is not freed on the audio thread: drain them (and drop them elsewhere)
    /// now and then. Past 64 waiting, a replaced patch is dropped where it is.
    pub fn retired(&mut self) -> std::vec::Drain<'_, Arc<CompiledPatch>> { self.retired.drain(..) }
    /// The watched part's modulated knobs, where its newest note has them (normalised 0..1), into out; how many.
    pub fn watched(&self, out: &mut [(u16, f64)]) -> usize { self.wave.watched_into(out) }
    /// Where a field slot's parameters are now (0 at the rim, 1 at the centre).
    pub fn field_focus(&self, slot: usize) -> Option<f64> { self.fields.focus(slot) }

    /// A testing aid: skip the master chain (gain, line amp, distance low-pass, limiter), to compare the stages
    /// before it with the HTML build's worklets.
    pub fn set_raw(&mut self, raw: bool) { self.raw = raw; }
    fn retire(&mut self, p: Option<Arc<CompiledPatch>>) {
        if let Some(p) = p { if self.retired.len() < self.retired.capacity() { self.retired.push(p); } }
    }
    fn apply_space(&mut self) {
        // farther off, more of the room: the wet rises toward at most 1.6x (never past 1) and the dry falls by up to 45%
        let e = self.listen; let mut v = self.space.verb; let wet = v.0[verb::V_WET];
        v.0[verb::V_WET] = wet + ((wet * 1.6 + 0.1).min(1.) - wet) * e;
        self.room.set(&v);
        self.dry = self.space.dry * (1. - 0.45 * e);
    }
    pub fn command(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::Patch { part, patch } => { let old = self.wave.set_patch(part, patch); self.retire(old); }
            Cmd::RemovePart { part } => { let old = self.wave.remove_part(part); self.retire(old); }
            Cmd::Note { at, part, key, midi, amp, vel, pan, hold, dark, sends } => self.wave.note(wave::NoteMsg { at: at as f64, part, key,
                midi: (midi as f64).clamp(0., 127.), amp: amp as f64, vel: vel as f64, pan: pan as f64, hold: hold as f64, dark: dark as f64, sends }),
            Cmd::Off { at, key } => { self.wave.off(at as f64, key); self.modal.damp(at, key); }
            Cmd::Modal { at, key, voice, midi, amp, pan, x, hold, dark, sends } => {
                self.modal.strike(voice.strike(at, key, midi as f64, amp as f64, pan as f64, x as f64, hold as f64, dark as f64, sends));
                if hold > 0. { self.modal.damp(at + (hold as f64 * self.fs) as u64, key); }
            }
            Cmd::Strike(s) => self.modal.strike(s),
            Cmd::Drum { at, voice, amp, pan, dark, sends } => self.drums.hit(at, &voice, amp as f64, pan as f64, dark as f64, sends),
            Cmd::Motion { part, slot, prog } => self.wave.motion(part, slot as usize, prog),
            Cmd::Tempo { bpm } => { let b = if bpm > 0. { bpm as f64 } else { 120. }; self.wave.bpm = b; self.fields.bpm = b; }
            Cmd::Space(s) => { self.space = s; self.apply_space(); }
            Cmd::Listen { e } => {
                self.listen = (e as f64).clamp(0., 1.);
                self.master_target_set();
                self.distant.target = 20000. * (700f64 / 20000.).powf(self.listen);
                self.apply_space();
            }
            Cmd::Master { level } => { self.space_master(level as f64); }
            Cmd::Climax(p) => self.climax.set(&p),
            Cmd::Field { slot, params } => self.fields.set(slot as usize, params),
            Cmd::FieldHit { at, slot, q, amt } => self.fields.hit(at, slot as usize, q as f64, amt as f64),
            Cmd::Watch { part } => self.wave.watch = part,
            Cmd::Panic => { self.wave.panic(); self.modal.panic(); self.drums.panic(); }
        }
    }
    fn space_master(&mut self, level: f64) { self.master_level = level.clamp(0., 1.5); self.master_target_set(); }
    fn master_target_set(&mut self) { self.master_target = self.master_level * (1. - 0.5 * self.listen); }

    /// Render `l.len()` frames into the two channels (overwriting them).
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        let len = l.len().min(r.len());
        let mut done = 0;
        while done < len {
            let n = BLOCK.min(len - done);
            self.block(n);
            l[done..done + n].copy_from_slice(&self.ol[..n]);
            r[done..done + n].copy_from_slice(&self.or[..n]);
            done += n;
        }
    }
    fn block(&mut self, n: usize) {
        let frame = self.frame;
        let (dl, dr) = (&mut self.dl[..n], &mut self.dr[..n]);
        dl.fill(0.); dr.fill(0.);
        self.aux.clear();
        // the engine's input: the wave synth (after its racks) and the drum bus
        self.wave.process(frame, n, dl, dr, &mut self.aux);
        self.drums.process(frame, n, dl, dr, &mut self.aux);
        let mut in_e = 0f64; for s in 0..n { in_e += (dl[s] * dl[s]) as f64; }
        let voices = self.modal.process(frame, n, dl, dr, &mut self.aux);
        // the room: the dry at its level and the reverb's wet; idle once the tail has outlived its input
        let (ol, or) = (&mut self.ol[..n], &mut self.or[..n]);
        ol.fill(0.); or.fill(0.);
        let quiet = voices == 0 && in_e < 1e-12 && self.room.energy < 1e-14 && self.room.cur[verb::V_FREEZE] < 0.5 && !self.room.pending;
        self.quiet_frames = if quiet { self.quiet_frames + n as u64 } else { 0 };
        if self.quiet_frames <= (self.room.hold_blocks * BLOCK) as u64 {
            let d = self.dry as f32;
            for s in 0..n { ol[s] = dl[s] * d; or[s] = dr[s] * d; }
            self.room.process(dl, dr, ol, or, n);
        }
        // the fields' return, beside the room
        self.fields.process(frame, n, &self.aux, ol, or);
        // master gain (gliding as setTargetAtTime), the line amp, the distance low-pass, the limiter
        if self.raw { self.frame += n as u64; return; }
        // nothing has reached the master for 2 s: every filter and the limiter's lookahead have run dry, so the chain
        // is skipped (silence out) until something arrives
        let mut pk = 0f32; for s in 0..n { pk = pk.max(ol[s].abs()).max(or[s].abs()); }
        self.silent_frames = if pk < 1e-9 { self.silent_frames + n as u64 } else { 0 };
        if self.silent_frames > 2 * self.fs as u64 {
            self.master += (self.master_target - self.master) * (1. - (1. - self.master_glide).powi(n as i32));
            ol.fill(0.); or.fill(0.); self.stats.peak = 0.; self.frame += n as u64; return;
        }
        for s in 0..n {
            self.master += (self.master_target - self.master) * self.master_glide;
            let g = self.master as f32; ol[s] *= g; or[s] *= g;
        }
        self.climax.run(ol, or);
        self.distant.process(ol, or);
        self.limiter.process(ol, or);
        // the limiter lets peaks through; WebAudio's destination clips them at full scale, and so does this
        let mut pk = 0f32; for s in 0..n { pk = pk.max(ol[s].abs()).max(or[s].abs()); ol[s] = ol[s].clamp(-1., 1.); or[s] = or[s].clamp(-1., 1.); }
        self.stats.peak = pk;
        self.frame += n as u64;
    }
}
