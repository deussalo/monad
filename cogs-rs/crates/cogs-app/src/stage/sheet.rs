//! The sheets (phone.js #dev): a bottom sheet over the stage. The notes editor keeps its tools at the bottom, under the
//! thumb: the roll on top, the disc's tuning under it, and the head row (snap, velocity, duplicate, copy, paste,
//! delete, undo, redo, close) over the readout strip. The panel (stage.js buildPanel) holds transport, the selection's
//! rows, the room, the master's line amp, files and the advanced drawer. Drag the grab down to tuck a sheet to its
//! strip (the machine above keeps taking touches), up or tap to bring it back, down again to close; a tap on the dim
//! stage above an open sheet closes it.

use super::bezel::{self, get, set, Key, Val, VERB_COLORS, VERB_MODES};
use super::draw::*;
use super::pen::*;
use super::roll::{Geo, Roll, SNAPS};
use super::*;
use cogs_sim::{Part, VoiceKind, SCALES};
use egui::{vec2, Align2, FontId, RichText, Stroke};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SheetKind {
    Notes,
    Panel,
}

pub struct Sheet {
    pub kind: SheetKind,
    pub min: bool,
    /// The grab being dragged: start y and how far.
    drag: Option<(f64, f64)>,
}

/// Who a finger in a sheet belongs to.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Owner {
    Roll,
    Grab,
    Widget,
    Scrim,
}

/// The pen's brush and the Euclid fill's settings (stage.js brush, eu).
pub struct Brush {
    pub p: f64,
    pub v: f64,
    pub eu_ring: f64,
    pub eu_k: f64,
    pub eu_rot: f64,
}
impl Default for Brush {
    fn default() -> Self {
        Brush { p: 1.0, v: 1.0, eu_ring: 0.0, eu_k: 3.0, eu_rot: 0.0 }
    }
}

const GRAB_H: f64 = 28.0;
/// The notes sheet's two rows of tools.
const HEAD_H: f64 = 96.0;
const CHIPS_H: f64 = 40.0;

/// The notes sheet's popovers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pop {
    Key,
    Octave,
    Range,
    Steps,
    Revs,
    Snap,
    Brush,
    Euclid,
    More,
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Tool {
    Lane,
    Brush,
    Euclid,
    ShiftL,
    ShiftR,
    Rec,
    More,
    Dup,
    Copy,
    Paste,
    Delete,
    Undo,
    Redo,
    Close,
}
fn nearest_rev(revs: f64) -> &'static str {
    REVS.iter().copied().fold(REVS[0], |a, b| if (frac(b) - revs).abs() < (frac(a) - revs).abs() { b } else { a })
}
/// "1/2" as "½", "-1/3" as "−⅓", whole numbers as they are.
fn pretty_frac(s: &str) -> String {
    let neg = s.starts_with('-');
    let body = s.trim_start_matches('-');
    let f = match body {
        "1/2" => "½",
        "1/3" => "⅓",
        "2/3" => "⅔",
        "1/4" => "¼",
        "3/4" => "¾",
        "1/6" => "⅙",
        "1/8" => "⅛",
        "4/3" => "1⅓",
        "3/2" => "1½",
        b => b,
    };
    format!("{}{}", if neg { "−" } else { "" }, f)
}
const REVS: [&str; 21] = ["-2", "-1", "-1/2", "-1/3", "-1/4", "0", "1/8", "1/6", "1/4", "1/3", "1/2", "2/3", "3/4", "1", "4/3", "3/2", "2", "3", "4", "6", "8"];
fn frac(s: &str) -> f64 {
    let mut it = s.split('/');
    let a: f64 = it.next().unwrap().parse().unwrap();
    let b: f64 = it.next().map(|b| b.parse().unwrap()).unwrap_or(1.0);
    a / b
}

impl Stage {
    // ---------- opening and closing ----------
    pub fn open_notes(&mut self, w: &World) {
        let Some(Sel::Cog(c)) = self.sel else { return };
        if w.cog(c).map_or(true, |c| c.track.is_none()) {
            return;
        }
        match &mut self.roll {
            Some(r) => r.retarget(c),
            None => self.roll = Some(Roll::new(c)),
        }
        self.sheet = Some(Sheet { kind: SheetKind::Notes, min: false, drag: None });
    }
    pub fn toggle_panel(&mut self) {
        self.sheet = match &self.sheet {
            Some(s) if s.kind == SheetKind::Panel => None,
            _ => Some(Sheet { kind: SheetKind::Panel, min: false, drag: None }),
        };
    }
    pub fn close_sheet(&mut self) {
        self.sheet = None;
    }
    pub fn sheet_open(&self) -> bool {
        self.sheet.as_ref().map_or(false, |s| !s.min)
    }

    // ---------- layout ----------
    /// The sheet's top, as it stands now (tucked: just its strip shows).
    pub(super) fn sheet_top(&self) -> Option<f64> {
        let s = self.sheet.as_ref()?;
        let full = match s.kind {
            SheetKind::Notes => (0.14 * self.ch).max(self.safe_top),
            SheetKind::Panel => (0.28 * self.ch).max(self.safe_top + 120.0),
        };
        let strip = self.ch - self.strip_h(s.kind);
        let base = if s.min { strip } else { full };
        let dy = s.drag.map(|d| d.1).unwrap_or(0.0);
        Some((base + dy).clamp(full, self.ch - 20.0))
    }
    fn strip_h(&self, k: SheetKind) -> f64 {
        match k {
            SheetKind::Notes => 1.0 + GRAB_H + HEAD_H + INTERACTION_H as f64 + self.safe_bot,
            SheetKind::Panel => GRAB_H + INTERACTION_H as f64 + self.safe_bot,
        }
    }
    /// The grab's band, in stage coordinates.
    fn grab_band(&self) -> Option<(f64, f64)> {
        let s = self.sheet.as_ref()?;
        let top = self.sheet_top()?;
        Some(match s.kind {
            SheetKind::Panel => (top, top + GRAB_H),
            SheetKind::Notes => {
                let y = if s.min { top } else { self.ch - self.strip_h(SheetKind::Notes) };
                (y, y + GRAB_H)
            }
        })
    }
    /// The roll's box, in stage coordinates.
    fn roll_box(&self, w: &World) -> Option<Rect> {
        let s = self.sheet.as_ref()?;
        if s.kind != SheetKind::Notes || s.min {
            return None;
        }
        let top = self.sheet_top()? + 8.0;
        let r = self.roll.as_ref()?;
        let g = r.geo(w, self.cw - 16.0, self.notes_layout().0)?;
        Some(Rect::from_min_size(pos2(8.0, top as f32), g.rect.size()))
    }
    fn roll_geo(&self, w: &World) -> Option<Geo> {
        let b = self.roll_box(w)?;
        let mut g = self.roll.as_ref()?.geo(w, self.cw - 16.0, self.notes_layout().0)?;
        g.rect = b;
        Some(g)
    }
    /// Which part of the sheet a press at s lands on; None: not the sheet's.
    pub(super) fn sheet_hit(&self, w: &World, s: (f64, f64)) -> Option<Owner> {
        let sh = self.sheet.as_ref()?;
        let top = self.sheet_top()?;
        if let Some(r) = self.pop_rect {
            if r.translate(-self.rect.min.to_vec2()).contains(pos2(s.0 as f32, s.1 as f32)) {
                return Some(Owner::Widget);
            }
        }
        if s.1 < top {
            return if sh.min { None } else { Some(Owner::Scrim) };
        }
        if let Some((a, b)) = self.grab_band() {
            if s.1 >= a && s.1 < b {
                return Some(Owner::Grab);
            }
        }
        if let Some(r) = self.roll_box(w) {
            if r.contains(pos2(s.0 as f32, s.1 as f32)) {
                return Some(Owner::Roll);
            }
        }
        Some(Owner::Widget)
    }

