//! The app's screen: the stage fills it. Holding the glyph opens the panel (worlds, play, what the keyboard plays,
//! audio readouts) until the real panels are ported. Selecting a part with a synth points the keyboard at it.
use crate::midi::Midi;
use crate::play::Shared;
use crate::stage::{Sel, Stage, StageIo, StageOut};
use crate::wave_ui::{EditorOut, WaveEditor};
use cogs_sim::Part;
use crate::rec::{self, Rec};
use std::path::PathBuf;
use cogs_sim::World;
use eframe::egui;
use std::sync::{atomic::Ordering::Relaxed, Arc, Mutex};

/// A file the platform picked for opening: its text, or why not (empty: cancelled).
pub static FILE_IN: Mutex<Option<Result<String, String>>> = Mutex::new(None);

/// The window's real insets in pixels, top and bottom (cutout, status and navigation bars), set by the activity.
pub static SAFE_PX: [std::sync::atomic::AtomicU32; 2] = [std::sync::atomic::AtomicU32::new(0), std::sync::atomic::AtomicU32::new(0)];

pub struct App {
    pub sh: Arc<Shared>,
    pub status: Arc<Mutex<String>>,
    pub connect: Box<dyn Fn()>,
    stage: Stage,
    panel: bool,
    world_ix: usize,
    fitted: bool,
    note: Option<(String, f64)>,
    editor: Option<WaveEditor>,
    /// The app's own storage: the world between launches, the patch library.
    pub data: Option<PathBuf>,
    save_at: Option<f64>,
    rec: Option<Rec>,
    can_record: bool,
    foreign: std::collections::HashSet<u64>,
    /// Auditions to let go of, by key, at an egui time.
    offs: Vec<(u32, f64)>,
    pub open_file: Option<Box<dyn Fn()>>,
    pub save_file: Option<Box<dyn Fn(&str, &str)>>,
    /// Back with nothing left to close: send the app to the background.
    pub leave: Option<Box<dyn Fn()>>,
}

