//! The stage in a desktop window, without sound: the world advances by the wall clock. For trying the stage on a PC.
//! cargo run -p cogs-app --example desktop -- [world.json]

use cogs::stage::{Stage, StageIo, StageOut};
use eframe::egui;
use std::time::Instant;

struct App {
    w: cogs_sim::World,
    stage: Stage,
    t0: Instant,
    off: f64,
    fitted: bool,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let now = self.t0.elapsed().as_secs_f64() - self.off;
        if !self.w.clock.paused {
            self.w.advance(now, &mut |_| {});
        }
        let io = StageIo { heard: now, ..Default::default() };
        if !self.fitted {
            self.fitted = true;
            self.stage.fit_now(&mut self.w, ui.max_rect().size(), &io);
        }
        for o in self.stage.show(ui, &mut self.w, &io) {
            if let StageOut::Paused { paused: false, .. } = o {
                // play resumes from the heard time
                self.off = self.t0.elapsed().as_secs_f64() - self.w.clock.sim_t;
            }
        }
    }
}

fn main() -> eframe::Result {
    let path = std::env::args().nth(1);
    let w = match path {
        Some(p) => cogs_sim::World::from_str(&std::fs::read_to_string(p).expect("read world")).expect("load world"),
        None => cogs_sim::World::demo(),
    };
    let opts = eframe::NativeOptions { viewport: egui::ViewportBuilder::default().with_inner_size([420.0, 860.0]), ..Default::default() };
    eframe::run_native("cogs stage", opts, Box::new(|_| Ok(Box::new(App { w, stage: Stage::new(), t0: Instant::now(), off: 0.0, fitted: false }))))
}
