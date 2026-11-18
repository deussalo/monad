//! The stage: the machine on screen (stage.js and phone.js). It draws the world, moves the camera, and turns touches and
//! the mouse into edits: dragging parts, carrying them from and to the shelf, rims, the speed knob, two-finger grabs,
//! pinch zoom, the marquee, seated readers and pins on a disc at note zoom. Undo is here too (`edits`).
//!
//! The app holds the world behind a mutex shared with the audio callback; it locks it for `Stage::show` each frame and
//! passes the heard time. What the stage wants of the rest of the app (a buzz, a note to audition, the panel, a save)
//! comes back as `StageOut`s. Bezels, the notes roll, the synth editor and panels come later: `StageOut::OpenEditor`
//! and `StageOut::TogglePanel` are their hooks, and `Stage::bezel_rect` will reserve their hit area.

pub mod bezel;
pub mod draw;
pub mod edits;
mod gesture;
mod icon_data;
pub mod icons;
pub mod pen;
pub mod roll;
mod shelf;
pub mod sheet;

use cogs_sim::{Part, PartRef, World, TAU};
use draw::*;
use egui::{pos2, Color32, Pos2, Rect};
use gesture::{Gesture, Grip};
use pen::*;
use std::collections::BTreeMap;

pub const ZMIN: f64 = 0.1;
pub const ZMAX: f64 = 8.0;
const FLY_DUR: f64 = 0.38;
pub const SHELF_H: f32 = 76.0;
pub const INTERACTION_H: f32 = 36.0;
const KNOB_R: f64 = 11.0;
const UNSEAT_PX: f64 = 36.0;
const SPREAD_DEAD: f64 = 10.0;
const ZERO_CATCH: f64 = 0.07;

/// What is selected: a part by kind and id, or a pin on a disc.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sel {
    Cog(u32),
    Arm(u32),
    Pad(u32),
    Reader(u32),
    String(u32),
    Belt(u32),
    Cam(u32),
    Pit(u32),
    Probe(u32),
    Field(u32),
    Dot { cog: u32, ring: f64, a: f64 },
}
impl Sel {
    pub fn part(self) -> Option<PartRef> {
        Some(match self {
            Sel::Cog(i) => PartRef::Cog(i),
            Sel::Arm(i) => PartRef::Arm(i),
            Sel::Pad(i) => PartRef::Pad(i),
            Sel::Reader(i) => PartRef::Reader(i),
            Sel::String(i) => PartRef::String(i),
            Sel::Belt(i) => PartRef::Belt(i),
            Sel::Cam(i) => PartRef::Cam(i),
            Sel::Pit(i) => PartRef::Pit(i),
            Sel::Probe(i) => PartRef::Probe(i),
            Sel::Field(i) => PartRef::Field(i),
            Sel::Dot { .. } => return None,
        })
    }
    pub fn name(self) -> &'static str {
        match self {
            Sel::Cog(_) => "cog",
            Sel::Arm(_) => "arm",
            Sel::Pad(_) => "pad",
            Sel::Reader(_) => "reader",
            Sel::String(_) => "string",
            Sel::Belt(_) => "belt",
            Sel::Cam(_) => "cam",
            Sel::Pit(_) => "sphere",
            Sel::Probe(_) => "probe",
            Sel::Field(_) => "fx",
            Sel::Dot { .. } => "pin",
        }
    }
}

/// What the stage asks of the app.
#[derive(Clone, Debug, PartialEq)]
pub enum StageOut {
    /// A haptic tick, in ms.
    Buzz(u32),
    /// Play a note now at a middling velocity, as the sim would strike it (stage.js sound): on a voiced part, or on the
    /// default pluck when `part` is None (a disc with no reader).
    Audition { part: Option<Part>, midi: f64, key: u32 },
    /// The world changed and settled into an undo step: save it.
    Changed,
    /// Play or pause; `from_start` when play begins on the start (schedule the first step so a pin on the line sounds).
    Paused { paused: bool, from_start: bool },
    /// The glyph was held: open or close the panel.
    TogglePanel,
    /// Open the selection's editor: "notes", "patch" or "shape".
    OpenEditor(&'static str),
    /// Connect a MIDI keyboard.
    ConnectMidi,
    /// Start or stop recording onto the plate.
    Record,
    /// The seal was pressed: copy this share link (the HTML build opens it).
    Share(String),
    /// Stop an audition (the roll's preview) `after` seconds from now.
    AuditionOff { key: u32, after: f64 },
    /// Save this cogs file (the panel's save file).
    SaveFile { name: String, text: String },
    /// Ask for a cogs file to open; hand its text to `Stage::open_text`.
    OpenFile,
}

/// What the app tells the stage each frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct StageIo {
    /// The heard time in sim seconds: what is drawn (clock.view).
    pub heard: f64,
    /// Safe-area insets, in points.
    pub safe_top: f32,
    pub safe_bottom: f32,
    /// The panel or an editor sheet covers the stage (the thumb row hides).
    pub busy: bool,
    /// Recording onto a plate now, and whether there is something to record onto.
    pub recording: bool,
    pub can_record: bool,
    /// What the MIDI button says ("MIDI", or the connected devices).
    pub midi_label: Option<&'static str>,
    pub midi_connected: bool,
}

#[derive(Clone, Copy, Debug)]
struct Fly {
    x0: f64,
    y0: f64,
    z0: f64,
    x1: f64,
    y1: f64,
    z1: f64,
    t0: f64,
}

#[derive(Clone, Copy, Debug)]
struct Finger {
    pos: (f64, f64),
    #[allow(dead_code)]
    touch: bool,
}

