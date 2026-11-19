//! Editing: what the stage's gestures do to the machine (stage.js newPart / placePart / removeObj, the disc's pins,
//! links, a probe's pinning), without the gestures themselves. Each ends solved.

use crate::model::*;
use crate::solve::seat_reader;
use crate::model::Voice;
use crate::world::World;
use serde_json::Value;

/// A part by kind and id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartRef {
    Cog(u32),
    Arm(u32),
    Pad(u32),
    Reader(u32),
    Belt(u32),
    String(u32),
    Cam(u32),
    Pit(u32),
    Probe(u32),
    Field(u32),
}

/// What a new cog is: a plain cog, a motor, or a disc with a score.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CogKind {
    Cog,
    Motor,
    Disc,
}

impl World {
    fn take_id(&mut self) -> u32 {
        let id = self.next;
        self.next += 1;
        id
    }
    /// pickCog: the topmost (then smallest) cog under (x, y), optionally only discs.
    pub fn pick_cog(&self, x: f64, y: f64, discs: bool, tol: f64) -> Option<usize> {
        let mut best: Option<usize> = None;
        for (i, c) in self.cogs.iter().enumerate() {
            if discs && c.track.is_none() {
                continue;
            }
            if hypot(c.x - x, c.y - y) > c.radius() + tol {
                continue;
            }
            best = match best {
                None => Some(i),
                Some(b) => {
                    let o = &self.cogs[b];
                    if c.layer > o.layer || (c.layer == o.layer && c.n < o.n) { Some(i) } else { Some(b) }
                }
            };
        }
        best
    }

