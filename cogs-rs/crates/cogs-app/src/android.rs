//! Android: AAudio for sound (low-latency, exclusive where the phone allows), a NativeActivity for the window, and a
//! thin Kotlin layer (MainActivity, MidiHub) for Bluetooth permission and MIDI devices, which hands every message to
//! `Native.midi` here.
use crate::{audio, midi, ui};
use winit::platform::android::activity::AndroidApp;
use jni::{objects::{JClass, JObject, JString}, sys::{jboolean, jint}, JNIEnv, JavaVM};
use ndk::audio::{AudioCallbackResult, AudioDirection, AudioFormat, AudioPerformanceMode, AudioSharingMode, AudioStream, AudioStreamBuilder};
use std::sync::{atomic::Ordering::Relaxed, Arc, Mutex, OnceLock};

static INBOX: OnceLock<Arc<audio::Inbox>> = OnceLock::new();
static STATUS: OnceLock<Arc<Mutex<String>>> = OnceLock::new();

#[no_mangle]
pub extern "system" fn Java_help_techmate_cogs_Native_midi(_env: JNIEnv, _c: JClass, st: jint, d1: jint, d2: jint) {
    if let (Some(ib), Some(m)) = (INBOX.get(), midi::decode(st as u8, d1 as u8, d2 as u8)) { ib.midi(m) }
}

#[no_mangle]
pub extern "system" fn Java_help_techmate_cogs_Native_status(mut env: JNIEnv, _c: JClass, text: JString, _connected: jboolean) {
    if let (Some(s), Ok(t)) = (STATUS.get(), env.get_string(&text)) { if let Ok(mut s) = s.lock() { *s = t.into() } }
}

fn open_audio(make: impl FnOnce(f32) -> audio::Render, inbox: Arc<audio::Inbox>) -> Option<AudioStream> {
    let probe = AudioStreamBuilder::new().ok()?.direction(AudioDirection::Output).performance_mode(AudioPerformanceMode::LowLatency)
        .sharing_mode(AudioSharingMode::Exclusive).format(AudioFormat::PCM_Float).channel_count(2);
    // The rate is the device's own, so nothing resamples on the way out; the engine is made for it.
    let sr = 48000.0f32;
    let mut render = make(sr);
    let mut scratch = vec![0f32; 2 * 8192];
    let mut written: i64 = 0;
    let ib = inbox.clone();
    let stream = probe.sample_rate(sr as i32).data_callback(Box::new(move |s: &AudioStream, data: *mut std::ffi::c_void, frames: i32| {
        let n = (frames as usize).min(4096);
        let (l, r) = scratch.split_at_mut(4096);
        render.block(&mut l[..n], &mut r[..n]);
        let out = unsafe { std::slice::from_raw_parts_mut(data as *mut f32, n * 2) };
        for i in 0..n { out[2 * i] = l[i]; out[2 * i + 1] = r[i] }
        written += n as i64;
        // Latency: what is queued ahead of the frame the speaker is playing now.
        if let Ok(ts) = s.timestamp(ndk::audio::Clockid::Monotonic) {
            let ahead = written - ts.frame_position;
            if ahead > 0 { ib.meters.latency_us.store((ahead as f64 / sr as f64 * 1e6) as u32, Relaxed) }
        }
        ib.meters.xruns.store(s.x_run_count().max(0) as u32, Relaxed);
        AudioCallbackResult::Continue
    })).error_callback(Box::new(|_s, e| log::error!("audio error {e:?}"))).open_stream().ok()?;
    // Two bursts of buffer: the least that survives a late callback.
    let burst = stream.frames_per_burst();
    let _ = stream.set_buffer_size_in_frames(burst * 2);
    let m = &inbox.meters;
    m.sample_rate.store(stream.sample_rate() as u32, Relaxed);
    m.burst.store(burst as u32, Relaxed);
    m.buffer.store(stream.buffer_size_in_frames() as u32, Relaxed);
    stream.request_start().ok()?;
    Some(stream)
}

#[no_mangle]
fn android_main(app: AndroidApp) {
    android_logger::init_once(android_logger::Config::default().with_max_level(log::LevelFilter::Info).with_tag("cogs"));
    let (inbox, make) = audio::channel();
    let _ = INBOX.set(inbox.clone());
    let status = STATUS.get_or_init(|| Arc::new(Mutex::new("No keyboard · tap Connect".into()))).clone();
    let stream = open_audio(make, inbox.clone());
    if stream.is_none() { *status.lock().unwrap() = "Audio failed to open".into() }
    let (vm, act) = (app.vm_as_ptr() as usize, app.activity_as_ptr() as usize);
    let connect = Box::new(move || {
        let Ok(vm) = (unsafe { JavaVM::from_raw(vm as *mut _) }) else { return };
        let Ok(mut env) = vm.attach_current_thread() else { return };
        let act = unsafe { JObject::from_raw(act as jni::sys::jobject) };
        if let Err(e) = env.call_method(&act, "connectMidi", "()V", &[]) { log::error!("connectMidi: {e}") }
    });
    let opts = eframe::NativeOptions { android_app: Some(app), ..Default::default() };
    let _ = eframe::run_native("cogs", opts, Box::new(move |_cc| Ok(Box::new(ui::App::new(inbox, status, connect)))));
    drop(stream);
}
