//! Android: AAudio for sound (low-latency, exclusive where the phone allows), a NativeActivity for the window, and a
//! thin Kotlin layer (MainActivity, MidiHub) for Bluetooth permission and MIDI devices, which hands every message to
//! `Native.midi` here.
use crate::{midi, play, ui};
use winit::platform::android::activity::AndroidApp;
use jni::{objects::{JClass, JObject, JString}, sys::{jboolean, jint}, JNIEnv, JavaVM};
use ndk::audio::{AudioCallbackResult, AudioDirection, AudioFormat, AudioPerformanceMode, AudioSharingMode, AudioStream, AudioStreamBuilder};
use std::sync::{atomic::Ordering::Relaxed, Arc, Mutex, OnceLock};

static SHARED: OnceLock<Arc<play::Shared>> = OnceLock::new();
static STATUS: OnceLock<Arc<Mutex<String>>> = OnceLock::new();

#[no_mangle]
pub extern "system" fn Java_help_techmate_cogs_Native_midi(_env: JNIEnv, _c: JClass, st: jint, d1: jint, d2: jint) {
    if let (Some(sh), Some(m)) = (SHARED.get(), midi::decode(st as u8, d1 as u8, d2 as u8)) { sh.midi(m) }
}

#[no_mangle]
pub extern "system" fn Java_help_techmate_cogs_Native_status(mut env: JNIEnv, _c: JClass, text: JString, _connected: jboolean) {
    if let (Some(s), Ok(t)) = (STATUS.get(), env.get_string(&text)) { if let Ok(mut s) = s.lock() { *s = t.into() } }
}

/// The window's insets in pixels, from the activity, whenever they change.
#[no_mangle]
pub extern "system" fn Java_help_techmate_cogs_Native_insets(_env: JNIEnv, _c: JClass, top: jint, bottom: jint) {
    ui::SAFE_PX[0].store(top.max(0) as u32, Relaxed);
    ui::SAFE_PX[1].store(bottom.max(0) as u32, Relaxed);
}

fn open_audio(mut render: play::Render, sh: Arc<play::Shared>, sr: f32) -> Option<AudioStream> {
    let mut scratch = vec![0f32; 2 * 4096];
    let mut written: i64 = 0;
    let mut latency = 0.02f64;
    let ib = sh.clone();
    let stream = AudioStreamBuilder::new().ok()?.direction(AudioDirection::Output).performance_mode(AudioPerformanceMode::LowLatency)
        .sharing_mode(AudioSharingMode::Exclusive).format(AudioFormat::PCM_Float).channel_count(2).sample_rate(sr as i32)
        .data_callback(Box::new(move |s: &AudioStream, data: *mut std::ffi::c_void, frames: i32| {
        let t = std::time::Instant::now();
        let out = unsafe { std::slice::from_raw_parts_mut(data as *mut f32, frames.max(0) as usize * 2) };
        let mut done = 0usize;
        while done < frames as usize {
            let n = (frames as usize - done).min(4096);
            let (l, r) = scratch.split_at_mut(4096);
            render.block(&mut l[..n], &mut r[..n], latency);
            for i in 0..n { out[2 * (done + i)] = l[i]; out[2 * (done + i) + 1] = r[i] }
            done += n;
        }
        written += frames as i64;
        // Latency: what is queued ahead of the frame the speaker is playing now.
        if let Ok(ts) = s.timestamp(ndk::audio::Clockid::Monotonic) {
            let ahead = written - ts.frame_position;
            if ahead > 0 { latency = ahead as f64 / sr as f64; ib.meters.latency_us.store((latency * 1e6) as u32, Relaxed) }
        }
        let m = &ib.meters;
        m.xruns.store(s.x_run_count().max(0) as u32, Relaxed);
        let busy = t.elapsed().as_secs_f64() / (frames.max(1) as f64 / sr as f64) * 100.;
        m.busy.store(busy as u32, Relaxed);
        AudioCallbackResult::Continue
    })).error_callback(Box::new(|_s, e| log::error!("audio error {e:?}"))).open_stream().ok()?;
    // Two bursts of buffer: the least that survives a late callback.
    let burst = stream.frames_per_burst();
    let _ = stream.set_buffer_size_in_frames(burst * 2);
    let m = &sh.meters;
    m.sample_rate.store(stream.sample_rate() as u32, Relaxed);
    m.burst.store(burst as u32, Relaxed);
    m.buffer.store(stream.buffer_size_in_frames() as u32, Relaxed);
    stream.request_start().ok()?;
    Some(stream)
}