    /// A new cog, motor or disc placed at (x, y): onto an axle near there (within 10 screen px at `zoom`) as its
    /// next layer, otherwise clear of every other cog and snapped into mesh where that overlaps nothing.
    pub fn add_cog(&mut self, kind: CogKind, x: f64, y: f64, zoom: f64) -> u32 {
        let id = self.take_id();
        let motor = if kind == CogKind::Motor {
            Some(Motor { rpm: if self.s.bpm != 0.0 { self.s.bpm / self.s.beats } else { 30.0 }, revs: 1.0 })
        } else {
            None
        };
        self.cogs.push(Cog {
            id, x, y, n: if kind == CogKind::Disc { 48.0 } else { 12.0 }, motor, layer: 0.0, th: 0.0, th0: 0.0, tooth: 0.0,
            track: if kind == CogKind::Disc { Some(Track::default()) } else { None },
            w: 0.0, k: None, jam: false, turns: 0.0, driver: None, scrub: None,
        });
        let c = self.cogs.len() - 1;
        self.place_cog(c, x, y, zoom);
        id
    }
    /// stage.js placeCog: put cog c at (x, y), onto an axle there or clear of the rest and snapped.
    pub fn place_cog(&mut self, c: usize, x: f64, y: f64, zoom: f64) {
        let tol = js_max(self.s.coax_tol, 10.0 / zoom);
        let axle = (0..self.cogs.len()).find(|&o| o != c && hypot(self.cogs[o].x - x, self.cogs[o].y - y) <= tol);
        match axle {
            Some(a) => {
                let (ax, ay) = (self.cogs[a].x, self.cogs[a].y);
                let top = self.stack_at(ax, ay).filter(|&o| o != c).map(|o| self.cogs[o].layer).fold(f64::NEG_INFINITY, js_max);
                let cc = &mut self.cogs[c];
                cc.x = ax;
                cc.y = ay;
                cc.layer = top + 1.0;
            }
            None => {
                let cc = &mut self.cogs[c];
                cc.x = x;
                cc.y = y;
                cc.layer = 0.0;
                self.clear_shift(&[(c, x, y)], 0.0, 0.0, None);
                self.snap_clear(c, &[c]);
            }
        }
        self.solve();
    }
    pub fn add_pad(&mut self, x: f64, y: f64) -> u32 {
        let id = self.take_id();
        self.pads.push(Pad { id, x, y, r: 12.0, voice: Voice::new(VoiceKind::Kick), hits: 0, last_v: 0.0, flash: f64::NEG_INFINITY });
        self.solve();
        id
    }
    /// A reader (or a comb of 12 tines) dropped at (x, y): on a disc it lies along the radius there, seated, spanning
    /// its rings; elsewhere it lies level.
    pub fn add_reader(&mut self, x: f64, y: f64, comb: bool) -> u32 {
        let id = self.take_id();
        let mut r = Reader {
            id, on: None, x1: x - 30.0, y1: y, x2: x + 30.0, y2: y, voice: Voice::reader(), tines: if comb { 12.0 } else { 0.0 }, comb_base: 60.0,
            comb_scale: "major".into(), seat: None, on_ix: None, hits: 0, last_v: 0.0, last_midi: 0.0, flash: f64::NEG_INFINITY,
        };
        if let Some(d) = self.pick_cog(x, y, true, 0.0) {
            let c = &self.cogs[d];
            let a = (y - c.y).atan2(x - c.x);
            let rings = c.track.as_ref().unwrap().rings();
            let (r0, r1) = (c.ring_r(0.0) - 6.0, c.ring_r((rings - 1) as f64) + 6.0);
            r.x1 = c.x + a.cos() * r0;
            r.y1 = c.y + a.sin() * r0;
            r.x2 = c.x + a.cos() * r1;
            r.y2 = c.y + a.sin() * r1;
            seat_reader(&mut r, c);
        }
        self.readers.push(r);
        self.solve();
        id
    }
    pub fn add_string(&mut self, x: f64, y: f64) -> u32 {
        let id = self.take_id();
        self.strings.push(Str {
            id, x1: x - 60.0, y1: y, x2: x + 60.0, y2: y, kind: "string".into(), f200: 220.0, quant: true, decay: 2.5, damp: 0.5, hard: 0.5,
            level: 0.7, vel_sense: 0.7, hits: 0, last_v: 0.0, last_u: 0.0, flash: f64::NEG_INFINITY,
        });
        self.solve();
        id
    }
    /// An arm dropped on a cog: it points at (x, y) as drawn and reaches there.
    pub fn add_arm(&mut self, x: f64, y: f64) -> Option<u32> {
        let c = self.pick_cog(x, y, false, 0.0)?;
        let id = self.take_id();
        let cc = &self.cogs[c];
        let a = wrap((y - cc.y).atan2(x - cc.x) - self.render_th(c));
        let len = js_max(4.0, hypot(x - cc.x, y - cc.y));
        self.arms.push(Arm { id, cog: cc.id, a, r0: 0.0, len, flex: 0.5, cog_ix: c, flash: f64::NEG_INFINITY, bend_t: f64::NEG_INFINITY, bend_a: 0.0 });
        self.solve();
        Some(id)
    }
    /// A cam dropped on a cog: it rides that cog, its follower where it was let go.
    pub fn add_cam(&mut self, x: f64, y: f64) -> Option<u32> {
        let c = self.pick_cog(x, y, false, 0.0)?;
        let id = self.take_id();
        let mut m = crate::motion::new_cam(id);
        m.cog = self.cogs[c].id;
        m.ang = (y - self.cogs[c].y).atan2(x - self.cogs[c].x);
        self.cams.push(m);
        self.solve();
        Some(id)
    }
    pub fn add_pit(&mut self, x: f64, y: f64) -> u32 {
        let id = self.take_id();
        self.pits.push(crate::pit::new_pit(id, x, y));
        self.solve();
        id
    }
    pub fn add_field(&mut self, x: f64, y: f64) -> u32 {
        let id = self.take_id();
        self.fields.push(crate::field::new_field(id, x, y));
        self.solve();
        id
    }
    /// A probe dropped at (x, y): pinned onto whatever moving thing is there, or None (nothing to pin to).
    pub fn add_probe(&mut self, x: f64, y: f64, zoom: f64) -> Option<u32> {
        let id = self.take_id();
        self.probes.push(crate::probe::new_probe(id));
        let i = self.probes.len() - 1;
        if !self.probe_attach(i, x, y, zoom, false) {
            self.probes.pop();
            return None;
        }
        self.solve();
        Some(id)
    }
    /// A belt between two cogs not on one axle.
    pub fn add_belt(&mut self, a: u32, b: u32, cross: bool) -> Option<u32> {
        let (ai, bi) = (self.cog_ix(a)?, self.cog_ix(b)?);
        if ai == bi || self.contact(ai, bi) == Some(crate::solve::Contact::Axle) {
            return None;
        }
        let id = self.take_id();
        self.belts.push(Belt { id, a, b, cross, a_ix: ai, b_ix: bi });
        self.solve();
        Some(id)
    }

