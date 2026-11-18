//! Recording (midi.js rec*): what the keyboard plays lands on the plate as pins, where the reader was when you heard
//! yourself play it, so it comes back round in time. A take adds to what is there and is one undo step.
//! The audio thread stamps each key with the sim time heard alongside it (`RecEv::at`); the UI applies it here.
use crate::stage::Sel;
use cogs_sim::{scale_ix, scale_note, Dot, World, SCALES};
use std::collections::HashMap;
use std::f64::consts::{PI, TAU};

#[derive(Clone, Copy, Debug)]
pub struct RecEv { pub on: bool, pub note: u8, pub vel: u8, pub at: f64 }

const MIN_LEN: f64 = 1. / 16.;
const TOP_RANGE: f64 = 3.;
const OCT: (f64, f64) = (1., 7.);

fn wrap(a: f64) -> f64 { a.rem_euclid(TAU) }
fn tidy(x: f64) -> f64 { (x * 1e6).round() / 1e6 }
fn same_cell(ring: f64, a: f64, d: &Dot) -> bool { d.ring == ring && (wrap(d.a - a + PI) - PI).abs() < 1e-6 }

struct Open { ring: f64, a: f64, at: f64 }

pub struct Rec {
    pub cog: u32,
    pub reader: u32,
    open: HashMap<u8, Open>,
    pub n: u32,
    moved: u32,
    pub grew: bool,
}

/// What record would write to: the selected plate and its reader, or the selected reader and its plate.
pub fn target(w: &World, sel: Option<Sel>) -> Option<(u32, Option<u32>)> {
    match sel? {
        Sel::Cog(c) | Sel::Dot { cog: c, .. } => { w.cog(c)?.track.as_ref()?; Some((c, w.readers.iter().find(|r| r.on == Some(c)).map(|r| r.id))) }
        Sel::Reader(i) => { let r = w.readers.iter().find(|r| r.id == i)?; let c = r.on?; w.cog(c)?.track.as_ref()?; Some((c, Some(i))) }
        _ => None,
    }
}

/// Where a reader meets the circle of radius R about (cx, cy): its world angle, and how far along the reader.
fn crossing(cx: f64, cy: f64, r: f64, x1: f64, y1: f64, x2: f64, y2: f64) -> Option<(f64, f64)> {
    let (dx, dy, fx, fy) = (x2 - x1, y2 - y1, x1 - cx, y1 - cy);
    let a = dx * dx + dy * dy; let b = 2. * (fx * dx + fy * dy); let d = b * b - 4. * a * (fx * fx + fy * fy - r * r);
    if a == 0. || d < 0. { return None }
    for u in [(-b - d.sqrt()) / (2. * a), (-b + d.sqrt()) / (2. * a)] { if (0. ..=1.).contains(&u) { return Some((u, (fy + dy * u).atan2(fx + dx * u))) } }
    None
}

impl Rec {
    pub fn start(w: &mut World, sel: Option<Sel>, have_keyboard: bool) -> Result<(Rec, String), String> {
        let (c, rd) = target(w, sel).ok_or("select a plate or its reader to record")?;
        let rd = rd.ok_or("put a reader on this plate to record")?;
        if w.clock.paused { w.set_paused(false); }
        let turning = w.cog(c).map_or(false, |c| c.w != 0.);
        let msg = if !turning { format!("plate {c} is not turning: give it a motor to record") }
            else if have_keyboard { format!("recording onto plate {c}") } else { format!("recording onto plate {c}: connect a keyboard to play") };
        Ok((Rec { cog: c, reader: rd, open: HashMap::new(), n: 0, moved: 0, grew: false }, msg))
    }
    pub fn live(&self, w: &World) -> bool { w.cog(self.cog).map_or(false, |c| c.track.is_some()) && w.readers.iter().any(|r| r.id == self.reader) }