    // ---------- input routed to the sheet ----------
    pub(super) fn sheet_down(&mut self, w: &mut World, id: u64, s: (f64, f64), who: Owner) {
        match who {
            Owner::Roll => {
                let Some(g) = self.roll_geo(w) else { return };
                let b = (s.0 - g.rect.min.x as f64, s.1 - g.rect.min.y as f64);
                let now = self.now;
                let mut out = std::mem::take(&mut self.out);
                if let Some(r) = &mut self.roll {
                    r.touch_down(w, &mut out, &g, id, b, now);
                }
                self.out = out;
            }
            Owner::Grab => {
                if let Some(sh) = &mut self.sheet {
                    sh.drag = Some((s.1, 0.0));
                }
            }
            _ => {}
        }
    }
    pub(super) fn sheet_move(&mut self, w: &mut World, id: u64, s: (f64, f64), who: Owner) {
        match who {
            Owner::Roll => {
                let Some(g) = self.roll_geo(w) else { return };
                let b = (s.0 - g.rect.min.x as f64, s.1 - g.rect.min.y as f64);
                let now = self.now;
                let mut out = std::mem::take(&mut self.out);
                if let Some(r) = &mut self.roll {
                    r.touch_move(w, &mut out, &g, id, b, now);
                    if let Some(st) = &r.status {
                        self.readout = Some((st.clone(), f64::INFINITY));
                    }
                }
                self.out = out;
            }
            Owner::Grab => {
                if let Some(sh) = &mut self.sheet {
                    if let Some(d) = &mut sh.drag {
                        d.1 = s.1 - d.0;
                    }
                }
            }
            _ => {}
        }
    }
    pub(super) fn sheet_up(&mut self, w: &mut World, id: u64, cancel: bool, who: Owner) {
        match who {
            Owner::Roll => {
                let Some(g) = self.roll_geo(w) else { return };
                let now = self.now;
                let mut out = std::mem::take(&mut self.out);
                let mut changed = false;
                if let Some(r) = &mut self.roll {
                    changed = r.touch_up(w, &mut out, &g, id, cancel, now);
                }
                self.out = out;
                if self.readout.as_ref().map_or(false, |r| r.1.is_infinite()) {
                    self.readout = None;
                }
                if changed {
                    self.changed(w, false);
                    if self.edits.commit(w) {
                        self.out.push(StageOut::Changed);
                    }
                }
            }
            Owner::Grab => {
                let Some(sh) = &mut self.sheet else { return };
                let dy = sh.drag.take().map(|d| d.1).unwrap_or(0.0);
                if cancel {
                    return;
                }
                if dy.abs() <= 6.0 {
                    sh.min = !sh.min;
                } else if sh.min {
                    if dy < -30.0 {
                        sh.min = false;
                    } else if dy > 40.0 {
                        self.sheet = None;
                    }
                } else {
                    let h = self.ch - self.sheet_top().unwrap_or(0.0);
                    if let Some(sh) = &mut self.sheet {
                        sh.min = dy > h * 0.3 || dy > 120.0;
                    }
                }
                self.buzz(6);
            }
            Owner::Scrim => {
                if !cancel {
                    self.sheet = None;
                }
            }
            Owner::Widget => {}
        }
    }

    // ---------- drawing the sheet and its widgets ----------
    pub(super) fn show_sheet(&mut self, ui: &mut egui::Ui, w: &mut World, extra: &mut dyn FnMut(&mut egui::Ui, &mut World)) {
        let Some(kind) = self.sheet.as_ref().map(|s| s.kind) else { return };
        if kind == SheetKind::Notes {
            match self.sel {
                Some(Sel::Cog(c)) if w.cog(c).map_or(false, |c| c.track.is_some()) => {
                    if let Some(r) = &mut self.roll {
                        r.retarget(c);
                    }
                }
                _ => {
                    self.sheet = None;
                    return;
                }
            }
        }
        if let Some(r) = &mut self.roll {
            let now = self.now;
            let mut out = std::mem::take(&mut self.out);
            r.tick(now, &mut out);
            if let Some(st) = &r.status {
                self.readout = Some((st.clone(), f64::INFINITY));
            }
            self.out = out;
        }
        let top = self.sheet_top().unwrap();
        let min = self.sheet.as_ref().unwrap().min;
        let o = self.rect.min;
        let p = ui.painter().clone();
        if !min {
            p.rect_filled(Rect::from_min_max(o, self.scr((self.cw, top))), 0.0, Color32::from_black_alpha(90));
        }
        let sheet = Rect::from_min_max(self.scr((0.0, top)), self.scr((self.cw, self.ch)));
        let round = if kind == SheetKind::Notes { 12 } else { 0 };
        p.rect_filled(sheet, egui::CornerRadius { nw: round, ne: round, sw: 0, se: 0 }, BG);
        p.line_segment([sheet.left_top(), sheet.right_top()], Stroke::new(1.0, LINE));
        style(ui);
        match kind {
            SheetKind::Notes => self.show_notes(ui, w, top, min),
            SheetKind::Panel => self.show_panel(ui, w, top, min, extra),
        }
        // the grab
        if let Some((a, _)) = self.grab_band() {
            let c = self.scr((self.cw / 2.0, a + GRAB_H / 2.0));
            p.rect_filled(Rect::from_center_size(c, vec2(40.0, 4.0)), 2.0, if min { HOT } else { DIM });
        }
    }

    /// The notes sheet's layout: the grid's box height, the chip row's top and the toolbar's top (stage coordinates).
    fn notes_layout(&self) -> (f64, f64, f64) {
        let top = self.sheet_top().unwrap_or(0.0);
        let strip = self.ch - self.strip_h(SheetKind::Notes);
        let chips = strip - CHIPS_H - 4.0;
        let grid_h = (chips - 6.0 - (top + 8.0)).max(80.0);
        (grid_h, chips, strip + GRAB_H)
    }

    fn show_notes(&mut self, ui: &mut egui::Ui, w: &mut World, top: f64, min: bool) {
        let strip = self.ch - self.strip_h(SheetKind::Notes);
        let bar_y = if min { top + GRAB_H } else { strip + GRAB_H };
        let mut changed = false;
        if !min {
            if let (Some(g), Some(r)) = (self.roll_geo(w), &self.roll) {
                let mut g2 = g;
                g2.rect = g.rect.translate(self.rect.min.to_vec2());
                r.draw(ui.painter(), w, &g2, self.now);
                ui.painter().rect_stroke(g2.rect, 6.0, Stroke::new(1.0, LINE), egui::StrokeKind::Outside);
            }
            let (_, chips_y, _) = self.notes_layout();
            changed |= self.notes_chips(ui, w, chips_y);
            ui.painter().line_segment([self.scr((0.0, strip)), self.scr((self.cw, strip))], Stroke::new(1.0, LINE));
        }
        changed |= self.notes_toolbar(ui, w, bar_y);
        changed |= self.notes_popover(ui, w);
        let ghost = if self.pop.map(|p| p.0) == Some(Pop::Euclid) { Some((self.brush.eu_ring, self.brush.eu_k as i64, self.brush.eu_rot as i64)) } else { None };
        if let Some(r) = &mut self.roll {
            r.ghost = ghost;
        }
        if changed {
            self.changed(w, true);
        }
    }

    /// One row of chips under the grid: the disc's tuning, each opening its own small popover.
    fn notes_chips(&mut self, ui: &mut egui::Ui, w: &World, y: f64) -> bool {
        let Some(Sel::Cog(id)) = self.sel else { return false };
        let Some(ci) = w.cog_ix(id) else { return false };
        let c = &w.cogs[ci];
        let Some(t) = c.track.as_ref() else { return false };
        let mut chips: Vec<(Pop, String)> = vec![
            (Pop::Key, format!("{} {}", NOTE_NAMES[(t.root as usize) % 12], SCALES[t.scale].0)),
            (Pop::Octave, format!("oct {}", t.octave)),
            (Pop::Range, format!("×{}", t.range)),
            (Pop::Steps, format!("{} steps", t.div)),
        ];
        let train = c.driver.is_some() || c.motor.is_some();
        if w.s.bpm != 0.0 && train {
            let revs = w.rpm(ci) * w.s.beats / w.s.bpm;
            chips.push((Pop::Revs, format!("{} rev", pretty_frac(nearest_rev(revs)))));
        }
        let font = FontId::proportional(14.0);
        let mut x = 8.0f32;
        for (pop, text) in chips {
            let tw = ui.painter().layout_no_wrap(text.clone(), font.clone(), FG).size().x;
            let r = Rect::from_min_size(self.scr((x as f64, y)), vec2(tw + 22.0, CHIPS_H as f32));
            x += tw + 22.0 + 6.0;
            let open = self.pop.map(|p| p.0) == Some(pop);
            let resp = ui.interact(r, ui.scope_id().with(("chip", pop as u8)), egui::Sense::click());
            ui.painter().rect_filled(r, 20.0, if open { LINE } else { PAD });
            if open {
                ui.painter().rect_stroke(r, 20.0, Stroke::new(1.0, HOT), egui::StrokeKind::Inside);
            }
            ui.painter().text(r.center(), Align2::CENTER_CENTER, text, font.clone(), FG);
            if resp.clicked() {
                self.toggle_pop(pop, r);
            }
        }
        false
    }