/// A picked file's text (or why there is none), for the UI to open.
#[no_mangle]
pub extern "system" fn Java_help_techmate_cogs_Native_fileText(mut env: JNIEnv, _c: JClass, text: JString, error: JString) {
    let get = |env: &mut JNIEnv, s: &JString| if s.is_null() { None } else { env.get_string(s).ok().map(String::from) };
    let r = match (get(&mut env, &text), get(&mut env, &error)) { (Some(t), _) => Ok(t), (None, Some(e)) => Err(e), (None, None) => Err(String::new()) };
    if let Ok(mut f) = ui::FILE_IN.lock() { *f = Some(r) }
}

/// Call a method on the activity from any thread.
fn call_activity(vm: usize, act: usize, name: &str, sig: &str, args: &[jni::objects::JValue]) {
    let Ok(vm) = (unsafe { JavaVM::from_raw(vm as *mut _) }) else { return };
    let Ok(mut env) = vm.attach_current_thread() else { return };
    let act = unsafe { JObject::from_raw(act as jni::sys::jobject) };
    if let Err(e) = env.call_method(&act, name, sig, args) { log::error!("{name}: {e}") }
}

#[no_mangle]
fn android_main(app: AndroidApp) {
    android_logger::init_once(android_logger::Config::default().with_max_level(log::LevelFilter::Info).with_tag("cogs"));
    let status = STATUS.get_or_init(|| Arc::new(Mutex::new("No keyboard · tap Connect".into()))).clone();
    // The engine is made here, off the audio thread: wavetables and buffers.
    let sr = 48000.0f32;
    let data = app.internal_data_path();
    // The world as it was left; the first launch (or a save that no longer loads) starts on the default world.
    let saved = data.as_ref().and_then(|d| std::fs::read_to_string(d.join("world.json")).ok()).and_then(|s| cogs_sim::World::from_str(&s).ok());
    let world = saved.unwrap_or_else(|| cogs_sim::World::from_str(crate::WORLDS[0].1).unwrap_or_else(|_| cogs_sim::World::demo()));
    let (sh, render) = play::start(world, sr);
    let _ = SHARED.set(sh.clone());
    let stream = open_audio(render, sh.clone(), sr);
    if stream.is_none() { *status.lock().unwrap() = "Audio failed to open".into() }
    let (vm, act) = (app.vm_as_ptr() as usize, app.activity_as_ptr() as usize);
    let connect = Box::new(move || call_activity(vm, act, "connectMidi", "()V", &[]));
    let open_file: Box<dyn Fn()> = Box::new(move || call_activity(vm, act, "openFile", "()V", &[]));
    let save_file: Box<dyn Fn(&str, &str)> = Box::new(move |name, text| {
        let Ok(jvm) = (unsafe { JavaVM::from_raw(vm as *mut _) }) else { return };
        let Ok(mut env) = jvm.attach_current_thread() else { return };
        let (Ok(n), Ok(t)) = (env.new_string(name), env.new_string(text)) else { return };
        let act = unsafe { JObject::from_raw(act as jni::sys::jobject) };
        if let Err(e) = env.call_method(&act, "saveFile", "(Ljava/lang/String;Ljava/lang/String;)V", &[(&n).into(), (&t).into()]) { log::error!("saveFile: {e}") }
    });
    let opts = eframe::NativeOptions { android_app: Some(app), ..Default::default() };
    let _ = eframe::run_native("cogs", opts, Box::new(move |_cc| { let mut a = ui::App::new(sh, status, connect); a.data = data; a.open_file = Some(open_file); a.save_file = Some(save_file); Ok(Box::new(a)) }));
    drop(stream);
}