struct Active {
    id: u64,
    g: Gesture,
    s0: (f64, f64),
    last: (f64, f64),
    t0: f64,
    moved: bool,
}

struct Pinch {
    d: f64,
    zoom: f64,
    anchor: (f64, f64),
}

struct GripState {
    a: u64,
    b: u64,
    hand: Grip,
    mid0: (f64, f64),
    d0: f64,
}

/// The speed knob being scrubbed: a hand on a turntable, sprung toward the drag's speed.
struct Spin {
    c: u32,
    held: bool,
    t: f64,
    rpm: f64,
    v: f64,
    samples: Vec<(f64, f64)>,
}

#[derive(Clone, Copy)]
struct LastTap {
    t: f64,
    s: (f64, f64),
    o: Option<Sel>,
}

pub struct Stage {
    pub sel: Option<Sel>,
    pub group: Vec<Sel>,
    /// The room's bezel is up (a tap on empty ground with nothing selected).
    pub room: bool,
    /// A part armed from the keyboard: the next press places it.
    pub armed: Option<&'static str>,
    marquee: Option<((f64, f64), (f64, f64))>,
    fly: Option<Fly>,
    fingers: BTreeMap<u64, Finger>,
    gesture: Option<Active>,
    pinch: Option<Pinch>,
    grip: Option<GripState>,
    spin: Option<Spin>,
    /// The knob being turned: its cog and angle on screen.
    knob: Option<(u32, f64)>,
    /// A motor's speed dial is up.
    crank: Option<u32>,
    /// A part carried in from the shelf: its ghost (an arm with nothing to ride yet) and a belt's band.
    carry_ghost: Option<(f64, f64)>,
    carrying: bool,
    band: Option<((f64, f64), (f64, f64))>,
    pit_band: Option<((f64, f64), (f64, f64))>,
    /// The finger is over the shelf with a part: letting go there deletes it.
    scrap: bool,
    snapped: bool,
    caught: bool,
    last_tap: Option<LastTap>,
    /// The pin selected by a hold, and where.
    dot_at: (f64, f64),
    hover: Option<(f64, f64)>,
    shift: bool,
    pub edits: edits::Edits,
    out: Vec<StageOut>,
    // layout, per frame
    rect: Rect,
    cw: f64,
    ch: f64,
    safe_top: f64,
    safe_bot: f64,
    now: f64,
    io: StageIo,
    // the shelf
    shelf_x: f64,
    shelf_fling: Option<(f64, f64)>,
    icons: icons::Icons,
    readout: Option<(String, f64)>,
    readout_held: bool,
    mouse_down: bool,
    seen_touch: bool,
    started: bool,
    trackpad: Option<(Option<u32>, f64, f64, f64, f64)>,
    down_t: f64,
    clip: Option<serde_json::Value>,
    bezel_owner: Option<Option<Sel>>,
    bezel_at: f64,
    seal_at: f64,
    /// A cam or probe waiting for the next tap to link it.
    linking: Option<Sel>,
    pub sheet: Option<sheet::Sheet>,
    pub roll: Option<roll::Roll>,
    pub brush: sheet::Brush,
    sheet_fingers: BTreeMap<u64, sheet::Owner>,
    json: String,
    file_msg: Option<String>,
    snap_pop: bool,
    /// The notes sheet's open popover and what opened it; where it was drawn.
    pop: Option<(sheet::Pop, Rect)>,
    pop_rect: Option<Rect>,
    /// The view moved aside for the panel (1) or the room ring (2): what to come back to, and what it was moved to.
    framed: Option<Framed>,
    /// The fit last applied by `fit_now`, and the layout it was for: while the view still stands there, a change of
    /// size or safe area fits again.
    auto_fit: Option<((f64, f64, f64), [f64; 4])>,
    /// A destructive panel button pressed once: which, and when.
    confirm: Option<(&'static str, f64)>,
}

#[derive(Clone, Copy, Debug)]
struct Framed {
    kind: u8,
    home: (f64, f64, f64),
    at: (f64, f64, f64),
}

impl Default for Stage {
    fn default() -> Self {
        Stage::new()
    }
}

impl Stage {
    pub fn new() -> Stage {
        Stage {
            sel: None, group: Vec::new(), room: false, armed: None, marquee: None, fly: None, fingers: BTreeMap::new(), gesture: None,
            pinch: None, grip: None, spin: None, knob: None, crank: None, carry_ghost: None, carrying: false, band: None, pit_band: None,
            scrap: false, snapped: false, caught: false, last_tap: None, dot_at: (0.0, 0.0), hover: None, shift: false,
            edits: edits::Edits::default(), out: Vec::new(), rect: Rect::NOTHING, cw: 0.0, ch: 0.0, safe_top: 0.0, safe_bot: 0.0, now: 0.0,
            io: StageIo::default(), shelf_x: 0.0, shelf_fling: None, icons: icons::Icons::default(), readout: None, readout_held: false,
            mouse_down: false, seen_touch: false, started: false, trackpad: None, down_t: 0.0, clip: None, bezel_owner: None, bezel_at: 0.0, seal_at: -9.0, linking: None, sheet: None, roll: None, brush: sheet::Brush::default(), sheet_fingers: BTreeMap::new(), json: String::new(), file_msg: None, snap_pop: false, pop: None, pop_rect: None, framed: None, auto_fit: None, confirm: None,
        }
    }