    /// The two rows of tools under the thumb: the roll's tools, then edits and the sheet's own.
    fn notes_toolbar(&mut self, ui: &mut egui::Ui, w: &mut World, y: f64) -> bool {
        let mut changed = false;
        let left = 8.0;
        let width = self.cw - 16.0;
        let has = self.roll.as_ref().map_or(false, |r| r.has_sel());
        let clip = self.roll.as_ref().map_or(false, |r| !r.clip.is_empty());
        let vel = self.roll.as_ref().map_or(false, |r| r.vel);
        let snap = self.roll.as_ref().map_or(5, |r| r.snap);
        // row one: snap · lane · brush · euclid · shift ◀ ▶ · record · more
        let n = 7.0;
        let snap_w = 56.0;
        let bw = ((width - snap_w) / n).clamp(40.0, 56.0);
        let o = self.rect.min;
        let at = |i: f64, row: f64| Rect::from_min_size(pos2(o.x + (left + snap_w + i * bw) as f32, o.y + (y + row * 48.0) as f32), vec2(bw as f32, 44.0));
        let sr = Rect::from_min_size(self.scr((left, y)), vec2(snap_w as f32, 44.0));
        let resp = ui.interact(sr, ui.scope_id().with("snap"), egui::Sense::click());
        let open = self.pop.map(|p| p.0) == Some(Pop::Snap);
        if open {
            ui.painter().rect_filled(sr.shrink(2.0), 8.0, LINE);
        }
        ui.painter().text(sr.center() - vec2(0.0, 6.0), Align2::CENTER_CENTER, SNAPS[snap].3, FontId::proportional(15.0), HOT);
        ui.painter().text(sr.center() + vec2(0.0, 11.0), Align2::CENTER_CENTER, "snap", FontId::proportional(10.0), DIM);
        if resp.clicked() {
            self.toggle_pop(Pop::Snap, sr);
        }
        let tools: [(Tool, bool); 7] = [(Tool::Lane, vel), (Tool::Brush, self.pop.map(|p| p.0) == Some(Pop::Brush)), (Tool::Euclid, self.pop.map(|p| p.0) == Some(Pop::Euclid)),
            (Tool::ShiftL, false), (Tool::ShiftR, false), (Tool::Rec, self.io.recording), (Tool::More, self.pop.map(|p| p.0) == Some(Pop::More))];
        for (i, (tool, on)) in tools.iter().enumerate() {
            let r = at(i as f64, 0.0);
            let resp = ui.interact(r, ui.scope_id().with(("tool", i)), egui::Sense::click());
            if *on {
                ui.painter().rect_filled(r.shrink(3.0), 8.0, LINE);
            }
            self.paint_tool(ui.painter(), *tool, r.translate(vec2(0.0, -6.0)), *on, true);
            let cap = match tool {
                Tool::Lane => "lane",
                Tool::Brush => "brush",
                Tool::Euclid => "euclid",
                Tool::ShiftL => "earlier",
                Tool::ShiftR => "later",
                Tool::Rec => "record",
                _ => "more",
            };
            ui.painter().text(r.center() + vec2(0.0, 13.0), Align2::CENTER_CENTER, cap, FontId::proportional(10.0), if *on { HOT } else { DIM });
            if resp.clicked() {
                self.buzz(6);
                match tool {
                    Tool::Lane => {
                        if let Some(rl) = &mut self.roll {
                            rl.vel = !rl.vel;
                        }
                    }
                    Tool::Brush => self.toggle_pop(Pop::Brush, r),
                    Tool::Euclid => self.toggle_pop(Pop::Euclid, r),
                    Tool::More => self.toggle_pop(Pop::More, r),
                    Tool::ShiftL | Tool::ShiftR => {
                        if let Some(rl) = &mut self.roll {
                            rl.shift(w, if *tool == Tool::ShiftL { -1.0 } else { 1.0 });
                        }
                        changed = true;
                    }
                    Tool::Rec => self.out.push(StageOut::Record),
                    _ => {}
                }
            }
        }
        // row two: duplicate · copy · paste · delete … undo · redo · close
        let row2: [(Tool, bool); 4] = [(Tool::Dup, has), (Tool::Copy, has), (Tool::Paste, clip), (Tool::Delete, has)];
        let bw2 = (width / 7.0).clamp(40.0, 52.0);
        for (i, (tool, enabled)) in row2.iter().enumerate() {
            let r = Rect::from_min_size(self.scr((left + i as f64 * bw2, y + 48.0)), vec2(bw2 as f32, 44.0));
            let resp = ui.interact(r, ui.scope_id().with(("edit", i)), egui::Sense::click());
            self.paint_tool(ui.painter(), *tool, r, false, *enabled);
            if resp.clicked() && *enabled {
                self.buzz(6);
                if let Some(rl) = &mut self.roll {
                    changed |= match tool {
                        Tool::Dup => rl.duplicate(w),
                        Tool::Copy => {
                            rl.copy_sel(w);
                            false
                        }
                        Tool::Paste => rl.paste(w),
                        _ => rl.delete_sel(w),
                    };
                }
            }
        }
        let right: [(Tool, bool); 3] = [(Tool::Undo, self.edits.can_undo()), (Tool::Redo, self.edits.can_redo()), (Tool::Close, true)];
        for (j, (tool, enabled)) in right.iter().enumerate() {
            let r = Rect::from_min_size(self.scr((left + width - bw2 * (3 - j) as f64, y + 48.0)), vec2(bw2 as f32, 44.0));
            let resp = ui.interact(r, ui.scope_id().with(("right", j)), egui::Sense::click());
            self.paint_tool(ui.painter(), *tool, r, false, *enabled);
            if resp.clicked() && *enabled {
                self.buzz(6);
                match tool {
                    Tool::Undo => self.undo(w),
                    Tool::Redo => self.redo(w),
                    _ => {
                        self.sheet = None;
                        self.pop = None;
                    }
                }
            }
        }
        changed
    }

    fn paint_tool(&mut self, p: &egui::Painter, tool: Tool, r: Rect, on: bool, enabled: bool) {
        let ink = if !enabled { alpha(FAINT, 1.0) } else if on { HOT } else { FG };
        let c = r.center();
        let ir = Rect::from_center_size(c, vec2(20.0, 20.0));
        let stroke = Stroke::new(1.6, ink);
        match tool {
            Tool::Lane => self.icons.paint(p, "Velocity", false, ir, ink),
            Tool::Brush => {
                // a pin, part full: the brush's odds and accent
                p.circle_stroke(c, 8.0, stroke);
                p.add(egui::Shape::convex_polygon(
                    std::iter::once(c).chain((0..=12).map(|k| {
                        let a = -std::f32::consts::FRAC_PI_2 + k as f32 / 12.0 * std::f32::consts::PI * 1.4;
                        c + vec2(a.cos(), a.sin()) * 8.0
                    })).collect(),
                    ink,
                    Stroke::NONE,
                ));
            }
            Tool::Euclid => {
                // pulses spread round a ring
                p.circle_stroke(c, 8.5, Stroke::new(1.0, alpha(ink, 0.6)));
                for k in 0..8 {
                    let a = -std::f32::consts::FRAC_PI_2 + k as f32 / 8.0 * std::f32::consts::TAU;
                    let q = c + vec2(a.cos(), a.sin()) * 8.5;
                    p.circle_filled(q, if k % 3 == 0 { 2.6 } else { 1.2 }, ink);
                }
            }
            Tool::ShiftL | Tool::ShiftR => {
                let s = if tool == Tool::ShiftL { -1.0 } else { 1.0 };
                p.add(egui::Shape::convex_polygon(vec![c + vec2(6.0 * s, 0.0), c + vec2(-4.0 * s, -6.5), c + vec2(-4.0 * s, 6.5)], ink, Stroke::NONE));
                p.line_segment([c + vec2(-7.0 * s, -6.5), c + vec2(-7.0 * s, 6.5)], stroke);
            }
            Tool::Rec => {
                p.circle_filled(c, 7.0, if enabled { BAD } else { ink });
                if on {
                    p.circle_stroke(c, 10.5, Stroke::new(1.5, BAD));
                }
            }
            Tool::More => {
                for k in [-6.0, 0.0, 6.0] {
                    p.circle_filled(c + vec2(k, 0.0), 2.0, ink);
                }
            }
            Tool::Dup => self.icons.paint(p, "CardStackPlus", false, ir, ink),
            Tool::Copy => self.icons.paint(p, "Copy", false, ir, ink),
            Tool::Paste => self.icons.paint(p, "Clipboard", false, ir, ink),
            Tool::Delete => self.icons.paint(p, "Trash", false, ir, ink),
            Tool::Undo => self.icons.paint(p, "Reset", false, ir, ink),
            Tool::Redo => self.icons.paint(p, "Reset", true, ir, ink),
            Tool::Close => self.icons.paint(p, "Cross2", false, ir, ink),
        }
    }

    fn toggle_pop(&mut self, pop: Pop, anchor: Rect) {
        self.pop = if self.pop.map(|p| p.0) == Some(pop) { None } else { Some((pop, anchor)) };
        self.pop_rect = None;
    }