    /// A key down, heard at sim time `at`. Returns a message when nothing could be written.
    pub fn note_on(&mut self, w: &mut World, note: u8, vel: u8, at: f64, velcurve: f64) -> Option<String> {
        let note_f = note as f64;
        let paused = w.clock.paused;
        let sim_t = w.clock.sim_t;
        let rd = w.readers.iter().find(|r| r.id == self.reader)?.clone();
        let c = w.cog_mut(self.cog)?;
        if paused { return Some("paused: press play to record".into()) }
        if c.w == 0. { return Some(format!("plate {} is not turning: nothing recorded", c.id)) }
        // the plate makes room for the note: an empty one moves to its octave, then its range grows
        if rd.tines == 0. {
            let t = c.track.as_mut()?;
            let (n0, b0, l) = (t.rings(), t.base(), SCALES[t.scale].1.len() as f64);
            let top = |t: &cogs_sim::Track| t.ring_note(t.rings() as i64 - 1);
            if t.dots.is_empty() && (note_f < t.base() || note_f > top(t)) { t.octave = (((note_f - t.root) / 12.).floor() - 1.).clamp(OCT.0, OCT.1) }
            while note_f < t.base() && t.range < TOP_RANGE && t.octave > OCT.0 { t.octave -= 1.; t.range += 1.; for d in &mut t.dots { d.ring += l } }
            while note_f > top(t) && t.range < TOP_RANGE { t.range += 1. }
            if t.rings() != n0 || t.base() != b0 { self.grew = true }
        }
        // the rings this reader can play, each with the note it sounds there (a comb's from where it is crossed)
        let t = c.track.as_ref()?;
        let comb = scale_ix(&rd.comb_scale).unwrap_or(0);
        let rings: Vec<(f64, f64, f64)> = (0..t.rings()).filter_map(|ring| {
            let (u, phi) = crossing(c.x, c.y, c.ring_r(ring as f64), rd.x1, rd.y1, rd.x2, rd.y2)?;
            let n = if rd.tines > 0. { scale_note(rd.comb_base, comb, ((u * rd.tines).floor()).min(rd.tines - 1.) as i64) } else { t.ring_note(ring as i64) };
            Some((ring as f64, phi, n))
        }).collect();
        if rings.is_empty() { return Some("the reader does not reach this plate's rings".into()) }
        // the nearest ring to the note, folded by octaves into what the reader reaches
        let lo = rings.iter().map(|r| r.2).fold(f64::INFINITY, f64::min);
        let hi = rings.iter().map(|r| r.2).fold(f64::NEG_INFINITY, f64::max);
        let mut n = note_f;
        while n < lo { n += 12. }
        while n > hi && n - 12. >= lo { n -= 12. }
        let hit = rings.iter().copied().reduce(|a, b| if (b.2 - n).abs() < (a.2 - n).abs() { b } else { a })?;
        let a = wrap(hit.1 - (c.th - c.w * (sim_t - at)));
        let vs = rd.voice.vel_sense;
        let vx = vel as f64 / 127.;
        let v = (1. - vs + vs * vx.powf(velcurve)).clamp(0.02, 1.);
        let t = c.track.as_mut()?;
        t.dots.retain(|d| !same_cell(hit.0, a, d));
        t.dots.push(Dot { ring: hit.0, a, p: 1., v, len: 1., skip: f64::INFINITY, flash: at });
        self.open.insert(note, Open { ring: hit.0, a, at });
        self.n += 1;
        if hit.2 != note_f { self.moved += 1 }
        None
    }
    /// A key up, heard at `at`: its pin takes the length it was held.
    pub fn note_off(&mut self, w: &mut World, note: u8, at: f64) {
        let Some(h) = self.open.remove(&note) else { return };
        Self::close(w, self.cog, &h, at);
    }
    fn close(w: &mut World, cog: u32, h: &Open, at: f64) {
        let Some(c) = w.cog_mut(cog) else { return };
        let cw = c.w.abs();
        let Some(t) = c.track.as_mut() else { return };
        let steps = (at - h.at) * cw * t.div / TAU;
        let len = tidy(steps).clamp(MIN_LEN, t.div);
        if let Some(d) = t.dots.iter_mut().find(|d| same_cell(h.ring, h.a, d)) { d.len = len; d.skip = f64::NEG_INFINITY; }
    }
    /// End the take: held keys close now. Returns what to say.
    pub fn stop(mut self, w: &mut World, at: f64) -> String {
        if !self.live(w) { return "recording stopped: its plate is gone".into() }
        for (_, h) in self.open.drain() { Self::close(w, self.cog, &h, at) }
        let s = if self.n == 1 { "" } else { "s" };
        let moved = if self.moved > 0 { format!(" ({} moved to fit its scale or reach)", self.moved) } else { String::new() };
        format!("{} note{s} on plate {}{moved}", self.n, self.cog)
    }
}