    /// stage.js removeObj: a cog takes its arms, cams and belts; a reader lets go of its cams; dead probes go.
    pub fn remove(&mut self, p: PartRef) {
        match p {
            PartRef::Cog(id) => {
                self.cogs.retain(|c| c.id != id);
                self.arms.retain(|a| a.cog != id);
                self.cams.retain(|m| m.cog != id);
                self.belts.retain(|b| b.a != id && b.b != id);
            }
            PartRef::Reader(id) => {
                self.readers.retain(|r| r.id != id);
                for m in &mut self.cams {
                    if m.to == Some(id) {
                        m.to = None;
                    }
                }
            }
            PartRef::Arm(id) => self.arms.retain(|o| o.id != id),
            PartRef::Pad(id) => self.pads.retain(|o| o.id != id),
            PartRef::Belt(id) => self.belts.retain(|o| o.id != id),
            PartRef::String(id) => self.strings.retain(|o| o.id != id),
            PartRef::Cam(id) => self.cams.retain(|o| o.id != id),
            PartRef::Pit(id) => self.pits.retain(|o| o.id != id),
            PartRef::Probe(id) => self.probes.retain(|o| o.id != id),
            PartRef::Field(id) => self.fields.retain(|o| o.id != id),
        }
        self.relink();
        self.prune_probes();
        self.solve();
    }

    /// stage.js setDot: put a pin on a disc's ring at angle a (from its mark), or take the one there off.
    pub fn set_dot(&mut self, cog: usize, ring: f64, a: f64, on: bool) {
        let Some(t) = self.cogs[cog].track.as_mut() else { return };
        let i = t.dots.iter().position(|o| o.ring == ring && (wrap(o.a - a + std::f64::consts::PI) - std::f64::consts::PI).abs() < 1e-6);
        match (on, i) {
            (true, None) => t.dots.push(Dot::new(ring, a)),
            (false, Some(i)) => {
                t.dots.remove(i);
            }
            _ => {}
        }
    }
    /// stage.js cellAt: the ring and step under world point (x, y) on a disc, as drawn.
    pub fn cell_at(&self, cog: usize, x: f64, y: f64) -> Option<(f64, f64)> {
        let c = &self.cogs[cog];
        let t = c.track.as_ref()?;
        let d = hypot(x - c.x, y - c.y);
        let r = c.radius();
        let n = t.rings();
        let ring = if n > 1 { js_round((d / r - 0.25) / 0.65 * (n - 1) as f64) } else { 0.0 };
        if ring < 0.0 || ring >= n as f64 {
            return None;
        }
        let step = TAU / t.div;
        let ang = (y - c.y).atan2(x - c.x) - self.render_th(cog);
        Some((ring, wrap(js_round(wrap(ang) / step) * step)))
    }