    /// The open popover, next to what opened it. A tap anywhere else closes it.
    fn notes_popover(&mut self, ui: &mut egui::Ui, w: &mut World) -> bool {
        let Some((pop, anchor)) = self.pop else {
            self.pop_rect = None;
            return false;
        };
        let Some(Sel::Cog(id)) = self.sel else { return false };
        let Some(ci) = w.cog_ix(id) else { return false };
        let mut changed = false;
        let mut close = false;
        let width: f32 = match pop {
            Pop::Key => 300.0,
            Pop::Revs => 260.0,
            Pop::Euclid => 300.0,
            Pop::Brush => 280.0,
            _ => 220.0,
        };
        let est_h = self.pop_rect.map(|r| r.height()).unwrap_or(220.0);
        let x = anchor.min.x.min(self.rect.max.x - width - 12.0).max(self.rect.min.x + 6.0);
        let y = anchor.min.y - est_h - 8.0;
        let area = egui::Area::new(ui.scope_id().with("notes-pop")).fixed_pos(pos2(x, y.max(self.rect.min.y + 4.0))).order(egui::Order::Foreground);
        let k = |key| Key::Track(id, key);
        let resp = area.show(ui.ctx(), |ui| {
            style(ui);
            egui::Frame::popup(ui.style()).fill(PAD).corner_radius(12.0).inner_margin(10.0).show(ui, |ui| {
                ui.set_width(width - 20.0);
                match pop {
                    Pop::Key => {
                        title(ui, "root");
                        let cur = get(w, &k("root")).n() as usize;
                        egui::Grid::new("roots").spacing([4.0, 4.0]).show(ui, |ui| {
                            for (i, nn) in NOTE_NAMES.iter().enumerate() {
                                if pick(ui, nn, i == cur, 42.0) {
                                    set(w, &k("root"), Val::N(i as f64));
                                    changed = true;
                                }
                                if i % 6 == 5 {
                                    ui.end_row();
                                }
                            }
                        });
                        title(ui, "scale");
                        let Val::S(cur) = get(w, &k("scale")) else { return };
                        ui.horizontal_wrapped(|ui| {
                            for (sname, _) in SCALES.iter() {
                                if pick(ui, sname, *sname == cur, 0.0) {
                                    set(w, &k("scale"), Val::S((*sname).into()));
                                    changed = true;
                                }
                            }
                        });
                    }
                    Pop::Octave => {
                        title(ui, "octave");
                        if let Some(v) = stepper(ui, get(w, &k("octave")).n(), 1.0, 7.0) {
                            set(w, &k("octave"), Val::N(v));
                            changed = true;
                        }
                    }
                    Pop::Range => {
                        title(ui, "octaves on the disc");
                        let cur = get(w, &k("range")).n();
                        ui.horizontal(|ui| {
                            for r in [1.0, 2.0, 3.0] {
                                if pick(ui, &format!("×{r}"), cur == r, 56.0) {
                                    set(w, &k("range"), Val::N(r));
                                    changed = true;
                                }
                            }
                        });
                    }
                    Pop::Steps => {
                        title(ui, "steps per turn");
                        let cur = get(w, &k("div")).n();
                        if let Some(v) = stepper(ui, cur, 1.0, 96.0) {
                            set(w, &k("div"), Val::N(v));
                            changed = true;
                        }
                        ui.horizontal_wrapped(|ui| {
                            for n in [8.0, 12.0, 16.0, 24.0, 32.0, 48.0] {
                                if pick(ui, &format!("{n}"), cur == n, 42.0) {
                                    set(w, &k("div"), Val::N(n));
                                    changed = true;
                                }
                            }
                        });
                    }
                    Pop::Revs => {
                        title(ui, "turns per bar");
                        let revs = w.rpm(ci) * w.s.beats / w.s.bpm.max(1.0);
                        let cur = nearest_rev(revs);
                        egui::Grid::new("revs").spacing([4.0, 4.0]).show(ui, |ui| {
                            for (i, r) in REVS.iter().enumerate() {
                                if pick(ui, &pretty_frac(r), *r == cur, 52.0) {
                                    let rpm = frac(r) * w.s.bpm / w.s.beats;
                                    w.drive(ci, rpm);
                                    changed = true;
                                }
                                if i % 4 == 3 {
                                    ui.end_row();
                                }
                            }
                        });
                    }
                    Pop::Snap => {
                        title(ui, "snap");
                        let snap = self.roll.as_ref().map_or(5, |r| r.snap);
                        egui::Grid::new("snaps").spacing([4.0, 4.0]).show(ui, |ui| {
                            for (i, s) in SNAPS.iter().enumerate() {
                                if pick(ui, s.3, i == snap, 64.0) {
                                    if let Some(r) = &mut self.roll {
                                        r.snap = i;
                                    }
                                    close = true;
                                }
                                if i % 3 == 2 {
                                    ui.end_row();
                                }
                            }
                        });
                    }
                    Pop::Brush => {
                        title(ui, "brush: new pins, and the selected ones");
                        ui.spacing_mut().slider_width = width - 120.0;
                        let (p0, v0) = (self.brush.p, self.brush.v);
                        ui.horizontal(|ui| {
                            ui.add_sized([70.0, 30.0], egui::Label::new("odds"));
                            ui.add(egui::Slider::new(&mut self.brush.p, 0.0..=1.0).step_by(0.01));
                        });
                        ui.horizontal(|ui| {
                            ui.add_sized([70.0, 30.0], egui::Label::new("velocity"));
                            ui.add(egui::Slider::new(&mut self.brush.v, 0.0..=1.0).step_by(0.01));
                        });
                        if (self.brush.p, self.brush.v) != (p0, v0) {
                            let (bp, bv) = (self.brush.p, self.brush.v);
                            if let (Some(r), Some(t)) = (&self.roll, w.cogs[ci].track.as_mut()) {
                                for &i in &r.sel {
                                    if let Some(d) = t.dots.get_mut(i) {
                                        if bp != p0 { d.p = bp; }
                                        if bv != v0 { d.v = bv; }
                                    }
                                }
                            }
                            changed = true;
                        }
                    }
                    Pop::Euclid => {
                        let t = w.cogs[ci].track.as_ref().unwrap();
                        let (rings, div) = (t.rings() as f64, t.div);
                        title(ui, "spread pulses evenly over a ring");
                        self.brush.eu_ring = self.brush.eu_ring.min(rings - 1.0);
                        self.brush.eu_k = self.brush.eu_k.min(div);
                        let note = note_name(t.ring_note(self.brush.eu_ring as i64) as i64);
                        egui::Grid::new("eu").spacing([8.0, 4.0]).show(ui, |ui| {
                            ui.label("ring");
                            if let Some(v) = stepper_label(ui, self.brush.eu_ring, 0.0, rings - 1.0, &note) {
                                self.brush.eu_ring = v;
                            }
                            ui.end_row();
                            ui.label("pulses");
                            if let Some(v) = stepper(ui, self.brush.eu_k, 0.0, div) {
                                self.brush.eu_k = v;
                            }
                            ui.end_row();
                            ui.label("rotate");
                            if let Some(v) = stepper(ui, self.brush.eu_rot, 0.0, div - 1.0) {
                                self.brush.eu_rot = v;
                            }
                            ui.end_row();
                        });
                        // the preview: one cell per step, the pulses lit
                        let n = div as i64;
                        let (kk, rot) = (self.brush.eu_k as i64, self.brush.eu_rot as i64);
                        let (rr, _) = ui.allocate_exact_size(vec2(width - 20.0, 22.0), egui::Sense::hover());
                        let cell = rr.width() / n.max(1) as f32;
                        for j in 0..n {
                            let on = kk > 0 && ((j + rot) * kk).rem_euclid(n.max(1)) < kk;
                            let cr = Rect::from_min_size(pos2(rr.min.x + j as f32 * cell, rr.min.y), vec2((cell - 1.0).max(1.0), 22.0));
                            ui.painter().rect_filled(cr, 2.0, if on { HOT } else { LINE });
                        }
                        ui.add_space(6.0);
                        if ui.add_sized([width - 20.0, 40.0], egui::Button::new(RichText::new("fill the ring").color(BG)).fill(HOT)).clicked() {
                            let b = (self.brush.eu_ring, self.brush.eu_k as i64, self.brush.eu_rot as i64, self.brush.p, self.brush.v);
                            if let Some(r) = &mut self.roll {
                                r.euclid(w, b.0, b.1, b.2, b.3, b.4);
                            }
                            changed = true;
                        }
                    }
                    Pop::More => {
                        let readers: Vec<u32> = w.readers.iter().filter(|r| r.on == Some(id)).map(|r| r.id).collect();
                        for rid in readers {
                            if ui.add_sized([width - 20.0, 40.0], egui::Button::new(format!("edit reader {rid}'s patch"))).clicked() {
                                self.select(Some(Sel::Reader(rid)));
                                self.sheet = None;
                                self.out.push(StageOut::OpenEditor("patch"));
                                close = true;
                            }
                        }
                        if ui.add_sized([width - 20.0, 40.0], egui::Button::new("clear all pins")).clicked() {
                            if let Some(t) = w.cogs[ci].track.as_mut() {
                                t.dots.clear();
                            }
                            if let Some(r) = &mut self.roll {
                                r.sel.clear();
                            }
                            changed = true;
                            close = true;
                        }
                        if ui.add_sized([width - 20.0, 40.0], egui::Button::new(RichText::new("drop the track").color(BAD))).clicked() {
                            w.cogs[ci].track = None;
                            self.sheet = None;
                            changed = true;
                            close = true;
                        }
                    }
                }
            });
        });
        let r = resp.response.rect;
        self.pop_rect = Some(r);
        // a press outside the popover and its opener closes it
        let pressed = ui.input(|i| if i.pointer.any_pressed() { i.pointer.interact_pos() } else { None });
        if let Some(p) = pressed {
            if !r.contains(p) && !anchor.contains(p) {
                close = true;
            }
        }
        if close {
            self.pop = None;
            self.pop_rect = None;
        }
        changed
    }
    // ---------- the panel ----------
    fn show_panel(&mut self, ui: &mut egui::Ui, w: &mut World, top: f64, min: bool, extra: &mut dyn FnMut(&mut egui::Ui, &mut World)) {
        if min {
            return;
        }
        let area = Rect::from_min_max(self.scr((6.0, top + GRAB_H)), self.scr((self.cw - 6.0, self.ch - INTERACTION_H as f64 - self.safe_bot)));
        let mut cui = ui.new_child(egui::UiBuilder::new().max_rect(area));
        egui::ScrollArea::vertical().id_salt("panel").max_height(area.height()).show(&mut cui, |ui| {
            ui.set_width(area.width() - 4.0);
            self.panel_body(ui, w, extra);
        });
    }
    fn panel_body(&mut self, ui: &mut egui::Ui, w: &mut World, extra: &mut dyn FnMut(&mut egui::Ui, &mut World)) {
        let mut changed = false;
        let mut structural = false;
        ui.style_mut().spacing.slider_width = (ui.available_width() - 190.0).max(100.0);
        legend(ui, "transport");
        ui.horizontal_wrapped(|ui| {
            if ui.button("close").clicked() {
                self.sheet = None;
            }
            if ui.add_enabled(self.edits.can_undo(), egui::Button::new("undo")).clicked() {
                self.undo(w);
            }
            if ui.add_enabled(self.edits.can_redo(), egui::Button::new("redo")).clicked() {
                self.redo(w);
            }
            if ui.button(if w.clock.paused { "play" } else { "pause" }).clicked() {
                let p = !w.clock.paused;
                let from_start = w.set_paused(p);
                self.out.push(StageOut::Paused { paused: p, from_start });
            }
            if ui.button("del").clicked() {
                self.remove_sel(w);
            }
            if ui.button("demo").clicked() {
                let paused = w.clock.paused;
                *w = World::demo();
                w.set_paused(paused);
                self.loaded(w);
                self.fly_fit(w);
                self.out.push(StageOut::Changed);
            }
            if ui.button("clear").clicked() {
                let mut j = w.serialize();
                for k in ["cogs", "arms", "pads", "readers", "belts", "strings", "cams", "pits", "probes", "fields"] {
                    j[k] = serde_json::json!([]);
                }
                if w.reload(&j).is_ok() {
                    self.loaded(w);
                    self.out.push(StageOut::Changed);
                }
            }
        });
        // the app's own rows (worlds, keyboard, audio)
        extra(ui, w);
        legend(ui, &self.sel_title(w));
        let (c, s) = self.inspector(ui, w);
        changed |= c;
        structural |= s;
        legend(ui, "room");
        egui::Grid::new("room").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            changed |= choice_row(ui, "mode", &Key::Space("mode"), w, &VERB_MODES);
            changed |= choice_row(ui, "color", &Key::Space("color"), w, &VERB_COLORS);
            for (k, l, a, b, st) in [("wet", "wet", 0.0, 1.5, 0.001), ("size", "size", 0.0, 1.0, 0.01), ("decay", "decay s", 0.2, 60.0, 0.01),
                ("highCut", "high cut Hz", 1000.0, 20000.0, 10.0), ("shimmer", "shimmer", 0.0, 1.0, 0.01)]
            {
                changed |= key_row(ui, l, &Key::Space(k), w, a, b, st);
            }
            changed |= toggle_row(ui, "freeze", &Key::Space("freeze"), w);
            changed |= key_row(ui, "master", &Key::S("master"), w, 0.0, 1.5, 0.01);
            let s1 = key_row(ui, "bpm lock", &Key::S("bpm"), w, 0.0, 300.0, 1.0);
            let s2 = key_row(ui, "beats/bar", &Key::S("beats"), w, 1.0, 16.0, 1.0);
            structural |= s1 || s2;
        });
        legend(ui, "master · climax");
        egui::Grid::new("climax").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            changed |= toggle_row(ui, "line amp", &Key::Climax("on"), w);
            for (k, l, a, b, st) in [("drive", "drive dB", -6.0, 12.0, 0.1), ("sat", "saturation %", 0.0, 150.0, 1.0)] {
                changed |= key_row(ui, l, &Key::Climax(k), w, a, b, st);
            }
            changed |= toggle_row(ui, "class A", &Key::Climax("classA"), w);
            for (k, l, a, b, st) in [("excite", "excite", 0.0, 10.0, 0.1), ("air", "air", 0.0, 10.0, 0.1), ("airFreq", "air Hz", 5000.0, 16000.0, 100.0),
                ("low", "low boost", 0.0, 10.0, 0.1), ("lowFreq", "low Hz", 40.0, 500.0, 1.0), ("high", "high boost", 0.0, 10.0, 0.1),
                ("highFreq", "high Hz", 150.0, 5000.0, 10.0), ("mix", "mix", 0.0, 1.0, 0.01), ("out", "out dB", -24.0, 6.0, 0.1)]
            {
                changed |= key_row(ui, l, &Key::Climax(k), w, a, b, st);
            }
        });
        legend(ui, "file");
        ui.horizontal_wrapped(|ui| {
            if ui.button("save file").clicked() {
                self.out.push(StageOut::SaveFile { name: cogs_file_name(), text: cogs_file(w) });
            }
            if ui.button("open file").clicked() {
                self.out.push(StageOut::OpenFile);
            }
        });
        if let Some(m) = &self.file_msg {
            ui.label(RichText::new(m).color(DIM));
        }
        egui::CollapsingHeader::new(RichText::new("advanced").color(DIM)).id_salt("advanced").show(ui, |ui| {
            legend(ui, "world");
            egui::Grid::new("world-s").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                let s = &mut w.s;
                for (l, v, a, b, st, solve) in [("seed", &mut s.seed, 0.0, 9999.0, 1.0, false), ("mesh tol", &mut s.mesh_tol, 0.0, 20.0, 0.5, true),
                    ("coax tol", &mut s.coax_tol, 0.0, 20.0, 0.5, true), ("snap tol", &mut s.snap_tol, 0.0, 60.0, 1.0, false), ("v ref px/s", &mut s.v_ref, 20.0, 3000.0, 1.0, false),
                    ("vel curve", &mut s.vel_curve, 0.1, 3.0, 0.01, false), ("lookahead s", &mut s.lookahead, 0.12, 0.5, 0.005, false), ("step rad", &mut s.max_step, 0.005, 0.3, 0.005, false),
                    ("zoom", &mut s.zoom, 0.1, 8.0, 0.01, false)]
                {
                    ui.label(l);
                    if ui.add(egui::Slider::new(v, a..=b).step_by(st)).changed() {
                        changed = true;
                        structural |= solve;
                    }
                    ui.end_row();
                }
                for (l, v) in [("windows", &mut s.windows), ("ids", &mut s.ids), ("ratio snap", &mut s.ratios)] {
                    ui.label(l);
                    let on = *v != 0.0;
                    if ui.add(egui::Button::new(if on { "on" } else { "off" }).selected(on)).clicked() {
                        *v = if on { 0.0 } else { 1.0 };
                        changed = true;
                    }
                    ui.end_row();
                }
            });
            legend(ui, "space");
            egui::Grid::new("space-all").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                changed |= key_row(ui, "dry", &Key::Space("dry"), w, 0.0, 1.0, 0.001);
                for (k, a, b) in [("predelay", 0.0, 500.0), ("attack", 0.0, 1.0), ("early", 0.0, 1.0), ("late", 0.0, 1.0), ("modRate", 0.05, 10.0), ("modDepth", 0.0, 1.0),
                    ("lowCut", 10.0, 1000.0), ("bassMult", 0.25, 4.0), ("bassXover", 50.0, 2000.0), ("highShelf", -24.0, 0.0), ("highFreq", 500.0, 16000.0),
                    ("earlyLate", 0.0, 1.0), ("width", 0.0, 1.5), ("shimmerPitch", -12.0, 24.0)]
                {
                    let st = if b - a >= 100.0 { 1.0 } else { 0.01 };
                    changed |= key_row(ui, k, &Key::Space(k), w, a, b, st);
                }
            });
            legend(ui, "stats");
            ui.label(format!("t={:.2} {} · events {} · steps {}", w.clock.view, if w.clock.paused { "paused" } else { "run" }, w.stats.events, w.stats.steps));
            ui.label(format!("world: c{} a{} p{} r{} b{} s{} cam{} sph{} pr{} fx{}", w.cogs.len(), w.arms.len(), w.pads.len(), w.readers.len(), w.belts.len(),
                w.strings.len(), w.cams.len(), w.pits.len(), w.probes.len(), w.fields.len()));
            legend(ui, "json");
            ui.horizontal_wrapped(|ui| {
                if ui.button("export").clicked() {
                    self.json = w.to_string();
                    self.file_msg = Some("ok".into());
                }
                if ui.button("import").clicked() {
                    let text = self.json.clone();
                    let msg = self.open_text(w, &text);
                    self.file_msg = Some(msg);
                }
            });
            ui.add(egui::TextEdit::multiline(&mut self.json).desired_rows(4).desired_width(f32::INFINITY).font(egui::TextStyle::Monospace));
        });
        ui.add_space(48.0);
        if changed || structural {
            self.changed(w, structural);
        }
    }

    fn sel_title(&self, w: &World) -> String {
        match self.sel {
            None => "selection".into(),
            Some(Sel::Cog(i)) => {
                let k = w.cog(i).map(|c| if c.track.is_some() { "disc" } else if c.motor.is_some() { "motor" } else { "cog" }).unwrap_or("cog");
                format!("{k} {i}")
            }
            Some(s) => format!("{} {}", s.name(), super::shelf::sel_id(s)),
        }
    }

    /// buildInspector: the selection's rows. Returns (changed, structural).
    fn inspector(&mut self, ui: &mut egui::Ui, w: &mut World) -> (bool, bool) {
        let (mut ch, mut st) = (false, false);
        let Some(sel) = self.sel else {
            ui.label(RichText::new("nothing selected").color(DIM));
            return (false, false);
        };
        egui::Grid::new("inspector").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| match sel {
            Sel::Cog(id) => {
                let Some(ci) = w.cog_ix(id) else { return };
                let n = w.cogs[ci].n;
                if let Some(v) = num_row(ui, "teeth", n, 6.0, 240.0, 1.0) {
                    let g = w.resizer(ci);
                    g.set(w, v);
                    st = true;
                }
                let motor = w.cogs[ci].motor.is_some() || w.cogs[ci].driver.is_some();
                if motor {
                    if let Some(v) = num_row(ui, "rpm", w.rpm(ci), -600.0, 600.0, 0.1) {
                        w.drive(ci, v);
                        st = true;
                    }
                } else {
                    ui.label("rpm");
                    ui.label(RichText::new("no motor in this train").color(HOT));
                    ui.end_row();
                }
                if let Some(v) = num_row(ui, "layer", w.cogs[ci].layer, 0.0, 7.0, 1.0) {
                    w.cogs[ci].layer = v;
                    st = true;
                }
                let ph = cogs_sim::wrap(w.render_th(ci)).to_degrees();
                if let Some(v) = num_row(ui, "phase deg", ph, 0.0, 360.0, 0.1) {
                    w.set_phase(ci, v.to_radians());
                    ch = true;
                }
                let c = &w.cogs[ci];
                ro_row(ui, "state", if c.jam { "JAM".into() } else if c.w != 0.0 { "turning".into() } else { "idle".into() });
                if c.track.is_some() {
                    for (key, lo, hi) in [("octave", 1.0, 7.0), ("range", 1.0, 3.0), ("div", 1.0, 96.0)] {
                        ch |= key_row(ui, if key == "div" { "steps" } else { key }, &Key::Track(id, key), w, lo, hi, 1.0);
                    }
                    let k = Key::Track(id, "scale");
                    ch |= choice_row_s(ui, "scale", &k, w, &SCALES.map(|s| s.0));
                    ch |= choice_row(ui, "root", &Key::Track(id, "root"), w, &NOTE_NAMES);
                    ui.label("");
                    if ui.button("edit notes").clicked() {
                        self.open_notes(w);
                    }
                    ui.end_row();
                } else if c.motor.is_none() {
                    ui.label("");
                    if ui.button("add track").clicked() {
                        w.cogs[ci].track = Some(cogs_sim::Track::default());
                        st = true;
                    }
                    ui.end_row();
                }
            }
            Sel::Arm(id) => {
                let Some(ai) = w.arms.iter().position(|a| a.id == id) else { return };
                let a = &mut w.arms[ai];
                if let Some(v) = num_row(ui, "angle deg", a.a.to_degrees(), 0.0, 360.0, 0.1) {
                    a.a = v.to_radians();
                    ch = true;
                }
                for (l, lo, hi, stp) in [("r0", 0.0, 500.0, 0.5), ("len", 1.0, 800.0, 0.5), ("flex", 0.0, 1.0, 0.01)] {
                    let v = match l {
                        "r0" => a.r0,
                        "len" => a.len,
                        _ => a.flex,
                    };
                    if let Some(v) = num_row(ui, l, v, lo, hi, stp) {
                        match l {
                            "r0" => a.r0 = v,
                            "len" => a.len = v,
                            _ => a.flex = v,
                        }
                        ch = true;
                    }
                }
                let c = &w.cogs[w.arms[ai].cog_ix];
                ro_row(ui, "cog", format!("c{}", c.id));
                ro_row(ui, "tip px/s", format!("{:.1}", c.w.abs() * (w.arms[ai].r0 + w.arms[ai].len)));
            }
            Sel::Pad(id) => {
                if let Some(o) = w.pads.iter_mut().find(|o| o.id == id) {
                    if let Some(v) = num_row(ui, "radius", o.r, 2.0, 200.0, 0.5) {
                        o.r = v;
                        ch = true;
                    }
                }
                ch |= self.voice_rows(ui, w, Part::Pad(id), true);
                if let Some(o) = w.pads.iter().find(|o| o.id == id) {
                    ro_row(ui, "hits", o.hits.to_string());
                    ro_row(ui, "last vn", format!("{:.1}", o.last_v));
                }
            }
            Sel::Reader(id) => {
                ch |= self.voice_rows(ui, w, Part::Reader(id), false);
                if w.readers.iter().any(|r| r.id == id && r.tines != 0.0) {
                    ch |= key_row(ui, "comb tines", &Key::Reader(id, "tines"), w, 1.0, 36.0, 1.0);
                    ch |= key_row(ui, "comb note", &Key::Reader(id, "combBase"), w, 12.0, 108.0, 1.0);
                    ch |= choice_row_s(ui, "comb scale", &Key::Reader(id, "combScale"), w, &SCALES.map(|s| s.0));
                }
                if let Some(o) = w.readers.iter().find(|o| o.id == id) {
                    ro_row(ui, "last note", if o.hits > 0 { note_name(o.last_midi as i64) } else { "-".into() });
                    ro_row(ui, "hits", o.hits.to_string());
                }
            }
            Sel::Pit(id) => {
                ch |= self.voice_rows(ui, w, Part::Pit(id), false);
                if let Some(p) = w.pits.iter_mut().find(|p| p.id == id) {
                    ui.label("notes");
                    ui.horizontal_wrapped(|ui| {
                        for m in 48..84 {
                            let on = p.notes.contains(&m);
                            let b = ui.add(egui::Button::new(RichText::new(note_name(m as i64)).size(11.0)).selected(on).min_size(vec2(36.0, 30.0)));
                            if b.clicked() {
                                if on && p.notes.len() > 1 {
                                    p.notes.retain(|n| *n != m);
                                } else if !on {
                                    p.notes.push(m);
                                    p.notes.sort();
                                }
                                p.retune();
                                ch = true;
                            }
                        }
                    });
                    ui.end_row();
                    ro_row(ui, "balls", p.balls.len().to_string());
                    ui.label("");
                    if ui.button("clear balls").clicked() {
                        p.balls.clear();
                        ch = true;
                    }
                    ui.end_row();
                }
            }
            Sel::Belt(id) => {
                if let Some(b) = w.belts.iter_mut().find(|b| b.id == id) {
                    ui.label("crossed");
                    if ui.add(egui::Button::new(if b.cross { "on" } else { "off" }).selected(b.cross)).clicked() {
                        b.cross = !b.cross;
                        st = true;
                    }
                    ui.end_row();
                }
            }
            Sel::String(id) => {
                let k = |key| Key::Str(id, key);
                ch |= choice_row_s(ui, "kind", &k("kind"), w, &["string", "bar"]);
                ch |= key_row(ui, "f @200px", &k("f200"), w, 20.0, 2000.0, 1.0);
                ch |= toggle_row(ui, "quantize", &k("quant"), w);
                for (key, hi) in [("decay", 8.0), ("damp", 2.0), ("hard", 1.0), ("level", 1.5), ("velSense", 1.0)] {
                    ch |= key_row(ui, key, &k(key), w, 0.0, hi, 0.01);
                }
                if let Some(s) = w.strings.iter().find(|s| s.id == id) {
                    ro_row(ui, "length", format!("{:.1}", s.len()));
                    ro_row(ui, "note", format!("{} {:.2}", note_name(s.midi().round() as i64), s.midi()));
                }
            }
            Sel::Cam(id) => {
                let k = |key| Key::Cam(id, key);
                let mut shapes = cogs_sim::motion::CAM_SHAPE_NAMES.to_vec();
                shapes.push("drawn");
                ch |= choice_row_s(ui, "shape", &k("shape"), w, &shapes);
                for (key, l) in [("phase", "phase"), ("lo", "low"), ("hi", "high")] {
                    ch |= key_row(ui, l, &k(key), w, 0.0, 1.0, 0.01);
                }
                ch |= key_row(ui, "macro", &k("macro"), w, 1.0, 8.0, 1.0);
                let synths: Vec<u32> = w.readers.iter().filter(|r| r.voice.kind == VoiceKind::Wave).map(|r| r.id).collect();
                let cur = w.cams.iter().find(|m| m.id == id).and_then(|m| m.to);
                ui.label("turns");
                egui::ComboBox::from_id_salt("cam-to").selected_text(cur.map(|r| format!("reader {r}")).unwrap_or("nothing".into())).show_ui(ui, |ui| {
                    if ui.selectable_label(cur.is_none(), "nothing").clicked() {
                        if let Some(m) = w.cams.iter_mut().find(|m| m.id == id) {
                            m.to = None;
                        }
                        st = true;
                    }
                    for r in synths {
                        if ui.selectable_label(cur == Some(r), format!("reader {r}")).clicked() {
                            w.link_cam(id, r);
                            st = true;
                        }
                    }
                });
                ui.end_row();
                if let Some(m) = w.cams.iter().find(|m| m.id == id) {
                    ro_row(ui, "travel", format!("{}%", (w.cam_lift(m) * 100.0).round()));
                }
            }
            Sel::Probe(id) => {
                let k = |key| Key::Probe(id, key);
                ch |= choice_row_s(ui, "axis", &k("axis"), w, &["y", "x", "reach"]);
                let targets: Vec<Part> = w.readers.iter().map(|o| Part::Reader(o.id)).chain(w.pads.iter().map(|o| Part::Pad(o.id))).chain(w.pits.iter().map(|o| Part::Pit(o.id))).collect();
                let cur = w.probes.iter().find(|p| p.id == id).and_then(|p| p.to);
                let name = |p: Part| match p {
                    Part::Reader(i) => format!("reader {i}"),
                    Part::Pad(i) => format!("pad {i}"),
                    Part::Pit(i) => format!("sphere {i}"),
                    Part::String(i) => format!("string {i}"),
                };
                ui.label("drives");
                egui::ComboBox::from_id_salt("probe-to").selected_text(cur.map(name).unwrap_or("nothing".into())).show_ui(ui, |ui| {
                    if ui.selectable_label(cur.is_none(), "nothing").clicked() {
                        if let Some(p) = w.probes.iter_mut().find(|p| p.id == id) {
                            p.to = None;
                        }
                        st = true;
                    }
                    for t in targets {
                        if ui.selectable_label(cur == Some(t), name(t)).clicked() {
                            w.link_probe(id, t);
                            st = true;
                        }
                    }
                });
                ui.end_row();
                match cur.and_then(|t| w.voice(t)).map(|v| v.kind) {
                    Some(VoiceKind::Wave) => ch |= key_row(ui, "macro", &k("macro"), w, 1.0, 8.0, 1.0),
                    Some(kind) => {
                        let defs: Vec<&str> = cogs_sim::voice::probe_defs(kind).iter().map(|d| d.0).collect();
                        ch |= choice_row_s(ui, "param", &k("param"), w, &defs);
                    }
                    None => {}
                }
                ch |= key_row(ui, "low", &k("lo"), w, 0.0, 1.0, 0.01);
                ch |= key_row(ui, "high", &k("hi"), w, 0.0, 1.0, 0.01);
                if let Some(pr) = w.probes.iter().find(|p| p.id == id) {
                    ro_row(ui, "value", format!("{}%", ((pr.lo + (pr.hi - pr.lo) * w.probe_now(pr)) * 100.0).round()));
                }
            }
            Sel::Field(id) => {
                let k = |key| Key::Field(id, key);
                ch |= choice_row_s(ui, "effect", &k("mode"), w, &["delay", "reverb", "dist"]);
                let mode = w.fields.iter().find(|f| f.id == id).map(|f| f.mode.clone()).unwrap_or_default();
                match mode.as_str() {
                    "reverb" => {
                        ch |= choice_row(ui, "algorithm", &k("verbMode"), w, &VERB_MODES);
                        ch |= choice_row(ui, "color", &k("verbColor"), w, &VERB_COLORS);
                        ch |= key_row(ui, "size", &k("size"), w, 0.0, 1.0, 0.01);
                        ch |= key_row(ui, "decay s", &k("decay"), w, 0.2, 20.0, 0.01);
                        ch |= key_row(ui, "predelay ms", &k("predelay"), w, 0.0, 200.0, 1.0);
                    }
                    "dist" => {
                        ch |= choice_row_s(ui, "kind", &k("kind"), w, &cogs_sim::field::FIELD_DIST_KINDS);
                        ch |= key_row(ui, "drive", &k("drive"), w, 0.0, 1.0, 0.01);
                        ch |= key_row(ui, "tone", &k("tone"), w, 0.0, 1.0, 0.01);
                    }
                    _ => {
                        ch |= choice_row_s(ui, "time", &k("sync"), w, &cogs_sim::field::FIELD_SYNC_OPTS);
                        ch |= key_row(ui, "free time s", &k("time"), w, 0.02, 2.5, 0.005);
                        ch |= key_row(ui, "feedback", &k("fb"), w, 0.0, 0.95, 0.01);
                        ch |= key_row(ui, "tone", &k("tone"), w, 0.0, 1.0, 0.01);
                        ch |= toggle_row(ui, "ping-pong", &k("ping"), w);
                    }
                }
                ch |= key_row(ui, "diffuse", &k("diffuse"), w, 0.0, 1.0, 0.01);
                ch |= key_row(ui, "return", &k("level"), w, 0.0, 1.5, 0.01);
                ch |= key_row(ui, "takes dry", &k("insert"), w, 0.0, 1.0, 0.01);
                ch |= key_row(ui, "radius", &k("r"), w, 30.0, 800.0, 1.0);
                if let Some(f) = w.fields.iter().find(|f| f.id == id) {
                    ro_row(ui, "now", if f.slot < 0 { "silent: only 8 fields sound at once".into() } else { format!("slot {}", f.slot + 1) });
                }
            }
            Sel::Dot { cog, ring, a } => {
                let k = |key| Key::Dot(cog, ring, a, key);
                ch |= key_row(ui, "probability", &k("p"), w, 0.0, 1.0, 0.01);
                ch |= key_row(ui, "velocity", &k("v"), w, 0.0, 1.0, 0.01);
                let div = w.cog(cog).and_then(|c| c.track.as_ref()).map(|t| t.div).unwrap_or(16.0);
                ch |= key_row(ui, "length steps", &k("len"), w, 1.0, div, 1.0);
            }
        });
        (ch, st)
    }
    /// voiceRows: the voice's kind, its knobs, level and velocity sense; the wave synth's patch is its own editor.
    fn voice_rows(&mut self, ui: &mut egui::Ui, w: &mut World, p: Part, with_note: bool) -> bool {
        let mut ch = false;
        let Some(kind) = w.voice(p).map(|v| v.kind) else { return false };
        ui.label("kind");
        let kinds = ["synth", "kick", "snare", "hat", "tom", "clap", "rim", "cowbell", "wood", "cymbal", "wave"];
        egui::ComboBox::from_id_salt(("kind", p.id())).selected_text(bezel::kind_label(kind.name())).show_ui(ui, |ui| {
            for k in kinds {
                if ui.selectable_label(kind.name() == k, bezel::kind_label(k)).clicked() {
                    set(w, &Key::VoiceKind(p), Val::S(k.into()));
                    ch = true;
                }
            }
        });
        ui.end_row();
        if kind == VoiceKind::Wave {
            if with_note {
                ch |= key_row(ui, "note", &Key::Voice(p, "note"), w, 12.0, 108.0, 1.0);
            }
            ui.label("");
            if ui.button("edit the synth's patch").clicked() {
                self.sheet = None;
                self.out.push(StageOut::OpenEditor("patch"));
            }
            ui.end_row();
        } else {
            for (k, l, a, b, s) in bezel::voice_defs_pub(kind) {
                if k == "note" && !with_note {
                    continue;
                }
                if k == "body" {
                    ch |= choice_row_s(ui, l, &Key::Voice(p, "body"), w, &["harmonic", "bell", "plate", "bar", "bowl"]);
                } else {
                    ch |= key_row(ui, l, &Key::Voice(p, k), w, a, b, s);
                }
            }
            ui.label("");
            if ui.button("use the synth").clicked() {
                set(w, &Key::VoiceKind(p), Val::S("wave".into()));
                ch = true;
            }
            ui.end_row();
        }
        ch |= key_row(ui, "level", &Key::Voice(p, "level"), w, 0.0, 1.5, 0.01);
        ch |= key_row(ui, "vel sense", &Key::Voice(p, "velSense"), w, 0.0, 1.0, 0.01);
        ch
    }

    /// openCogsFile: a cogs file, or a bare machine (the old json export). Returns what happened, in words.
    pub fn open_text(&mut self, w: &mut World, text: &str) -> String {
        let j: serde_json::Value = match serde_json::from_str(text) {
            Ok(j) => j,
            Err(e) => return format!("not a cogs file: {e}"),
        };
        if j.get("osc").map_or(false, |o| o.is_array()) {
            return "that is a synth patch: open it in the synth editor".into();
        }
        let world = if j.get("cogsFile").is_some() { j.get("world").cloned().unwrap_or_default() } else { j };
        if !world.get("cogs").map_or(false, |c| c.is_array()) {
            return "not a cogs file: no machine in it".into();
        }
        let paused = w.clock.paused;
        match World::from_json(&world) {
            Ok(mut nw) => {
                nw.clock = w.clock.clone();
                crate::play::bump_versions(w, &mut nw);
                *w = nw;
                w.set_paused(paused);
                self.loaded(w);
                self.out.push(StageOut::Changed);
                let n = w.cogs.len() + w.arms.len() + w.pads.len() + w.readers.len() + w.belts.len() + w.strings.len() + w.cams.len() + w.probes.len();
                format!("machine loaded: {n} part{}", if n == 1 { "" } else { "s" })
            }
            Err(e) => format!("not loaded: {e}"),
        }
    }
}