    /// Draw the stage into the ui's whole rect and take its input. Returns what the app should do.
    pub fn show(&mut self, ui: &mut egui::Ui, w: &mut World, io: &StageIo) -> Vec<StageOut> {
        self.show_with(ui, w, io, &mut |_, _| {})
    }
    /// As `show`, with the app's own rows for the panel (worlds, keyboard, audio readouts): `extra` draws them.
    pub fn show_with(&mut self, ui: &mut egui::Ui, w: &mut World, io: &StageIo, extra: &mut dyn FnMut(&mut egui::Ui, &mut World)) -> Vec<StageOut> {
        let rect = ui.max_rect();
        let resp = ui.allocate_rect(rect, egui::Sense::click_and_drag());
        let _ = resp;
        self.rect = rect;
        self.cw = rect.width() as f64;
        self.ch = rect.height() as f64;
        self.safe_top = io.safe_top as f64;
        self.safe_bot = io.safe_bottom as f64;
        self.io = *io;
        if self.sheet_open() {
            self.io.busy = true;
        }
        self.now = ui.input(|i| i.time);
        if !self.started {
            self.started = true;
            self.edits.reset(w);
        }
        if !w.clock.paused {
            w.clock.view = w.clock.view.max(io.heard);
        }
        self.validate(w);
        self.refit_if_moved(w);
        self.input(ui, w);
        self.tick(w);
        let painter = ui.painter_at(rect);
        self.draw(&painter, w);
        self.show_sheet(ui, w, extra);
        self.draw_readout(&painter);
        // a pending change settles into an undo step once things are quiet and no finger is down
        if let Some(t) = self.edits.pending {
            if self.fingers.is_empty() && !self.mouse_down && self.now - t >= edits::EDITS_QUIET && self.edits.commit(w) {
                self.out.push(StageOut::Changed);
            }
        }
        ui.ctx().request_repaint();
        std::mem::take(&mut self.out)
    }

    /// The Back button: close the open popover, else the sheet, else the room ring or a pending link, else clear the
    /// selection. False when there was nothing to close.
    pub fn back(&mut self, w: &mut World) -> bool {
        self.validate(w);
        if self.pop.take().is_some() {
            self.pop_rect = None;
            return true;
        }
        if self.sheet.take().is_some() {
            return true;
        }
        if self.linking.take().is_some() || self.armed.take().is_some() {
            self.readout = None;
            return true;
        }
        if self.room {
            self.room = false;
            return true;
        }
        if self.sel.is_some() || !self.group.is_empty() {
            self.select(None);
            return true;
        }
        false
    }
    /// Say the world was changed from outside the stage (a panel, a bezel): it becomes an undo step when quiet.
    pub fn mark_changed(&mut self) {
        self.edits.mark(self.now);
    }
    pub fn undo(&mut self, w: &mut World) {
        if self.edits.undo(w) {
            self.validate(w);
            self.buzz(6);
            self.out.push(StageOut::Changed);
        }
    }
    pub fn redo(&mut self, w: &mut World) {
        if self.edits.redo(w) {
            self.validate(w);
            self.buzz(6);
            self.out.push(StageOut::Changed);
        }
    }
    /// A world was loaded whole: forget the selection and the history.
    pub fn loaded(&mut self, w: &World) {
        self.sel = None;
        self.group.clear();
        self.gesture = None;
        self.edits.reset(w);
    }

