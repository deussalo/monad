//! The web build's two halves and what they say to each other. The page has no threads it can share memory with
//! (no SharedArrayBuffer without COOP/COEP headers, which static hosts do not send), so the machine lives twice:
//!
//! - The audio worklet owns the World and the Engine (`AudioSide`): play.rs's Render advances the world per 128-frame
//!   quantum and plays what it does, as on Android; its Sync step runs every 8 quanta.
//! - The UI keeps its own World, the one the stage edits and draws. It advances it to the heard time itself, its
//!   sounds thrown away, so gears, pins and flashes are drawn from the same deterministic machine. Whenever the world
//!   changes (an edit, a drag, play or pause), it sends the whole save with its clock: the worklet lays it over its
//!   own (`World::reload`, every cog keeping its turns, live balls kept), sets the clock back to the UI's time and
//!   steps forward silently to where it was, so nothing already played sounds again and nothing is skipped.
//! - The worklet posts back, every 8 quanta, its sim time and the context time that sim time will be played at (the
//!   UI's heard time is that, less how far ahead of the speaker the audio runs), note counts and the peak; and the
//!   balls (which only the worklet's world can know: they are chaotic), which the UI lays over its own.
//!
//! Messages are JSON text both ways. UI → worklet: `{"type":"world","t":sim_t,"paused":b,"at_start":b,"turns":[[cog,n],..],
//! "skips":[[cog,ring,a,until],..],"world":{save}}`,
//! `{"type":"midi","d":[status,d1,d2]}`, `{"type":"audition","part":id,"midi":m,"key":k}`, `{"type":"part","id":id}`
//! (what the keyboard plays), `{"type":"listen","vx","vy","zoom","w","h","e"}` (the camera, for pan and distance).
//! Worklet → UI: `{"type":"state","t","ct","paused","notes","events","peak","voices","balls":[[pit,ball,x,y,vx,vy,flash,
//! [t,x,y,...]],...],"pits":[[pit,flash,hits],...],"rec":[[on,note,vel,at],...]}`. While the UI records (`{"type":"rec","on":b}`)
//! the worklet stamps every key with the sim time heard with it, as play.rs does, and the UI writes them onto its
//! plate with rec::Rec, then sends the world back. `{"type":"release","key":k}` lets go of an audition.
//! `{"type":"watch","part":id}` (no part: stop) asks for the modulated knobs of the patch open in the editor: the state
//! then carries `"mods":[knob,position,...]` while a note of it sounds.
use crate::midi;
use crate::play::{self, Listen, Render, Shared, Sync};
use cogs_sim::World;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{atomic::Ordering::Relaxed, Arc};

pub const QUANTUM: usize = 128;
/// A pin's `skip` of infinity, in JSON.
const SKIP_FOREVER: f64 = 1e300;

/// The worklet's side: the machine and the sound.
pub struct AudioSide {
    pub sh: Arc<Shared>,
    render: Render,
    sync: Sync,
    sr: f64,
    quanta: u64,
    pub l: [f32; QUANTUM],
    pub r: [f32; QUANTUM],
    out: String,
    peak: f32,
    /// keys heard while recording, stamped with their sim time, for the next state message
    rec: Vec<crate::play::RecEv>,
}

impl AudioSide {
    /// Slow (wavetables, buffers): the worklet makes it once, in its constructor.
    pub fn new(sr: f32) -> Self {
        let mut w = World::new();
        w.set_paused(true);
        let (sh, render, sync) = play::parts(w, sr);
        AudioSide { sh, render, sync, sr: sr as f64, quanta: 0, l: [0.; QUANTUM], r: [0.; QUANTUM], out: String::with_capacity(1 << 16), peak: 0., rec: Vec::with_capacity(256) }
    }