/// The cogs file: the machine and (later) the patch library, in one.
pub fn cogs_file(w: &World) -> String {
    serde_json::json!({"cogsFile": 1, "world": w.serialize(), "patches": {}}).to_string()
}
pub fn cogs_file_name() -> String {
    "cogs.cogs.json".into()
}

// ---------- widgets ----------
pub(super) fn style(ui: &mut egui::Ui) {
    let s = ui.style_mut();
    s.visuals = egui::Visuals::dark();
    s.visuals.panel_fill = BG;
    s.visuals.window_fill = PAD;
    s.visuals.extreme_bg_color = Color32::BLACK;
    s.visuals.override_text_color = Some(FG);
    for wv in [&mut s.visuals.widgets.inactive, &mut s.visuals.widgets.hovered, &mut s.visuals.widgets.active, &mut s.visuals.widgets.noninteractive] {
        wv.corner_radius = egui::CornerRadius::same(8);
    }
    s.visuals.widgets.inactive.weak_bg_fill = PAD;
    s.visuals.widgets.inactive.bg_fill = PAD;
    s.visuals.widgets.hovered.weak_bg_fill = PAD;
    s.visuals.widgets.active.weak_bg_fill = LINE;
    s.visuals.selection.bg_fill = alpha(HOT, 0.35);
    s.visuals.selection.stroke = Stroke::new(1.0, HOT);
    s.visuals.handle_shape = egui::style::HandleShape::Circle;
    s.spacing.interact_size = vec2(44.0, 22.0);
    s.spacing.slider_width = 150.0;
    s.spacing.button_padding = vec2(10.0, 6.0);
    // the sheets read in the proportional face, at a few steady sizes
    use egui::TextStyle as T;
    s.text_styles.insert(T::Body, FontId::proportional(14.0));
    s.text_styles.insert(T::Button, FontId::proportional(14.0));
    s.text_styles.insert(T::Small, FontId::proportional(11.0));
    s.text_styles.insert(T::Heading, FontId::proportional(17.0));
}
/// A popover's title.
fn title(ui: &mut egui::Ui, t: &str) {
    ui.label(RichText::new(t).size(12.0).color(DIM));
    ui.add_space(2.0);
}
/// A choice button, lit when chosen; min width w (0: fit).
fn pick(ui: &mut egui::Ui, text: &str, on: bool, w: f32) -> bool {
    let b = egui::Button::new(RichText::new(text).color(if on { BG } else { FG })).fill(if on { HOT } else { LINE }).min_size(vec2(w.max(40.0), 40.0));
    ui.add(b).clicked()
}
/// A stepper showing a label instead of the number.
fn stepper_label(ui: &mut egui::Ui, v: f64, lo: f64, hi: f64, label: &str) -> Option<f64> {
    let mut out = None;
    ui.horizontal(|ui| {
        if ui.add_sized([40.0, 40.0], egui::Button::new("−")).clicked() && v > lo {
            out = Some(v - 1.0);
        }
        ui.add_sized([48.0, 40.0], egui::Label::new(RichText::new(label).size(15.0)));
        if ui.add_sized([40.0, 40.0], egui::Button::new("+")).clicked() && v < hi {
            out = Some(v + 1.0);
        }
    });
    out
}
fn legend(ui: &mut egui::Ui, t: &str) {
    ui.add_space(8.0);
    ui.label(RichText::new(t).color(DIM));
}
fn chip(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.vertical(|ui| {
        ui.label(RichText::new(label.to_uppercase()).size(11.0).color(DIM));
        add(ui);
    });
}
/// A number with − and + beside it.
fn stepper(ui: &mut egui::Ui, v: f64, lo: f64, hi: f64) -> Option<f64> {
    let mut out = None;
    ui.horizontal(|ui| {
        if ui.add_sized([40.0, 40.0], egui::Button::new("−")).clicked() && v > lo {
            out = Some(v - 1.0);
        }
        ui.add_sized([30.0, 40.0], egui::Label::new(RichText::new(setting(v)).size(15.0)));
        if ui.add_sized([40.0, 40.0], egui::Button::new("+")).clicked() && v < hi {
            out = Some(v + 1.0);
        }
    });
    out
}
fn setting(x: f64) -> String {
    bezel::setting_text(x)
}
fn num_row(ui: &mut egui::Ui, label: &str, v: f64, lo: f64, hi: f64, step: f64) -> Option<f64> {
    ui.label(label);
    let mut x = v;
    let r = ui.add(egui::Slider::new(&mut x, lo..=hi).step_by(step));
    ui.end_row();
    if r.changed() && x != v { Some(x) } else { None }
}
fn key_row(ui: &mut egui::Ui, label: &str, k: &Key, w: &mut World, lo: f64, hi: f64, step: f64) -> bool {
    let v = get(w, k).n();
    match num_row(ui, label, v, lo, hi, step) {
        Some(x) => {
            if set(w, k, Val::N(x)) {
                w.solve();
            }
            true
        }
        None => false,
    }
}
fn toggle_row(ui: &mut egui::Ui, label: &str, k: &Key, w: &mut World) -> bool {
    ui.label(label);
    let on = get(w, k).n() != 0.0;
    let r = ui.add(egui::Button::new(RichText::new(if on { "on" } else { "off" }).color(if on { HOT } else { FG })).selected(on));
    ui.end_row();
    if r.clicked() {
        if set(w, k, Val::N(if on { 0.0 } else { 1.0 })) {
            w.solve();
        }
        return true;
    }
    false
}
/// A choice of indices, shown by name.
fn choice_row(ui: &mut egui::Ui, label: &str, k: &Key, w: &mut World, names: &[&str]) -> bool {
    ui.label(label);
    let cur = get(w, k).n() as usize;
    let mut ch = false;
    egui::ComboBox::from_id_salt(("choice", label)).selected_text(names.get(cur).copied().unwrap_or("")).show_ui(ui, |ui| {
        for (i, n) in names.iter().enumerate() {
            if ui.selectable_label(i == cur, *n).clicked() {
                if set(w, k, Val::N(i as f64)) {
                    w.solve();
                }
                ch = true;
            }
        }
    });
    ui.end_row();
    ch
}
/// A choice of strings.
fn choice_row_s(ui: &mut egui::Ui, label: &str, k: &Key, w: &mut World, opts: &[&str]) -> bool {
    ui.label(label);
    let cur = match get(w, k) {
        Val::S(s) => s,
        Val::N(x) => setting(x),
    };
    let mut ch = false;
    egui::ComboBox::from_id_salt(("choice-s", label)).selected_text(cur.clone()).show_ui(ui, |ui| {
        for o in opts {
            if ui.selectable_label(*o == cur, *o).clicked() {
                if set(w, k, Val::S((*o).into())) {
                    w.solve();
                }
                ch = true;
            }
        }
    });
    ui.end_row();
    ch
}
fn ro_row(ui: &mut egui::Ui, label: &str, v: String) {
    ui.label(label);
    ui.label(RichText::new(v).color(HOT));
    ui.end_row();
}