    // ---------- geometry ----------
    pub(crate) fn to_screen(&self, w: &World, x: f64, y: f64) -> (f64, f64) {
        ((x - w.view.x) * w.s.zoom + self.cw / 2.0, (y - w.view.y) * w.s.zoom + self.ch / 2.0)
    }
    pub(crate) fn to_world(&self, w: &World, s: (f64, f64)) -> (f64, f64) {
        ((s.0 - self.cw / 2.0) / w.s.zoom + w.view.x, (s.1 - self.ch / 2.0) / w.s.zoom + w.view.y)
    }
    pub(crate) fn shelf_top(&self) -> f64 {
        self.ch - SHELF_H as f64 - INTERACTION_H as f64 - self.safe_bot
    }
    /// The top of the two rows of controls over the shelf.
    pub(crate) fn thumb_top(&self) -> f64 {
        self.shelf_top() - 108.0
    }
    /// The part of the screen the machine can be seen in: under the status bar, above the controls or an open panel.
    pub(crate) fn free_area(&self) -> (f64, f64) {
        let t = self.safe_top;
        let b = match &self.sheet {
            Some(s) if !s.min => self.sheet_top().unwrap_or(self.ch) - 6.0,
            _ => self.thumb_top(),
        };
        (t, b.max(t + 60.0))
    }
    pub(crate) fn off_stage(&self, s: (f64, f64)) -> bool {
        s.1 >= self.shelf_top()
    }
    fn pen<'a>(&self, p: &'a egui::Painter, w: &World) -> Pen<'a> {
        let z = w.s.zoom;
        Pen::new(p, z, self.rect.min.x as f64 + self.cw / 2.0 - w.view.x * z, self.rect.min.y as f64 + self.ch / 2.0 - w.view.y * z, w.clock.view)
    }
    pub(crate) fn scr(&self, s: (f64, f64)) -> Pos2 {
        pos2(self.rect.min.x + s.0 as f32, self.rect.min.y + s.1 as f32)
    }

    // ---------- selection ----------
    pub fn is_sel(&self, o: Sel) -> bool {
        self.sel == Some(o) || self.group.contains(&o)
    }
    pub(crate) fn select(&mut self, s: Option<Sel>) {
        if s.is_some() {
            self.room = false;
        }
        self.sel = s;
        self.group.clear();
    }
    /// Drop a selection whose part has gone (an undo, a delete).
    fn validate(&mut self, w: &World) {
        let ok = |s: &Sel| exists(w, *s);
        if let Some(s) = self.sel {
            if !ok(&s) {
                self.sel = None;
            }
        }
        self.group.retain(ok);
    }
    pub(crate) fn buzz(&mut self, ms: u32) {
        self.out.push(StageOut::Buzz(ms));
    }
    pub(crate) fn changed(&mut self, w: &mut World, structural: bool) {
        if structural {
            w.solve();
        }
        self.edits.mark(self.now);
    }
    pub(crate) fn say(&mut self, text: String, held: bool) {
        self.readout = Some((text, if held { f64::INFINITY } else { self.now + 1.8 }));
        self.readout_held = held;
    }

    // ---------- the camera ----------
    pub(crate) fn fly_to(&mut self, w: &World, x: f64, y: f64, z: f64) {
        self.fly = Some(Fly { x0: w.view.x, y0: w.view.y, z0: w.s.zoom, x1: x, y1: y, z1: z.clamp(ZMIN, ZMAX), t0: self.now });
    }
    fn step_camera(&mut self, w: &mut World) {
        let Some(m) = self.fly else { return };
        let k = ((self.now - m.t0) / FLY_DUR).clamp(0.0, 1.0);
        let e = 1.0 - (1.0 - k).powi(3);
        w.s.zoom = m.z0 * (m.z1 / m.z0).powf(e);
        w.view.x = m.x0 + (m.x1 - m.x0) * e;
        w.view.y = m.y0 + (m.y1 - m.y0) * e;
        if k >= 1.0 {
            self.fly = None;
        }
    }
    pub(crate) fn zoom_at(&self, w: &mut World, s: (f64, f64), zoom: f64, anchor: (f64, f64)) {
        w.s.zoom = zoom.clamp(ZMIN, ZMAX);
        w.view.x = anchor.0 - (s.0 - self.cw / 2.0) / w.s.zoom;
        w.view.y = anchor.1 - (s.1 - self.ch / 2.0) / w.s.zoom;
    }
    /// The view centre that puts world point p at the middle of the open stage.
    pub(crate) fn centred(&self, p: (f64, f64), z: f64) -> (f64, f64) {
        let (t, b) = self.free_area();
        (p.0, p.1 - ((t + b) / 2.0 - self.ch / 2.0) / z)
    }
    /// fitTarget: the whole machine in the open stage.
    pub fn fit_target(&self, w: &World) -> (f64, f64, f64, f64) {
        let Some((cx, cy, bw, bh)) = bounds(w) else { return (0.0, 0.0, 1.0, 1.0) };
        let (t, b) = self.free_area();
        let fit = ((self.cw - 24.0) / bw).min((b - t - 24.0) / bh);
        let z = fit.clamp(ZMIN, 2.0);
        let c = self.centred((cx, cy), z);
        // shrunk into the room ring on purpose: the listener is not to hear that as walking away
        let fit = match self.framed {
            Some(f) if f.kind == 2 => fit.min(w.s.zoom),
            _ => fit,
        };
        (c.0, c.1, z, fit)
    }
    fn fit_layout(&self) -> [f64; 4] {
        [self.cw, self.ch, self.safe_top, self.safe_bot]
    }
    /// The real safe areas or a new window size arrived after the fit: fit again, unless the view has been moved.
    fn refit_if_moved(&mut self, w: &mut World) {
        let Some((at, layout)) = self.auto_fit else { return };
        if layout == self.fit_layout() {
            return;
        }
        if !same_view(w, at) {
            self.auto_fit = None;
            return;
        }
        let f = self.fit_target(w);
        w.view.x = f.0;
        w.view.y = f.1;
        w.s.zoom = f.2;
        self.auto_fit = Some(((f.0, f.1, f.2), self.fit_layout()));
    }
    /// The panel and the room ring move the machine into the space they leave, and put it back when they go,
    /// unless it has been moved meanwhile.
    fn step_frame(&mut self, w: &World) {
        let panel = self.sheet.as_ref().map_or(false, |s| s.kind == sheet::SheetKind::Panel && !s.min && s_not_dragged(s));
        let want = if panel { 1 } else if self.room && self.sel.is_none() { 2 } else { 0 };
        let cur = self.framed.map_or(0, |f| f.kind);
        if want == cur || (want == 0 && self.sheet.as_ref().map_or(false, |s| s.kind == sheet::SheetKind::Panel && !s.min)) {
            return;
        }
        let was = self.framed.take();
        let unmoved = was.map_or(false, |f| same_view(w, f.at) || self.fly.map_or(false, |m| m.x1 == f.at.0 && m.y1 == f.at.1 && m.z1 == f.at.2));
        let home = match was {
            Some(f) if unmoved => f.home,
            _ => (w.view.x, w.view.y, w.s.zoom),
        };
        if want == 0 {
            if unmoved {
                self.fly_to(w, home.0, home.1, home.2);
            }
            return;
        }
        let at = if want == 1 {
            let f = self.fit_target(w);
            (f.0, f.1, f.2)
        } else {
            self.room_target(w)
        };
        self.framed = Some(Framed { kind: want, home, at });
        self.fly_to(w, at.0, at.1, at.2);
    }
    /// No part of the machine is in the free area: the fit button lights.
    pub(crate) fn machine_lost(&self, w: &World) -> bool {
        let Some((cx, cy, bw, bh)) = bounds(w) else { return false };
        let (t, b) = self.free_area();
        let a = self.to_screen(w, cx - bw / 2.0, cy - bh / 2.0);
        let c = self.to_screen(w, cx + bw / 2.0, cy + bh / 2.0);
        c.0 < 0.0 || a.0 > self.cw || c.1 < t || a.1 > b
    }
    /// Put the whole machine in view now (the demo's start).
    pub fn fit_now(&mut self, w: &mut World, size: egui::Vec2, io: &StageIo) {
        self.cw = size.x as f64;
        self.ch = size.y as f64;
        self.safe_top = io.safe_top as f64;
        self.safe_bot = io.safe_bottom as f64;
        let f = self.fit_target(w);
        w.view.x = f.0;
        w.view.y = f.1;
        w.s.zoom = f.2;
        self.auto_fit = Some(((f.0, f.1, f.2), self.fit_layout()));
    }
    pub(crate) fn fly_fit(&mut self, w: &World) {
        let f = self.fit_target(w);
        self.fly_to(w, f.0, f.1, f.2);
    }
    /// Double tap: fly to the part's working zoom, or back out when already there.
    pub(crate) fn focus(&mut self, w: &World, o: Sel, s: (f64, f64)) {
        let (z, at) = match o {
            Sel::Pad(i) => match w.pads.iter().find(|p| p.id == i) {
                Some(p) => (30.0 / p.r, (p.x, p.y)),
                None => return,
            },
            Sel::Reader(i) | Sel::String(i) => {
                let seg = if let Sel::Reader(_) = o {
                    w.readers.iter().find(|r| r.id == i).map(|r| (r.x1, r.y1, r.x2, r.y2))
                } else {
                    w.strings.iter().find(|r| r.id == i).map(|r| (r.x1, r.y1, r.x2, r.y2))
                };
                let Some((x1, y1, x2, y2)) = seg else { return };
                (170.0 / (x2 - x1).hypot(y2 - y1).max(20.0), ((x1 + x2) / 2.0, (y1 + y2) / 2.0))
            }
            Sel::Arm(i) => {
                let Some(a) = w.arms.iter().find(|a| a.id == i) else { return };
                let c = &w.cogs[a.cog_ix];
                (150.0 / (a.r0 + a.len), (c.x, c.y))
            }
            Sel::Pit(i) => {
                let Some(p) = w.pits.iter().find(|a| a.id == i) else { return };
                ((self.cw * 0.8 / p.w).min((self.shelf_top() - 40.0) * 0.8 / p.h), (p.x, p.y))
            }
            Sel::Field(i) => {
                let Some(f) = w.fields.iter().find(|a| a.id == i) else { return };
                ((self.cw * 0.8).min((self.shelf_top() - 40.0) * 0.8) / (2.0 * f.r), (f.x, f.y))
            }
            Sel::Cog(i) => {
                let Some(c) = w.cog(i) else { return };
                if c.track.is_some() {
                    let z = 14.0 / ring_gap(c);
                    let q = self.to_world(w, s);
                    if w.s.zoom < z * 0.9 {
                        self.fly_to(w, q.0 - (s.0 - self.cw / 2.0) / z, q.1 - (s.1 - self.ch / 2.0) / z, z);
                        return;
                    }
                    (z, (c.x, c.y))
                } else {
                    (120.0 / c.radius(), (c.x, c.y))
                }
            }
            _ => return,
        };
        if w.s.zoom >= z * 0.9 {
            self.fly_fit(w);
            return;
        }
        let c = self.centred(at, z);
        self.fly_to(w, c.0, c.1, z);
    }

    // ---------- the knob ----------
    pub(crate) fn knob_cog(&self, w: &World) -> Option<u32> {
        if let Some((c, _)) = self.knob {
            return Some(c);
        }
        match self.sel {
            Some(Sel::Cog(c)) if w.cog(c).is_some() && self.group.len() < 2 => Some(c),
            _ => None,
        }
    }
    /// The knob's place on screen: on the face above the hub, or just past the rim when the cog is small.
    pub(crate) fn knob_at(&self, w: &World, c: u32, a: f64) -> ((f64, f64), (f64, f64)) {
        let cg = w.cog(c).unwrap();
        let s = self.to_screen(w, cg.x, cg.y);
        let rs = cg.radius() * w.s.zoom;
        let d = if rs >= 48.0 { rs * 0.55 } else { rs + KNOB_R + 8.0 };
        ((s.0 + a.cos() * d, s.1 + a.sin() * d), s)
    }
    pub(crate) fn catch_zero(&mut self, a: f64) -> f64 {
        let on = wrap_pi(a).abs() < ZERO_CATCH;
        if on && !self.caught {
            self.buzz(8);
        }
        self.caught = on;
        if on { 0.0 } else { a }
    }

    // ---------- each frame ----------
    fn tick(&mut self, w: &mut World) {
        self.step_frame(w);
        self.step_camera(w);
        self.step_spin(w);
        self.step_holds(w);
        self.step_shelf();
    }
    fn step_spin(&mut self, w: &mut World) {
        const SPIN_HOLD: f64 = 26.0;
        const SPIN_FREE: f64 = 7.0;
        const SPIN_DAMP: f64 = 0.8;
        let Some(p) = &mut self.spin else { return };
        let Some(ci) = w.cog_ix(p.c) else {
            self.spin = None;
            return;
        };
        let t = self.now;
        let dt = (t - p.t).clamp(0.0, 0.1);
        p.t = t;
        let goal = if p.held { drag_rpm(p, t) } else { set_rpm(w, ci) };
        let w0 = if p.held { SPIN_HOLD } else { SPIN_FREE };
        let n = (dt * 240.0).ceil().max(1.0) as usize;
        let h = dt / n as f64;
        for _ in 0..n {
            p.v += (w0 * w0 * (goal - p.rpm) - 2.0 * SPIN_DAMP * w0 * p.v) * h;
            p.rpm += p.v * h;
        }
        if !p.held && (p.rpm - goal).abs() < 0.01 * goal.abs().max(1.0) && p.v.abs() < 0.5 {
            w.retimed(|w| {
                w.cogs[ci].scrub = None;
                w.solve();
            });
            self.spin = None;
            return;
        }
        let rpm = p.rpm;
        w.retimed(|w| {
            w.cogs[ci].scrub = Some(rpm);
            w.solve();
        });
    }

    // ---------- drawing ----------
    fn draw(&mut self, painter: &egui::Painter, w: &World) {
        painter.rect_filled(self.rect, 0.0, BG);
        let pen = self.pen(painter, w);
        let mut marks = Marks::default();
        if let Some(Sel::Dot { cog, ring, a }) = self.sel {
            marks.dot = Some((cog, ring, a));
        }
        let windows = |arm: u32, pad: u32| w.s.windows != 0.0 || self.is_sel(Sel::Arm(arm)) || self.is_sel(Sel::Pad(pad));
        draw_world(&pen, w, &marks, windows);
        let px = pen.px();
        for band in [self.pit_band, self.band].into_iter().flatten() {
            pen.dashed(band.0 .0, band.0 .1, band.1 .0, band.1 .1, 2.0, DIM, 6.0, 5.0, 0.0);
        }
        // the selection's halo
        if let Some(s) = self.sel {
            match s {
                Sel::Cog(i) => {
                    if let Some(ci) = w.cog_ix(i) {
                        let c = &w.cogs[ci];
                        halo(&pen, c.x, c.y, c.radius() + 3.2);
                        if w.clock.paused {
                            zero_tick(&pen, c, w.render_th(ci));
                        }
                    }
                }
                Sel::Pad(i) => {
                    if let Some(p) = w.pads.iter().find(|p| p.id == i) {
                        halo(&pen, p.x, p.y, p.r + 2.4);
                    }
                }
                Sel::Arm(i) => {
                    if let Some(ai) = w.arms.iter().position(|p| p.id == i) {
                        let t = w.arm_tip(ai);
                        halo(&pen, t.0, t.1, 3.3);
                    }
                }
                Sel::Cam(i) => {
                    if let Some(m) = w.cams.iter().find(|p| p.id == i) {
                        let t = cam_tip(m, &w.cogs[m.cog_ix], w.render_th(m.cog_ix));
                        halo(&pen, t.0, t.1, 3.3);
                    }
                }
                Sel::Probe(i) => {
                    if let Some(pr) = w.probes.iter().find(|p| p.id == i) {
                        if w.probe_alive(pr) {
                            let t = w.probe_point(pr);
                            halo(&pen, t.0, t.1, 3.6);
                        }
                    }
                }
                Sel::Reader(i) => {
                    if let Some(r) = w.readers.iter().find(|p| p.id == i) {
                        halo(&pen, r.x1, r.y1, 3.8);
                        halo(&pen, r.x2, r.y2, 3.8);
                    }
                }
                Sel::String(i) => {
                    if let Some(r) = w.strings.iter().find(|p| p.id == i) {
                        halo(&pen, r.x1, r.y1, 3.8);
                        halo(&pen, r.x2, r.y2, 3.8);
                    }
                }
                Sel::Pit(i) => {
                    if let Some(p) = w.pits.iter().find(|p| p.id == i) {
                        draw_pit_sel(&pen, p.x, p.y, p.w);
                    }
                }
                Sel::Field(i) => {
                    if let Some(f) = w.fields.iter().find(|p| p.id == i) {
                        draw_field_sel(&pen, f);
                    }
                }
                _ => {}
            }
        }
        if w.s.ids != 0.0 {
            let t = |x: f64, y: f64, s: String| pen.text(x, y, &s, 10.0, DIM, egui::Align2::LEFT_BOTTOM);
            for c in &w.cogs {
                t(c.x + 4.0 * px, c.y - 4.0 * px, format!("c{}", c.id));
            }
            for p in &w.pads {
                t(p.x + p.r + 2.0 * px, p.y, format!("p{}", p.id));
            }
            for r in &w.readers {
                t(r.x2 + 2.0 * px, r.y2, format!("r{}", r.id));
            }
            for r in &w.strings {
                t(r.x2 + 2.0 * px, r.y2, format!("s{}", r.id));
            }
            for (i, a) in w.arms.iter().enumerate() {
                let p = w.arm_tip(i);
                t(p.0, p.1, format!("a{}", a.id));
            }
        }
        self.draw_group(painter, w);
        if let Some(c) = self.crank {
            self.draw_crank(painter, w, c);
        }
        self.draw_knob(painter, w);
        self.draw_chrome(painter, w);
    }

    fn draw_group(&self, p: &egui::Painter, w: &World) {
        let st = egui::Stroke::new(1.0, SEL);
        if let Some((a, b)) = self.marquee {
            let (a, b) = (self.to_screen(w, a.0, a.1), self.to_screen(w, b.0, b.1));
            let r = Rect::from_two_pos(self.scr(a), self.scr(b));
            p.rect_filled(r, 0.0, alpha(SEL, 0.08));
            let c = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
            p.extend(egui::Shape::dashed_line(&c, st, 5.0, 4.0));
        }
        for g in &self.group {
            let ring = |x: f64, y: f64, r: f64| {
                p.circle_stroke(self.scr(self.to_screen(w, x, y)), r as f32, st);
            };
            match *g {
                Sel::Cog(i) => {
                    if let Some(c) = w.cog(i) {
                        ring(c.x, c.y, c.radius() * w.s.zoom + 5.0)
                    }
                }
                Sel::Pad(i) => {
                    if let Some(c) = w.pads.iter().find(|o| o.id == i) {
                        ring(c.x, c.y, c.r * w.s.zoom + 5.0)
                    }
                }
                Sel::Pit(i) => {
                    if let Some(c) = w.pits.iter().find(|o| o.id == i) {
                        ring(c.x, c.y, c.w / 2.0 * w.s.zoom + 5.0)
                    }
                }
                Sel::Field(i) => {
                    if let Some(c) = w.fields.iter().find(|o| o.id == i) {
                        ring(c.x, c.y, 12.0)
                    }
                }
                Sel::Arm(i) => {
                    if let Some(ai) = w.arms.iter().position(|o| o.id == i) {
                        let t = w.arm_tip(ai);
                        ring(t.0, t.1, 7.0)
                    }
                }
                Sel::Cam(i) => {
                    if let Some(m) = w.cams.iter().find(|o| o.id == i) {
                        let t = cam_tip(m, &w.cogs[m.cog_ix], w.render_th(m.cog_ix));
                        ring(t.0, t.1, 7.0)
                    }
                }
                Sel::Probe(i) => {
                    if let Some(pr) = w.probes.iter().find(|o| o.id == i) {
                        if w.probe_alive(pr) {
                            let t = w.probe_point(pr);
                            ring(t.0, t.1, 8.0)
                        }
                    }
                }
                Sel::Reader(i) | Sel::String(i) => {
                    let seg = if let Sel::Reader(_) = g {
                        w.readers.iter().find(|r| r.id == i).map(|r| (r.x1, r.y1, r.x2, r.y2))
                    } else {
                        w.strings.iter().find(|r| r.id == i).map(|r| (r.x1, r.y1, r.x2, r.y2))
                    };
                    if let Some((x1, y1, x2, y2)) = seg {
                        ring(x1, y1, 6.0);
                        ring(x2, y2, 6.0);
                        p.line_segment([self.scr(self.to_screen(w, x1, y1)), self.scr(self.to_screen(w, x2, y2))], st);
                    }
                }
                _ => {}
            }
        }
    }

    fn draw_knob(&self, p: &egui::Painter, w: &World) {
        let Some(c) = self.knob_cog(w) else { return };
        let a = self.knob.map(|k| k.1).unwrap_or(-std::f64::consts::FRAC_PI_2);
        let (k, s) = self.knob_at(w, c, a);
        let live = self.knob.is_some();
        let ink = if live { HOT } else { SEL };
        p.line_segment([self.scr(s), self.scr(k)], egui::Stroke::new(1.0, alpha(ink, 0.6)));
        p.circle_filled(self.scr(k), KNOB_R as f32, BG);
        p.circle_stroke(self.scr(k), KNOB_R as f32, egui::Stroke::new(2.0, ink));
        p.circle_filled(self.scr(k), 3.5, ink);
    }

    fn draw_crank(&self, p: &egui::Painter, w: &World, c: u32) {
        let Some(cg) = w.cog(c) else { return };
        let s = self.to_screen(w, cg.x, cg.y);
        let r = cg.radius() * w.s.zoom + 22.0;
        let a0 = -std::f64::consts::FRAC_PI_2;
        let at = |a: f64, r: f64| self.scr((s.0 + a.cos() * r, s.1 + a.sin() * r));
        for d in detents(w) {
            for sg in [-1.0, 1.0] {
                let a = a0 + gauge_angle(sg * d);
                p.line_segment([at(a, r - 4.0), at(a, r + 4.0)], egui::Stroke::new(1.0, DIM));
            }
        }
        let a1 = a0 + gauge_angle(w.motor_rpm(cg));
        let (lo, hi) = (a0.min(a1), a0.max(a1));
        let n = (((hi - lo) * r / 3.0).ceil() as usize).max(2);
        let pts: Vec<Pos2> = (0..=n).map(|i| at(lo + (hi - lo) * i as f64 / n as f64, r)).collect();
        p.add(egui::Shape::line(pts, egui::Stroke::new(2.0, HOT)));
        p.circle_filled(at(a1, r), 3.5, HOT);
    }
}

