//! The cogs app: one Rust program for Android (and, later, desktop and the web).
//! The audio callback owns the engine and runs the machine; everything else talks to it through rings and a lock
//! it never waits on.
pub mod midi;
pub mod play;
#[cfg(target_os = "android")]
pub mod ui;
#[cfg(target_os = "android")]
mod android;

/// The worlds that ship with the app.
pub const WORLDS: [(&str, &str); 3] = [
    ("Minimal & Dub House", include_str!("../../../../cogs/src/default-world.json")),
    ("Capstan", include_str!("../../../../cogs/worlds/capstan.json")),
    ("Dubstep", include_str!("../../../../cogs/worlds/dubstep.json")),
];
