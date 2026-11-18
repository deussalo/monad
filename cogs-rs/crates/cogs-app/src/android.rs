//! Android: AAudio for sound (low-latency, exclusive where the phone allows), a NativeActivity for the window, and a
//! thin Kotlin layer (MainActivity, MidiHub) for Bluetooth permission and MIDI devices, which hands every message to
//! `Native.midi` here.
use crate::{aaudio, midi, play, ui};
use winit::platform::android::activity::AndroidApp;
use jni::{objects::{JClass, JObject, JString}, sys::{jboolean, jint}, JNIEnv, JavaVM};
use std::sync::{atomic::Ordering::Relaxed, Arc, Mutex, OnceLock};

/// The running app's, replaced if the system recreates the activity in the same process.
static SHARED: Mutex<Option<Arc<play::Shared>>> = Mutex::new(None);
static STATUS: OnceLock<Arc<Mutex<String>>> = OnceLock::new();

#[no_mangle]
pub extern "system" fn Java_help_techmate_cogs_Native_midi(_env: JNIEnv, _c: JClass, st: jint, d1: jint, d2: jint) {
    // parked, nothing would read it until the sound comes back, and it would all play at that moment
    if let (Ok(g), Some(m)) = (SHARED.lock(), midi::decode(st as u8, d1 as u8, d2 as u8)) { if let Some(sh) = g.as_ref().filter(|s| !s.idle.load(Relaxed)) { sh.midi(m) } }
}

/// The activity's lifecycle (1 resume, 2 pause, 3 start, 4 stop, 5 first window focus), for the audio supervisor.
#[no_mangle]
pub extern "system" fn Java_help_techmate_cogs_Native_lifecycle(_env: JNIEnv, _c: JClass, what: jint) { aaudio::lifecycle(what) }

/// The machine is running, for the activity deciding whether leaving needs the playback service.
#[no_mangle]
pub extern "system" fn Java_help_techmate_cogs_Native_playing(_env: JNIEnv, _c: JClass) -> jboolean {
    let sh = SHARED.lock().ok().and_then(|g| g.clone());
    sh.and_then(|s| s.world.lock().ok().map(|w| !w.clock.paused)).unwrap_or(false) as jboolean
}

/// The notification's and media session's buttons: 1 play, 2 pause, 3 stop.
#[no_mangle]
pub extern "system" fn Java_help_techmate_cogs_Native_control(_env: JNIEnv, _c: JClass, what: jint) { aaudio::control(what) }

#[no_mangle]
pub extern "system" fn Java_help_techmate_cogs_Native_status(mut env: JNIEnv, _c: JClass, text: JString, _connected: jboolean) {
    if let (Some(s), Ok(t)) = (STATUS.get(), env.get_string(&text)) { if let Ok(mut s) = s.lock() { *s = t.into() } }
}