    /// One message from the UI.
    pub fn message(&mut self, text: &str) {
        let Ok(m) = serde_json::from_str::<Value>(text) else { return };
        let f = |k: &str| m.get(k).and_then(|v| v.as_f64()).unwrap_or(0.);
        match m.get("type").and_then(|v| v.as_str()).unwrap_or("") {
            "world" => {
                if let Some(j) = m.get("world") {
                    let turns: Vec<(u32, f64)> = m.get("turns").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|p| Some((p.get(0)?.as_u64()? as u32, p.get(1)?.as_f64()?))).collect()).unwrap_or_default();
                    let skips: Vec<[f64; 4]> = m.get("skips").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|p| Some([p.get(0)?.as_f64()?, p.get(1)?.as_f64()?, p.get(2)?.as_f64()?, p.get(3)?.as_f64()?])).collect()).unwrap_or_default();
                    self.apply_world(j, &turns, &skips, f("t"), m["paused"].as_bool().unwrap_or(false), m["at_start"].as_bool().unwrap_or(false));
                }
            }
            "midi" => {
                let d: Vec<u8> = m.get("d").and_then(|v| v.as_array()).map(|a| a.iter().map(|x| x.as_u64().unwrap_or(0) as u8).collect()).unwrap_or_default();
                if let Some(e) = midi::decode(*d.first().unwrap_or(&0), *d.get(1).unwrap_or(&0), *d.get(2).unwrap_or(&0)) { self.sh.midi(e) }
            }
            "release" => self.sh.midi(midi::Midi::Release { key: f("key") as u32 }),
            "rec" => self.sh.recording.store(m.get("on").and_then(|v| v.as_bool()).unwrap_or(false), Relaxed),
            "audition" => self.sh.midi(midi::Midi::Audition { part: f("part") as u32, midi: f("midi") as f32, key: f("key") as u32 }),
            "part" => self.sh.midi_part.store(f("id") as u32, Relaxed),
            "watch" => self.sh.meters.set_watch(m.get("part").and_then(|v| v.as_u64()).map(|p| p as u32)),
            "listen" => {
                if let Ok(mut l) = self.sh.listen.lock() { *l = Listen { vx: f("vx"), vy: f("vy"), zoom: f("zoom").max(1e-3), w: f("w").max(1.), h: f("h").max(1.), e: f("e") as f32 } }
            }
            _ => {}
        }
    }

    /// The UI's world, laid over this one at the UI's clock, then stepped silently back up to where this one was.
    fn apply_world(&mut self, j: &Value, turns: &[(u32, f64)], skips: &[[f64; 4]], t: f64, paused: bool, at_start: bool) {
        let Ok(mut w) = self.sh.world.lock() else { return };
        let fresh = w.cogs.is_empty() && w.pads.is_empty() && w.readers.is_empty();
        // how far ahead of the UI this world runs (what is queued for the speaker)
        let lead = if !w.clock.paused && !fresh { (w.clock.sim_t - t).clamp(0., 0.5) } else { 0. };
        // a voice keeps its version where its patch is unchanged, so only what changed is compiled again
        let mut old: HashMap<u32, (Value, u32)> = HashMap::new();
        for p in &w.pads { old.insert(p.id, (Value::Object(p.voice.raw.clone()), p.voice.ver)); }
        for r in &w.readers { old.insert(r.id, (Value::Object(r.voice.raw.clone()), r.voice.ver)); }
        for p in &w.pits { old.insert(p.id, (Value::Object(p.voice.raw.clone()), p.voice.ver)); }
        let live: Vec<((u32, u32), cogs_sim::Ball)> = w.pits.iter().flat_map(|p| p.balls.iter().map(move |b| ((p.id, b.id), b.clone()))).collect();
        let frames: HashMap<u32, (f64, f64, f64)> = w.pits.iter().map(|p| (p.id, (p.x, p.y, p.w))).collect();
        if w.reload(j).is_err() { return }
        let fix = |id: u32, v: &mut cogs_sim::model::Voice| {
            if let Some((raw, ver)) = old.get(&id) { v.ver = if *raw == Value::Object(v.raw.clone()) { *ver } else { ver.wrapping_add(1) } }
        };
        for p in &mut w.pads { fix(p.id, &mut p.voice) }
        for r in &mut w.readers { fix(r.id, &mut r.voice) }
        for p in &mut w.pits { fix(p.id, &mut p.voice) }
        // the plates' turn counts as the UI has them at its time (a chance pin hashes its turn: one counted twice
        // across the wrap would play a different pattern)
        for c in &mut w.cogs { if let Some(&(_, n)) = turns.iter().find(|x| x.0 == c.id) { c.turns = n } }
        // pins that must not sound yet (a key still held on one): the save does not carry that
        for s in skips {
            let Some(tr) = w.cog_mut(s[0] as u32).and_then(|c| c.track.as_mut()) else { continue };
            if let Some(d) = tr.dots.iter_mut().find(|d| d.ring == s[1] && (d.a - s[2]).abs() < 1e-9) { d.skip = if s[3] >= SKIP_FOREVER { f64::INFINITY } else { s[3] } }
        }
        w.clock.sim_t = t;
        w.clock.view = t;
        w.set_paused(paused);
        w.clock.at_start = at_start;
        self.sh.velcurve.store((w.s.vel_curve as f32).to_bits(), Relaxed);
        if !paused && lead > 0. { w.advance(t + lead, &mut |_| {}); }
        // the balls as they were here, at this world's own time (they are chaotic: only this side knows them)
        if !fresh {
            // where they fly is this side's; which note each is, and the sphere around it, are the edit's
            for p in &mut w.pits {
                let to = (p.x, p.y, p.w);
                let from = frames.get(&p.id).copied().unwrap_or(to);
                for b in &mut p.balls {
                    let Some((_, l)) = live.iter().find(|x| x.0 == (p.id, b.id)) else { continue };
                    let (note, r) = (b.note, b.r);
                    *b = l.clone();
                    (b.note, b.r) = (note, r);
                    b.reframe(from, to);
                }
            }
        }
        drop(w);
        // a new part should be ready before its first note
        self.sync.step(&self.sh);
    }

    /// Render one quantum into `l`/`r`. `ct` is the context time at its start (the worklet's currentTime).
    pub fn block(&mut self, ct: f64) {
        self.render.block(&mut self.l, &mut self.r, 0.);
        self.quanta += 1;
        if let Ok(mut c) = self.sh.rec.try_lock() { while let Ok(e) = c.pop() { if self.rec.len() < self.rec.capacity() { self.rec.push(e) } } }
        for i in 0..QUANTUM { self.peak = self.peak.max(self.l[i].abs()).max(self.r[i].abs()); }
        if self.quanta % 8 == 0 {
            self.sync.step(&self.sh);
            self.post(ct + QUANTUM as f64 / self.sr);
        }
    }

    /// The state message due after the last quantum, if any (taken).
    pub fn outbox(&mut self) -> Option<&str> { if self.out.is_empty() { None } else { Some(&self.out) } }
    pub fn clear_outbox(&mut self) { self.out.clear(); }

    fn post(&mut self, ct_end: f64) {
        use std::fmt::Write;
        let Ok(w) = self.sh.world.try_lock() else { return };
        let m = &self.sh.meters;
        self.out.clear();
        let _ = write!(self.out, r#"{{"type":"state","t":{},"ct":{},"paused":{},"notes":{},"events":{},"peak":{},"voices":{}"#, w.clock.sim_t, ct_end, w.clock.paused,
            m.notes.load(Relaxed), m.events.load(Relaxed), self.peak, self.render.voices());
        self.peak = 0.;
        if let Some(part) = m.watch.load(Relaxed).checked_sub(1) {
            let mut mods = Vec::new();
            m.watched(part, &mut mods);
            if !mods.is_empty() {
                self.out.push_str(r#","mods":["#);
                for (i, (d, x)) in mods.iter().enumerate() { let _ = write!(self.out, "{}{},{:.4}", if i > 0 { "," } else { "" }, d, x); }
                self.out.push(']');
            }
        }
        if !self.rec.is_empty() {
            self.out.push_str(r#","rec":["#);
            for (i, e) in self.rec.iter().enumerate() { let _ = write!(self.out, "{}[{},{},{},{}]", if i > 0 { "," } else { "" }, e.on as u8, e.note, e.vel, e.at); }
            self.out.push(']');
            self.rec.clear();
        }
        // the balls, every other post: their places and trails, which the UI draws at its heard time
        if !w.pits.is_empty() && self.quanta % 16 == 0 {
            let fnum = |x: f64| if x.is_finite() { x } else { -1e9 };
            self.out.push_str(r#","balls":["#);
            let mut first = true;
            for p in &w.pits { for b in &p.balls {
                if !first { self.out.push(',') } first = false;
                let _ = write!(self.out, "[{},{},{},{},{},{},{},[", p.id, b.id, b.x, b.y, b.vx, b.vy, fnum(b.flash));
                for (i, s) in b.trail.iter().enumerate() { let _ = write!(self.out, "{}{},{},{}", if i > 0 { "," } else { "" }, s[0], s[1], s[2]); }
                self.out.push_str("]]");
            } }
            self.out.push_str(r#"],"pits":["#);
            for (i, p) in w.pits.iter().enumerate() { let _ = write!(self.out, "{}[{},{},{}]", if i > 0 { "," } else { "" }, p.id, fnum(p.flash), p.hits); }
            self.out.push(']');
        }
        self.out.push('}');
    }
}

/// The UI's side of a state message: lay the worklet's balls over the UI's own world.
pub fn apply_balls(w: &mut World, state: &Value) {
    if let Some(bs) = state.get("balls").and_then(|v| v.as_array()) {
        for b in bs {
            let Some(a) = b.as_array() else { continue };
            let n = |i: usize| a.get(i).and_then(|v| v.as_f64()).unwrap_or(0.);
            let (pid, bid) = (n(0) as u32, n(1) as u32);
            let Some(p) = w.pits.iter_mut().find(|p| p.id == pid) else { continue };
            let Some(ball) = p.balls.iter_mut().find(|x| x.id == bid) else { continue };
            ball.x = n(2); ball.y = n(3); ball.vx = n(4); ball.vy = n(5);
            let fl = n(6); ball.flash = if fl <= -1e8 { f64::NEG_INFINITY } else { fl };
            if let Some(tr) = a.get(7).and_then(|v| v.as_array()) {
                ball.trail.clear();
                for c in tr.chunks(3) { if c.len() == 3 { ball.trail.push_back([c[0].as_f64().unwrap_or(0.), c[1].as_f64().unwrap_or(0.), c[2].as_f64().unwrap_or(0.)]) } }
            }
        }
    }
    if let Some(ps) = state.get("pits").and_then(|v| v.as_array()) {
        for q in ps {
            let Some(a) = q.as_array() else { continue };
            let n = |i: usize| a.get(i).and_then(|v| v.as_f64()).unwrap_or(0.);
            if let Some(p) = w.pits.iter_mut().find(|p| p.id == n(0) as u32) { let fl = n(1); p.flash = if fl <= -1e8 { f64::NEG_INFINITY } else { fl }; p.hits = n(2) as u32; }
        }
    }
}

/// The pins not to sound yet: [cog, ring, angle, until].
fn skips(w: &World) -> Vec<[f64; 4]> {
    let mut out = Vec::new();
    for c in &w.cogs { if let Some(t) = &c.track { for d in &t.dots { if d.skip > w.clock.sim_t { out.push([c.id as f64, d.ring, d.a, d.skip.min(SKIP_FOREVER)]) } } } }
    out
}

/// The world message for the worklet: the save, and the clock it was taken at.
pub fn world_message(w: &World) -> String {
    let turns: Vec<String> = w.cogs.iter().map(|c| format!("[{},{}]", c.id, c.turns)).collect();
    let skips: Vec<String> = skips(w).iter().map(|s| format!("[{},{},{},{}]", s[0], s[1], s[2], s[3])).collect();
    format!(r#"{{"type":"world","t":{},"paused":{},"at_start":{},"turns":[{}],"skips":[{}],"world":{}}}"#, w.clock.sim_t, w.clock.paused, w.clock.at_start, turns.join(","), skips.join(","), w.to_string())
}

/// What decides whether the world must go to the worklet again: the save without the camera, and while playing
/// without the plates' phases and the balls (they move by themselves on both sides).
pub fn world_sig(w: &World) -> Value {
    fn strip(v: &mut Value, keys: &[&str]) {
        match v {
            Value::Object(m) => { for k in keys { m.remove(*k); } for x in m.values_mut() { strip(x, keys) } }
            Value::Array(a) => for x in a { strip(x, keys) },
            _ => {}
        }
    }
    let mut v = w.serialize();
    if let Some(o) = v.as_object_mut() { o.remove("view"); if let Some(Value::Object(s)) = o.get_mut("S") { s.remove("zoom"); } }
    if !w.clock.paused { strip(&mut v, &["th", "bx", "by", "bvx", "bvy"]); }
    json!([v, w.clock.paused, w.clock.at_start, skips(w)])
}