// ---------- helpers ----------
fn s_not_dragged(s: &sheet::Sheet) -> bool {
    !s.dragging()
}
fn same_view(w: &World, at: (f64, f64, f64)) -> bool {
    (w.view.x - at.0).abs() * w.s.zoom < 0.5 && (w.view.y - at.1).abs() * w.s.zoom < 0.5 && (w.s.zoom / at.2 - 1.0).abs() < 1e-3
}

pub(crate) fn exists(w: &World, s: Sel) -> bool {
    match s {
        Sel::Cog(i) => w.cogs.iter().any(|o| o.id == i),
        Sel::Arm(i) => w.arms.iter().any(|o| o.id == i),
        Sel::Pad(i) => w.pads.iter().any(|o| o.id == i),
        Sel::Reader(i) => w.readers.iter().any(|o| o.id == i),
        Sel::String(i) => w.strings.iter().any(|o| o.id == i),
        Sel::Belt(i) => w.belts.iter().any(|o| o.id == i),
        Sel::Cam(i) => w.cams.iter().any(|o| o.id == i),
        Sel::Pit(i) => w.pits.iter().any(|o| o.id == i),
        Sel::Probe(i) => w.probes.iter().any(|o| o.id == i),
        Sel::Field(i) => w.fields.iter().any(|o| o.id == i),
        Sel::Dot { cog, ring, a } => w.cog(cog).and_then(|c| c.track.as_ref()).map_or(false, |t| {
            t.dots.iter().any(|d| d.ring == ring && wrap_pi(d.a - a).abs() < 1e-6)
        }),
    }
}

