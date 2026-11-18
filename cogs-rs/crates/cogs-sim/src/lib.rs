//! cogs' machine: the world (cogs, plates, arms, pads, readers, belts, strings, cams, boxes, probes, fields), the
//! solver, and `advance`, which steps it and reports every sound it makes, ported from sim.js, motion.js, pit.js,
//! probe.js and field.js. It reads and writes the same `.cogs.json` saves as the HTML build.

/// A sound the machine made, at `t` seconds of sim time.
#[derive(Clone, Debug)]
pub struct Event {
    pub t: f64,
    pub part: u32,
    pub midi: f32,
    pub amp: f32,
    pub pan: f32,
    pub vel: f32,
    pub hold: f32,
    pub dark: f32,
}
