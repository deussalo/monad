//! cogs' sound: the audio engine, ported from the HTML build's worklets (wave.worklet.js, engine.worklet.js,
//! verb.js, climax.js, field.worklet.js) and the WebAudio drum graph in sim.js. Nothing here allocates or locks
//! inside `Engine::process`, so it can run in a real-time audio callback.
//!
//! Time is in frames at the engine's sample rate, counted from the engine's start (`Engine::frame`).

/// One thing for the engine to do, at a frame (a frame in the past plays at once).
#[derive(Clone, Debug)]
pub enum Cmd {
    /// A wave-synth patch for a part: everything a `wave` voice carries in a save, as JSON.
    Patch { part: u32, patch: std::sync::Arc<serde_json::Value> },
    /// A note on a part's voice. `key` names it for `Off`; `hold` > 0 releases it by itself after that many seconds.
    Note { at: u64, part: u32, key: u32, midi: f32, amp: f32, vel: f32, pan: f32, hold: f32, dark: f32 },
    /// Release the notes with this key.
    Off { at: u64, key: u32 },
    /// Every note off at once.
    Panic,
}

pub struct Engine {
    pub sample_rate: f32,
    frame: u64,
}

impl Engine {
    pub fn new(sample_rate: f32) -> Self { Engine { sample_rate, frame: 0 } }
    /// Frames rendered so far: the engine's clock.
    pub fn frame(&self) -> u64 { self.frame }
    pub fn command(&mut self, _cmd: Cmd) {}
    /// Render `l.len()` frames into the two channels (overwriting them).
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        l.fill(0.0);
        r.fill(0.0);
        self.frame += l.len() as u64;
    }
}
