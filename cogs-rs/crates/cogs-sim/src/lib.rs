//! cogs' machine: the world (cogs, plates, arms, pads, readers, belts, strings, cams, boxes, probes, fields), the
//! solver, and `advance`, which steps it and reports every sound it makes, ported from sim.js, motion.js, pit.js,
//! probe.js and field.js. It reads and writes the same `.cogs.json` saves as the HTML build.
//!
//! ```ignore
//! let mut w = cogs_sim::World::from_str(json)?;
//! w.set_paused(false);
//! w.advance(t_block_end, &mut |e| engine.push(e));
//! ```
//!
//! Time is sim seconds (`World::clock.sim_t`). The app advances the world to each audio block's end and maps event
//! times to frames; it sets `clock.view` to the heard time (what is drawn), which `render_th`, the balls' drawn
//! places and setPhase read.

pub mod edit;
pub mod field;
pub mod model;
pub mod motion;
pub mod pit;
pub mod probe;
pub mod sim;
pub mod solve;
pub mod voice;
pub mod world;

pub use edit::{CogKind, PartRef};
pub use model::*;
pub use motion::{Motion, MotionFrom};
pub use sim::{chance, Event, Send, Source};
pub use solve::{Contact, Resizer};
pub use world::World;