/// The machine's extent: centre and size.
pub(crate) fn bounds(w: &World) -> Option<(f64, f64, f64, f64)> {
    let (mut x0, mut x1, mut y0, mut y1) = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
    let mut add = |x: f64, y: f64, r: f64| {
        x0 = x0.min(x - r);
        x1 = x1.max(x + r);
        y0 = y0.min(y - r);
        y1 = y1.max(y + r);
    };
    for c in &w.cogs {
        add(c.x, c.y, c.radius() + 4.0);
    }
    for p in &w.pads {
        add(p.x, p.y, p.r + 3.0);
    }
    for r in &w.readers {
        add(r.x1, r.y1, 4.0);
        add(r.x2, r.y2, 4.0);
    }
    for r in &w.strings {
        add(r.x1, r.y1, 4.0);
        add(r.x2, r.y2, 4.0);
    }
    // an arm's whole sweep, so a turning arm stays in the frame
    for a in &w.arms {
        let c = &w.cogs[a.cog_ix];
        add(c.x, c.y, a.r0 + a.len + 4.0);
    }
    for p in &w.pits {
        add(p.x, p.y, p.w / 2.0 + 4.0);
    }
    for f in &w.fields {
        add(f.x, f.y, f.r * 0.7);
    }
    if !x0.is_finite() {
        return None;
    }
    Some(((x0 + x1) / 2.0, (y0 + y1) / 2.0, x1 - x0, y1 - y0))
}