impl App {
    pub fn new(sh: Arc<Shared>, status: Arc<Mutex<String>>, connect: Box<dyn Fn()>) -> Self {
        App { sh, status, connect, stage: Stage::new(), panel: false, world_ix: 0, fitted: false, note: None, editor: None, data: None, save_at: None, rec: None, can_record: false, foreign: Default::default(), offs: Vec::new(), open_file: None, save_file: None, leave: None }
    }
    /// The part a selection plays from the keyboard: a reader, pad or box; a plate means its reader.
    fn keyboard_part(w: &World, sel: Option<Sel>) -> Option<u32> {
        match sel? {
            Sel::Reader(i) | Sel::Pad(i) | Sel::Pit(i) => Some(i),
            Sel::Cog(c) | Sel::Dot { cog: c, .. } => w.readers.iter().find(|r| r.on == Some(c)).map(|r| r.id),
            _ => None,
        }
    }
    fn load(&mut self, i: usize) {
        let Ok(nw) = World::from_str(crate::WORLDS[i].1) else { return };
        let mut nw = nw;
        if let Ok(mut w) = self.sh.world.lock() {
            // the new world's voices start their versions past the old ones', so the sync thread recompiles every
            // patch even where a part id is reused
            crate::play::bump_versions(&w, &mut nw);
            let paused = w.clock.paused; *w = nw; w.set_paused(paused); self.stage.loaded(&w);
        }
        self.world_ix = i; self.fitted = false; self.sh.midi_part.store(0, Relaxed); self.editor = None;
        self.save_at = Some(0.0);
    }
    /// Record onto the selected plate, or end the take (one undo step).
    fn toggle_rec(&mut self, keyboard: bool, now: f64) {
        let Ok(mut w) = self.sh.world.lock() else { return };
        if let Some(r) = self.rec.take() {
            self.sh.recording.store(false, Relaxed);
            let n = r.n;
            let at = w.clock.sim_t;
            let msg = r.stop(&mut w, at);
            drop(w);
            if n > 0 { self.stage.mark_changed() }
            self.note = Some((msg, now));
        } else {
            match Rec::start(&mut w, self.stage.sel, keyboard) {
                Ok((r, msg)) => { self.sh.midi_part.store(r.reader, Relaxed); self.rec = Some(r); self.sh.recording.store(true, Relaxed); self.note = Some((msg, now)) }
                Err(e) => self.note = Some((e.into(), now)),
            }
        }
    }
    /// The keys the audio thread heard, onto the plate.
    fn drain_rec(&mut self, now: f64) {
        let mut evs = Vec::new();
        if let Ok(mut c) = self.sh.rec.lock() { while let Ok(e) = c.pop() { evs.push(e) } }
        let Some(r) = self.rec.as_mut() else { return };
        let Ok(mut w) = self.sh.world.lock() else { return };
        if !r.live(&w) { drop(w); self.sh.recording.store(false, Relaxed); self.rec = None; self.note = Some(("recording stopped: its plate is gone".into(), now)); return }
        let vc = f32::from_bits(self.sh.velcurve.load(Relaxed)) as f64;
        for e in evs {
            if e.on { if let Some(m) = r.note_on(&mut w, e.note, e.vel, e.at, vc) { self.note = Some((m, now)) } } else { r.note_off(&mut w, e.note, e.at) }
        }
    }
    /// The part an editor opens on: a reader, pad or box.
    fn sel_part(sel: Option<Sel>) -> Option<Part> {
        match sel? { Sel::Reader(i) => Some(Part::Reader(i)), Sel::Pad(i) => Some(Part::Pad(i)), Sel::Pit(i) => Some(Part::Pit(i)), _ => None }
    }
    /// The world into the app's storage, written whole then renamed, so a kill mid-write keeps the last one.
    fn save_world(&self) {
        let Some(d) = &self.data else { return };
        let Ok(s) = self.sh.world.lock().map(|w| w.to_string()) else { return };
        let _ = std::fs::create_dir_all(d);
        let tmp = d.join("world.json.tmp");
        if std::fs::write(&tmp, s).is_ok() { let _ = std::fs::rename(&tmp, d.join("world.json")); }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if ctx.pixels_per_point() < 2.0 { ctx.set_pixels_per_point(2.5) }
        let connected = self.status.lock().map(|s| s.starts_with("Playing from")).unwrap_or(false);
        let ppp = ctx.pixels_per_point();
        let inset = |i: usize, least: f32| (SAFE_PX[i].load(Relaxed) as f32 / ppp).max(least);
        let io = StageIo { heard: self.sh.heard(), safe_top: inset(0, 28.0), safe_bottom: inset(1, 16.0), busy: self.panel || self.editor.is_some(), recording: self.rec.is_some(), can_record: self.can_record,
            midi_label: Some(if connected { "keyboard" } else { "MIDI" }), midi_connected: connected };
        let mut outs = Vec::new();
        let mut load_req = None;
        crate::wave_ui::strip_foreign(&ctx, &mut self.foreign);
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
            if let Ok(mut w) = self.sh.world.lock() {
                if !self.fitted { self.stage.fit_now(&mut w, ui.max_rect().size(), &io); self.fitted = true }
                // the app's rows in the stage's panel: the keyboard, the shipped worlds, audio readouts
                let (sh, status, ix) = (self.sh.clone(), self.status.clone(), self.world_ix);
                let mut connect = false;
                let load = &mut load_req;
                outs = self.stage.show_with(ui, &mut w, &io, &mut |ui, _w| {
                    let m = &sh.meters;
                    ui.label(egui::RichText::new(status.lock().map(|s| s.clone()).unwrap_or_default()).size(12.0));
                    ui.horizontal_wrapped(|ui| {
                        if ui.add_sized([150.0, 40.0], egui::Button::new("connect keyboard")).clicked() { connect = true }
                        for (i, (name, _)) in crate::WORLDS.iter().enumerate() {
                            if ui.add(egui::Button::new(*name).selected(i == ix).min_size(egui::vec2(0.0, 40.0))).clicked() { *load = Some(i) }
                        }
                    });
                    let sr = m.sample_rate.load(Relaxed).max(1) as f32;
                    ui.label(egui::RichText::new(format!("audio {} Hz · buffer {:.1} ms · out {:.1} ms · glitches {} · busy {}% · keys {} · notes {}",
                        sr as u32, m.buffer.load(Relaxed) as f32 / sr * 1000.0, m.latency_us.load(Relaxed) as f32 / 1000.0, m.xruns.load(Relaxed),
                        m.busy.load(Relaxed), m.notes.load(Relaxed), m.events.load(Relaxed))).size(11.0));
                });
                if connect { (self.connect)() }
                if let Some(p) = Self::keyboard_part(&w, self.stage.sel) { self.sh.midi_part.store(p, Relaxed) }
                self.can_record = rec::target(&w, self.stage.sel).map_or(false, |(_, r)| r.is_some());
                if let Some(r) = &self.rec { self.sh.midi_part.store(r.reader, Relaxed) }
            }
        });
        if let Some(i) = load_req { self.load(i) }
        // a file the picker handed back: open it as the stage's panel does
        if let Some(r) = FILE_IN.lock().ok().and_then(|mut f| f.take()) {
            let now = ctx.input(|i| i.time);
            match r {
                Ok(text) => {
                    let msg = match self.sh.world.lock() { Ok(mut w) => self.stage.open_text(&mut w, &text), Err(_) => "busy".into() };
                    self.editor = None; self.fitted = false; self.save_at = Some(0.0);
                    self.note = Some((msg, now));
                }
                Err(e) if !e.is_empty() => self.note = Some((e, now)),
                Err(_) => {}
            }
        }
        for o in outs {
            match o {
                StageOut::ConnectMidi => (self.connect)(),
                StageOut::TogglePanel => self.panel = !self.panel,
                StageOut::Audition { part, midi, key } => self.sh.midi(Midi::Audition { part: part.map(|p| p.id()).unwrap_or(0), midi: midi as f32, key }),
                StageOut::OpenEditor("patch") => {
                    let lib = self.data.as_ref().map(|d| d.join("patches"));
                    let ed = Self::sel_part(self.stage.sel).and_then(|p| self.sh.world.lock().ok().and_then(|w| WaveEditor::open(&w, p, lib)));
                    if ed.is_none() { self.note = Some(("This part's voice is a drum or a modal synth: only the wave synth has an editor so far.".into(), ctx.input(|i| i.time))) }
                    self.editor = ed;
                }
                StageOut::OpenEditor(kind) => self.note = Some((format!("The {kind} editor is being ported next."), ctx.input(|i| i.time))),
                StageOut::Changed => { if self.save_at.is_none() { self.save_at = Some(ctx.input(|i| i.time) + 1.5) } }
                StageOut::Record => self.toggle_rec(connected, ctx.input(|i| i.time)),
                StageOut::AuditionOff { key, after } => self.offs.push((key, ctx.input(|i| i.time) + after)),
                StageOut::Share(link) => { ctx.copy_text(link); self.note = Some(("share link copied".into(), ctx.input(|i| i.time))) }
                StageOut::SaveFile { name, text } => {
                    if let Some(f) = &self.save_file { f(&name, &text) } else {
                        let r = self.data.as_ref().ok_or_else(|| "no storage".to_string()).and_then(|d| { let p = d.join("files"); std::fs::create_dir_all(&p).map_err(|e| e.to_string())?; std::fs::write(p.join(&name), text).map_err(|e| e.to_string()).map(|_| name.clone()) });
                        self.note = Some((match r { Ok(n) => format!("saved {n} in the app's files"), Err(e) => format!("not saved: {e}") }, ctx.input(|i| i.time)));
                    }
                }
                StageOut::OpenFile => match &self.open_file { Some(f) => f(), None => self.note = Some(("No file picker here.".into(), ctx.input(|i| i.time))) },
                _ => {}
            }
        }
        if let Some((t, at)) = &self.note {
            if ctx.input(|i| i.time) - at > 2.5 { self.note = None } else {
                egui::Area::new(egui::Id::new("note")).anchor(egui::Align2::CENTER_TOP, [0.0, 40.0]).show(&ctx, |ui| { egui::Frame::popup(ui.style()).show(ui, |ui| ui.label(t.as_str())); });
            }
        }
        self.drain_rec(ctx.input(|i| i.time));
        let now = ctx.input(|i| i.time);
        let sh = self.sh.clone();
        self.offs.retain(|&(key, at)| if now >= at { sh.midi(Midi::Release { key }); false } else { true });
        if !self.offs.is_empty() { ctx.request_repaint() }
        if self.panel { self.panel_ui(&ctx) }
        // Android's Back closes what is open, the editor first
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::BrowserBack)) {
            let closed = if self.editor.is_some() { self.editor = None; true }
                else if self.panel { self.panel = false; true }
                else { self.sh.world.lock().map(|mut w| self.stage.back(&mut w)).unwrap_or(true) };
            if !closed { if let Some(f) = &self.leave { f() } }
        }
        if let Some(ed) = self.editor.as_mut() {
            ed.safe_bottom = io.safe_bottom;
            let ver0 = self.sh.world.lock().ok().and_then(|w| w.voice(ed.part).map(|v| v.ver));
            let outs = ed.show(&ctx, &self.sh.world);
            let ver1 = self.sh.world.lock().ok().and_then(|w| w.voice(ed.part).map(|v| v.ver));
            if ver0 != ver1 && self.save_at.is_none() { self.save_at = Some(ctx.input(|i| i.time) + 1.5) }
            for o in outs {
                match o {
                    EditorOut::Audition(part) => self.sh.midi(Midi::Audition { part, midi: 60.0, key: 0x7fff_f000 }),
                    EditorOut::Close => self.editor = None,
                }
            }
        }
        if let Some(t) = self.save_at {
            let now = ctx.input(|i| i.time);
            if now >= t { self.save_world(); self.save_at = None } else { ctx.request_repaint_after(std::time::Duration::from_secs_f64(t - now)) }
        }
    }
}