/// The window's insets in pixels, from the activity, whenever they change.
#[no_mangle]
pub extern "system" fn Java_help_techmate_cogs_Native_insets(_env: JNIEnv, _c: JClass, top: jint, bottom: jint) {
    ui::SAFE_PX[0].store(top.max(0) as u32, std::sync::atomic::Ordering::Relaxed);
    ui::SAFE_PX[1].store(bottom.max(0) as u32, std::sync::atomic::Ordering::Relaxed);
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

/// The launch's `--es audio ...` (aaudio.rs): `adb shell am start -S -n help.techmate.cogs/.MainActivity --es audio probe`.
fn intent_audio(vm: usize, act: usize) -> String {
    let run = || -> jni::errors::Result<String> {
        let vm = unsafe { JavaVM::from_raw(vm as *mut _) }?;
        let mut env = vm.attach_current_thread()?;
        let act = unsafe { JObject::from_raw(act as jni::sys::jobject) };
        let intent = env.call_method(&act, "getIntent", "()Landroid/content/Intent;", &[])?.l()?;
        if intent.is_null() { return Ok(String::new()) }
        let key = env.new_string("audio")?;
        let v = env.call_method(&intent, "getStringExtra", "(Ljava/lang/String;)Ljava/lang/String;", &[(&key).into()])?.l()?;
        if v.is_null() { return Ok(String::new()) }
        let s: String = env.get_string(&JString::from(v))?.into();
        Ok(s)
    };
    run().unwrap_or_default()
}

/// What the phone says about its output, for reading beside what each stream is granted.
fn log_audio_device(vm: usize, act: usize) {
    let run = || -> jni::errors::Result<()> {
        let vm = unsafe { JavaVM::from_raw(vm as *mut _) }?;
        let mut env = vm.attach_current_thread()?;
        let act = unsafe { JObject::from_raw(act as jni::sys::jobject) };
        let name = env.new_string("audio")?;
        let am = env.call_method(&act, "getSystemService", "(Ljava/lang/String;)Ljava/lang/Object;", &[(&name).into()])?.l()?;
        let mut line = String::from("audio device:");
        for key in ["android.media.property.OUTPUT_SAMPLE_RATE", "android.media.property.OUTPUT_FRAMES_PER_BUFFER", "android.media.property.SUPPORT_AUDIO_SOURCE_UNPROCESSED"] {
            let k = env.new_string(key)?;
            let v = env.call_method(&am, "getProperty", "(Ljava/lang/String;)Ljava/lang/String;", &[(&k).into()])?.l()?;
            let s: String = if v.is_null() { "-".into() } else { env.get_string(&JString::from(v))?.into() };
            line += &format!(" {}={s}", key.rsplit('.').next().unwrap_or(key));
        }
        let pm = env.call_method(&act, "getPackageManager", "()Landroid/content/pm/PackageManager;", &[])?.l()?;
        for f in ["android.hardware.audio.low_latency", "android.hardware.audio.pro"] {
            let k = env.new_string(f)?;
            let has = env.call_method(&pm, "hasSystemFeature", "(Ljava/lang/String;)Z", &[(&k).into()])?.z()?;
            line += &format!(" {}={has}", f.rsplit('.').next().unwrap_or(f));
        }
        log::info!("{line}");
        Ok(())
    };
    if let Err(e) = run() { log::warn!("audio device: {e}") }
}

#[no_mangle]
fn android_main(app: AndroidApp) {
    android_logger::init_once(android_logger::Config::default().with_max_level(log::LevelFilter::Info).with_tag("cogs"));
    let status = STATUS.get_or_init(|| Arc::new(Mutex::new("No keyboard · tap Connect".into()))).clone();
    let (vm, act) = (app.vm_as_ptr() as usize, app.activity_as_ptr() as usize);
    let opts = aaudio::Options::parse(&intent_audio(vm, act));
    log_audio_device(vm, act);
    if opts.bench { aaudio::bench() }
    if opts.probe { aaudio::probe("early", &opts) }
    let data = app.internal_data_path();
    // The world as it was left; the first launch (or a save that no longer loads) starts on the default world.
    let saved = data.as_ref().and_then(|d| std::fs::read_to_string(d.join("world.json")).ok()).and_then(|s| cogs_sim::World::from_str(&s).ok());
    // `world=N` in the launch options starts on a shipped world, for testing
    let forced = opts.world.and_then(|i| crate::WORLDS.get(i)).and_then(|w| cogs_sim::World::from_str(w.1).ok());
    let world = forced.or(saved).unwrap_or_else(|| cogs_sim::World::from_str(crate::WORLDS[0].1).unwrap_or_else(|_| cogs_sim::World::demo()));
    // The stream first: the engine runs at the rate it is granted. The engine is made here, off the audio thread.
    let stream = aaudio::open_best(&opts, None);
    let sr = stream.as_ref().map_or(48000, |s| s.info.rate) as f32;
    let (sh, render) = play::start(world, sr);
    if let Ok(mut g) = SHARED.lock() { *g = Some(sh.clone()) }
    if opts.stats || opts.probe { aaudio::log_stats(sh.clone()) }
    let st = status.clone();
    let first = if opts.probe { drop(stream); None } else { stream };
    let audio = aaudio::supervise(first, render, sh.clone(), opts, sr as i32, move || { if let Ok(mut s) = st.lock() { *s = "Audio failed to open".into() } });
    let connect = Box::new(move || call_activity(vm, act, "connectMidi", "()V", &[]));
    let leave: Box<dyn Fn()> = Box::new(move || call_activity(vm, act, "leaveApp", "()V", &[]));
    let open_file: Box<dyn Fn()> = Box::new(move || call_activity(vm, act, "openFile", "()V", &[]));
    let save_file: Box<dyn Fn(&str, &str)> = Box::new(move |name, text| {
        let Ok(jvm) = (unsafe { JavaVM::from_raw(vm as *mut _) }) else { return };
        let Ok(mut env) = jvm.attach_current_thread() else { return };
        let (Ok(n), Ok(t)) = (env.new_string(name), env.new_string(text)) else { return };
        let act = unsafe { JObject::from_raw(act as jni::sys::jobject) };
        if let Err(e) = env.call_method(&act, "saveFile", "(Ljava/lang/String;Ljava/lang/String;)V", &[(&n).into(), (&t).into()]) { log::error!("saveFile: {e}") }
    });
    let sh_end = sh.clone();
    let opts = eframe::NativeOptions { android_app: Some(app), ..Default::default() };
    let _ = eframe::run_native("cogs", opts, Box::new(move |_cc| { let mut a = ui::App::new(sh, status, connect); a.data = data; a.open_file = Some(open_file); a.save_file = Some(save_file); a.leave = Some(leave); Ok(Box::new(a)) }));
    sh_end.running.store(false, std::sync::atomic::Ordering::Relaxed);
    let _ = audio.join();
}
