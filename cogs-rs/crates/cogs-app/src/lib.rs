//! The cogs app: one Rust program for Android (and, later, desktop and the web).
//! The audio callback owns the engine; everything else talks to it through lock-free rings.
pub mod audio;
pub mod midi;
pub mod testsynth;
pub mod stage;
#[cfg(target_os = "android")]
pub mod ui;
#[cfg(target_os = "android")]
mod android;
