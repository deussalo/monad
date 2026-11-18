//! The audio worklet's wasm: `cogs_app::link::AudioSide` behind a few C-ABI exports for web/worklet.js.
//! Single-threaded (the worklet's own thread); the state lives in one static.
use cogs::link::{AudioSide, QUANTUM};

static mut SIDE: Option<AudioSide> = None;
static mut INBOX: Vec<u8> = Vec::new();

#[allow(static_mut_refs)]
fn side() -> &'static mut AudioSide { unsafe { SIDE.as_mut().expect("cogs_init first") } }

/// Make the engine and an empty machine (slow: call it from the processor's constructor).
#[no_mangle]
pub extern "C" fn cogs_init(sample_rate: f32) { unsafe { SIDE = Some(AudioSide::new(sample_rate)) } }

/// Room for a message of `len` bytes; write it there, then call `cogs_message(len)`.
#[no_mangle]
#[allow(static_mut_refs)]
pub extern "C" fn cogs_inbox(len: usize) -> *mut u8 {
    unsafe { INBOX.clear(); INBOX.resize(len, 0); INBOX.as_mut_ptr() }
}
#[no_mangle]
#[allow(static_mut_refs)]
pub extern "C" fn cogs_message(len: usize) {
    let bytes = unsafe { &INBOX[..len.min(INBOX.len())] };
    if let Ok(s) = std::str::from_utf8(bytes) { side().message(s) }
}

/// Render one quantum; returns where its 2 × 128 samples are (left, then right). `ct`: the worklet's currentTime.
#[no_mangle]
pub extern "C" fn cogs_block(ct: f64) -> *const f32 {
    let s = side();
    s.block(ct);
    s.l.as_ptr()
}
/// The right channel of the last quantum.
#[no_mangle]
pub extern "C" fn cogs_right() -> *const f32 { side().r.as_ptr() }
#[no_mangle]
pub extern "C" fn cogs_quantum() -> usize { QUANTUM }

/// A state message for the UI after the last quantum: its length (0: none), at `cogs_outbox_ptr()`.
#[no_mangle]
pub extern "C" fn cogs_outbox_len() -> usize { side().outbox().map_or(0, |s| s.len()) }
#[no_mangle]
pub extern "C" fn cogs_outbox_ptr() -> *const u8 { side().outbox().map_or(std::ptr::null(), |s| s.as_ptr()) }
#[no_mangle]
pub extern "C" fn cogs_outbox_clear() { side().clear_outbox() }