impl App {
    fn panel_ui(&mut self, ctx: &egui::Context) {
        let sh = self.sh.clone();
        let m = &sh.meters;
        let mut open = true;
        egui::Window::new("cogs").open(&mut open).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_BOTTOM, [0.0, -12.0]).show(ctx, |ui| {
            ui.label(self.status.lock().map(|s| s.clone()).unwrap_or_default());
            if ui.add_sized([200.0, 44.0], egui::Button::new("Connect keyboard")).clicked() { (self.connect)() }
            ui.separator();
            let mut load = None;
            ui.horizontal_wrapped(|ui| {
                for (i, (name, _)) in crate::WORLDS.iter().enumerate() {
                    if ui.add_sized([100.0, 40.0], egui::Button::new(*name).selected(i == self.world_ix)).clicked() { load = Some(i) }
                }
            });
            if let Some(i) = load { self.load(i) }
            ui.separator();
            let sr = m.sample_rate.load(Relaxed).max(1) as f32;
            ui.label(format!("audio {} Hz · buffer {} ({:.1} ms)", sr as u32, m.buffer.load(Relaxed), m.buffer.load(Relaxed) as f32 / sr * 1000.0));
            ui.label(format!("output latency {:.1} ms · glitches {} · audio busy {}%", m.latency_us.load(Relaxed) as f32 / 1000.0, m.xruns.load(Relaxed), m.busy.load(Relaxed)));
            ui.label(format!("keys {} · machine notes {}", m.notes.load(Relaxed), m.events.load(Relaxed)));
        });
        if !open { self.panel = false }
    }
}
