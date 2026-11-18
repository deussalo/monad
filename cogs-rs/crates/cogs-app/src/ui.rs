//! The app's screen: the stage fills it. Holding the glyph opens the panel (worlds, play, what the keyboard plays,
//! audio readouts) until the real panels are ported. Selecting a part with a synth points the keyboard at it.
use crate::midi::Midi;
use crate::play::Shared;
use crate::stage::{Sel, Stage, StageIo, StageOut};
use cogs_sim::World;
use eframe::egui;
use std::sync::{atomic::Ordering::Relaxed, Arc, Mutex};

pub struct App {
    pub sh: Arc<Shared>,
    pub status: Arc<Mutex<String>>,
    pub connect: Box<dyn Fn()>,
    stage: Stage,
    panel: bool,
    world_ix: usize,
    fitted: bool,
    note: Option<(String, f64)>,
}

impl App {
    pub fn new(sh: Arc<Shared>, status: Arc<Mutex<String>>, connect: Box<dyn Fn()>) -> Self {
        App { sh, status, connect, stage: Stage::new(), panel: false, world_ix: 0, fitted: false, note: None }
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
        if let Ok(mut w) = self.sh.world.lock() { let paused = w.clock.paused; *w = nw; w.set_paused(paused); self.stage.loaded(&w); }
        self.world_ix = i; self.fitted = false; self.sh.midi_part.store(0, Relaxed);
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if ctx.pixels_per_point() < 2.0 { ctx.set_pixels_per_point(2.5) }
        let connected = self.status.lock().map(|s| s.starts_with("Playing from")).unwrap_or(false);
        let io = StageIo { heard: self.sh.heard(), safe_top: 28.0, safe_bottom: 16.0, busy: self.panel, recording: false, can_record: false,
            midi_label: Some(if connected { "keyboard" } else { "MIDI" }), midi_connected: connected };
        let mut outs = Vec::new();
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
            if let Ok(mut w) = self.sh.world.lock() {
                if !self.fitted { self.stage.fit_now(&mut w, ui.max_rect().size(), &io); self.fitted = true }
                outs = self.stage.show(ui, &mut w, &io);
                if let Some(p) = Self::keyboard_part(&w, self.stage.sel) { self.sh.midi_part.store(p, Relaxed) }
            }
        });
        for o in outs {
            match o {
                StageOut::ConnectMidi => (self.connect)(),
                StageOut::TogglePanel => self.panel = !self.panel,
                StageOut::Audition { part, midi, key } => self.sh.midi(Midi::Audition { part: part.map(|p| p.id()).unwrap_or(0), midi: midi as f32, key }),
                StageOut::OpenEditor(kind) => self.note = Some((format!("The {kind} editor is being ported next."), ctx.input(|i| i.time))),
                StageOut::Record => self.note = Some(("Recording onto plates comes with the notes editor.".into(), ctx.input(|i| i.time))),
                _ => {}
            }
        }
        if let Some((t, at)) = &self.note {
            if ctx.input(|i| i.time) - at > 2.5 { self.note = None } else {
                egui::Area::new(egui::Id::new("note")).anchor(egui::Align2::CENTER_TOP, [0.0, 40.0]).show(&ctx, |ui| { egui::Frame::popup(ui.style()).show(ui, |ui| ui.label(t.as_str())); });
            }
        }
        if self.panel { self.panel_ui(&ctx) }
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