    /// The macro slot (1-based) a cam or probe linking to voice v takes: its first free one (no routes, knob at 0),
    /// else the first no other cam or probe link uses.
    fn free_macro(&self, target: Part, except: crate::motion::MotionFrom) -> i64 {
        self.pick_macro(target, Some(except)).unwrap_or(1)
    }
    /// As free_macro, counting every link to target but `skip`'s first one (the link being replaced); None when all
    /// eight are taken.
    fn pick_macro(&self, target: Part, skip: Option<crate::motion::MotionFrom>) -> Option<i64> {
        use crate::motion::MotionFrom;
        let mut used = [false; 9];
        let mut mark = |m: i64| used[m.clamp(1, 8) as usize] = true;
        for m in &self.cams {
            if m.to.map(Part::Reader) == Some(target) && skip != Some(MotionFrom::Cam(m.id)) {
                mark(m.macro_);
            }
            for l in m.more.iter().filter(|l| l.to == target) { mark(l.macro_); }
        }
        for p in &self.probes {
            if p.to == Some(target) && skip != Some(MotionFrom::Probe(p.id)) {
                mark(p.macro_);
            }
            for l in p.more.iter().filter(|l| l.to == target) { mark(l.macro_); }
        }
        let v = self.voice(target)?;
        let mac = v.raw.get("macro").and_then(|m| m.as_array());
        let mods = v.raw.get("mods").and_then(|m| m.as_array());
        for i in 0..8usize {
            let name = format!("m{}", i + 1);
            let knob = mac.and_then(|m| m.get(i)).map(|x| crate::voice::truthy(Some(x))).unwrap_or(false);
            let routed = mods.map_or(false, |ms| ms.iter().any(|r| {
                r.get("src").and_then(|s| s.as_str()) == Some(&name) || r.get("aux").and_then(|s| s.as_str()) == Some(&name)
            }));
            if !used[i + 1] && !knob && !routed {
                return Some(i as i64 + 1);
            }
        }
        (1..=8).find(|&i| !used[i as usize])
    }
    /// Link a cam or probe to one more synth macro, on target's first free one: its first link if it has none.
    /// Ok(the 1-based slot), or Err(why not).
    pub fn add_macro_link(&mut self, from: crate::motion::MotionFrom, target: Part) -> Result<i64, &'static str> {
        use crate::motion::MotionFrom;
        if self.voice(target).map(|v| v.kind) != Some(VoiceKind::Wave) {
            return Err("that part is not a synth");
        }
        let (first, n) = match from {
            MotionFrom::Cam(id) => match self.cams.iter().find(|m| m.id == id) {
                Some(m) => (m.to.map(Part::Reader), m.more.len()),
                None => return Err("no such cam"),
            },
            MotionFrom::Probe(id) => match self.probes.iter().find(|p| p.id == id) {
                Some(p) => (p.to.filter(|&t| self.voice(t).map(|v| v.kind) == Some(VoiceKind::Wave)), p.more.len()),
                None => return Err("no such probe"),
            },
        };
        if first.is_none() {
            let ok = match (from, target) {
                (MotionFrom::Cam(id), Part::Reader(r)) => self.link_cam(id, r),
                (MotionFrom::Probe(id), t) => self.link_probe(id, t),
                _ => false,
            };
            if !ok { return Err("that part can't take this link") }
            return Ok(match from {
                MotionFrom::Cam(id) => self.cams.iter().find(|m| m.id == id).map_or(1, |m| m.macro_),
                MotionFrom::Probe(id) => self.probes.iter().find(|p| p.id == id).map_or(1, |p| p.macro_),
            });
        }
        if n + 1 >= MACRO_LINKS {
            return Err("it already turns 64 macros");
        }
        let Some(slot) = self.pick_macro(target, None) else { return Err("all 8 macros of that synth are taken") };
        let l = MacroLink { to: target, macro_: slot };
        match from {
            MotionFrom::Cam(id) => if let Some(m) = self.cams.iter_mut().find(|m| m.id == id) { m.more.push(l) },
            MotionFrom::Probe(id) => if let Some(p) = self.probes.iter_mut().find(|p| p.id == id) { p.more.push(l) },
        }
        self.solve();
        Ok(slot)
    }
    /// Take away link i of a cam or probe (0: its first; the next becomes first where it can).
    pub fn remove_macro_link(&mut self, from: crate::motion::MotionFrom, i: usize) {
        use crate::motion::MotionFrom;
        match from {
            MotionFrom::Cam(id) => {
                let Some(m) = self.cams.iter_mut().find(|m| m.id == id) else { return };
                if i == 0 {
                    m.to = None;
                    m.to_ix = None;
                    if let Some(k) = m.more.iter().position(|l| matches!(l.to, Part::Reader(_))) {
                        let l = m.more.remove(k);
                        m.to = Some(l.to.id());
                        m.macro_ = l.macro_;
                    }
                } else if i - 1 < m.more.len() {
                    m.more.remove(i - 1);
                }
            }
            MotionFrom::Probe(id) => {
                let Some(p) = self.probes.iter_mut().find(|p| p.id == id) else { return };
                if i == 0 {
                    p.to = None;
                    if !p.more.is_empty() {
                        let l = p.more.remove(0);
                        p.to = Some(l.to);
                        p.macro_ = l.macro_;
                    }
                } else if i - 1 < p.more.len() {
                    p.more.remove(i - 1);
                }
            }
        }
        self.relink();
        self.solve();
    }
    /// Set the 1-based macro slot of link i (0: the first).
    pub fn set_link_macro(&mut self, from: crate::motion::MotionFrom, i: usize, slot: i64) {
        use crate::motion::MotionFrom;
        let slot = slot.clamp(1, WAVE_MACROS as i64);
        match from {
            MotionFrom::Cam(id) => if let Some(m) = self.cams.iter_mut().find(|m| m.id == id) {
                if i == 0 { m.macro_ = slot } else if let Some(l) = m.more.get_mut(i - 1) { l.macro_ = slot }
            },
            MotionFrom::Probe(id) => if let Some(p) = self.probes.iter_mut().find(|p| p.id == id) {
                if i == 0 { p.macro_ = slot } else if let Some(l) = p.more.get_mut(i - 1) { l.macro_ = slot }
            },
        }
    }
    /// linkCam: link a cam to a synth reader, on its first free macro. False if the reader is not a synth.
    pub fn link_cam(&mut self, cam: u32, reader: u32) -> bool {
        let Some(ri) = self.reader_ix(reader) else { return false };
        if self.readers[ri].voice.kind != VoiceKind::Wave {
            return false;
        }
        let Some(mi) = self.cams.iter().position(|m| m.id == cam) else { return false };
        let slot = self.free_macro(Part::Reader(reader), crate::motion::MotionFrom::Cam(cam));
        let m = &mut self.cams[mi];
        m.to = Some(reader);
        m.macro_ = slot;
        self.solve();
        true
    }
    /// linkProbe: a synth gives it a free macro; anything else, a knob (its `param`, or the preferred one).
    pub fn link_probe(&mut self, probe: u32, to: Part) -> bool {
        let Some(pi) = self.probes.iter().position(|p| p.id == probe) else { return false };
        let Some(v) = self.voice(to) else { return false };
        let kind = v.kind;
        if kind == VoiceKind::Wave {
            let slot = self.free_macro(to, crate::motion::MotionFrom::Probe(probe));
            self.probes[pi].to = Some(to);
            self.probes[pi].macro_ = slot;
        } else {
            let pr = &mut self.probes[pi];
            pr.to = Some(to);
            pr.param = crate::voice::probe_def(kind, pr.param.as_deref()).map(|d| d.0.to_string());
        }
        self.solve();
        true
    }
    /// probeAttach: pin probe i onto what is under (x, y): a cam's follower, a ball or an arm when close (in screen px
    /// at `zoom`), otherwise the cog or disc there. Over nothing it comes loose, or with `keep` stays where it was.
    pub fn probe_attach(&mut self, i: usize, x: f64, y: f64, zoom: f64, keep: bool) -> bool {
        let d = |qx: f64, qy: f64| hypot(qx - x, qy - y) * zoom;
        let mut best: Option<(f64, Host, u32, u32, f64, f64)> = None;
        let mut take = |dist: f64, reach: f64, at: (Host, u32, u32, f64, f64)| {
            if dist <= reach && best.map_or(true, |b| dist < b.0) {
                best = Some((dist, at.0, at.1, at.2, at.3, at.4));
            }
        };
        for m in &self.cams {
            let (tx, ty) = self.cam_tip(m);
            take(d(tx, ty), 22.0, (Host::Cam, m.id, 0, 0.0, 0.0));
        }
        for q in &self.pits {
            for b in &q.balls {
                let (bx, by) = crate::pit::ball_at(b, self.clock.view);
                take(d(bx, by) - b.r * zoom, 14.0, (Host::Ball, q.id, b.id, 0.0, 0.0));
            }
        }
        for a in &self.arms {
            let c = &self.cogs[a.cog_ix];
            let ang = self.render_th(a.cog_ix) + a.a;
            let (ux, uy) = (ang.cos(), ang.sin());
            let r = clamp((x - c.x) * ux + (y - c.y) * uy, a.r0, a.r0 + a.len);
            take(d(c.x + ux * r, c.y + uy * r), 14.0, (Host::Arm, a.id, 0, r, 0.0));
        }
        let mut at = best.map(|b| (b.1, b.2, b.3, b.4, b.5));
        if at.is_none() {
            if let Some(ci) = self.pick_cog(x, y, false, 0.0) {
                let c = &self.cogs[ci];
                at = Some((Host::Cog, c.id, 0, hypot(x - c.x, y - c.y), (y - c.y).atan2(x - c.x) - self.render_th(ci)));
            }
        }
        let pr = &mut self.probes[i];
        let Some((host, on, ball, r, a)) = at else {
            if !keep {
                pr.host = None;
                pr.on_ix = None;
            }
            return false;
        };
        let was = pr.host;
        pr.host = Some(host);
        pr.on = on;
        pr.ball = ball;
        pr.r = r;
        pr.a = a;
        pr.key = [f64::NAN; 13];
        if pr.host != was {
            if host == Host::Cam {
                pr.axis = Axis::Reach;
            } else if pr.axis == Axis::Reach && host != Host::Cog {
                pr.axis = Axis::Y;
            }
        }
        self.solve();
        true
    }
    /// Seat a reader on a disc (or unseat it with None), where it lies now.
    pub fn seat(&mut self, r: usize, disc: Option<usize>) {
        match disc {
            Some(c) if self.cogs[c].track.is_some() => {
                let c = self.cogs[c].clone();
                seat_reader(&mut self.readers[r], &c);
            }
            _ => {
                self.readers[r].on = None;
                self.readers[r].seat = None;
            }
        }
        self.solve();
    }
    /// Replace a part's voice from JSON (a patch loaded, a kind changed).
    pub fn set_voice(&mut self, p: Part, v: &Value) -> Result<(), String> {
        let nv = Voice::import(v, p.id())?;
        let slot = self.voice_mut(p).ok_or("no such part")?;
        let ver = slot.ver;
        *slot = nv;
        slot.ver = ver.wrapping_add(1);
        Ok(())
    }
}