const RPM_DETENTS: [f64; 20] = [5.0, 7.5, 10.0, 12.0, 15.0, 20.0, 24.0, 30.0, 40.0, 45.0, 60.0, 72.0, 80.0, 90.0, 120.0, 144.0, 160.0, 180.0, 240.0, 300.0];
const BAR_DETENTS: [f64; 15] = [1.0 / 8.0, 1.0 / 6.0, 1.0 / 4.0, 1.0 / 3.0, 1.0 / 2.0, 2.0 / 3.0, 3.0 / 4.0, 1.0, 4.0 / 3.0, 3.0 / 2.0, 2.0, 3.0, 4.0, 6.0, 8.0];
pub(crate) fn detents(w: &World) -> Vec<f64> {
    if w.s.bpm != 0.0 { BAR_DETENTS.iter().map(|r| r * w.s.bpm / w.s.beats).collect() } else { RPM_DETENTS.to_vec() }
}
pub(crate) const GAUGE_SWEEP: f64 = 1.7 * std::f64::consts::PI;
pub(crate) fn gauge_angle(rpm: f64) -> f64 {
    rpm.signum() * ((rpm.abs().max(1.0) / 2.5).ln() / 120f64.ln()).clamp(0.0, 1.0) * GAUGE_SWEEP * if rpm == 0.0 { 0.0 } else { 1.0 }
}
pub(crate) fn gauge_rpm(a: f64) -> f64 {
    let k = a.abs() / GAUGE_SWEEP;
    if k < 0.04 { 0.0 } else { a.signum() * 2.5 * 120f64.powf(k) }
}
/// A released dial keeps its speed; it settles on a step only when let go within 6% of one.
pub(crate) fn detent(w: &World, rpm: f64) -> f64 {
    let a = rpm.abs();
    if a < 3.0 {
        return 0.0;
    }
    let ds = detents(w);
    let d = ds.iter().copied().fold(ds[0], |b, d| if (d / a).ln().abs() < (b / a).ln().abs() { d } else { b });
    if (d / a).ln().abs() < 1.06f64.ln() { rpm.signum() * d } else { rpm }
}
fn set_rpm(w: &World, c: usize) -> f64 {
    let cg = &w.cogs[c];
    match cg.driver {
        Some(m) if !cg.jam => w.motor_rpm(&w.cogs[m]) * cg.k.unwrap_or(1.0) / w.cogs[m].k.unwrap_or(1.0),
        _ => 0.0,
    }
}
/// The drag's speed over the last 80 ms of pointer samples; once the pointer stops sending it fades.
fn drag_rpm(p: &Spin, t: f64) -> f64 {
    const WIN: f64 = 0.08;
    let q = &p.samples;
    let (te, ae) = *q.last().unwrap();
    let from = (te - WIN).max(q[0].0);
    if te - from < 0.012 {
        return if t - te < WIN { p.rpm } else { 0.0 };
    }
    let mut at = q[0].1;
    for i in (1..q.len()).rev() {
        if q[i - 1].0 <= from {
            let ((t0, a0), (t1, a1)) = (q[i - 1], q[i]);
            let d = t1 - t0;
            at = a0 + (a1 - a0) * ((from - t0) / if d == 0.0 { 1.0 } else { d }).clamp(0.0, 1.0);
            break;
        }
    }
    (ae - at) / (te - from) * 60.0 / TAU * (2.0 - (t - te) / (WIN / 2.0)).clamp(0.0, 1.0)
}
pub(crate) fn seg_dist(p: (f64, f64), x1: f64, y1: f64, x2: f64, y2: f64) -> f64 {
    cogs_sim::sim::seg_dist_pt(p.0, p.1, x1, y1, x2, y2)
}
pub(crate) fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}
#[allow(dead_code)]
pub(crate) fn rgb(c: Color32) -> Color32 {
    c
}
