//! Android: AAudio for sound (low-latency, exclusive where the phone allows), a NativeActivity for the window, and a
//! thin Kotlin layer (MainActivity, MidiHub) for Bluetooth permission and MIDI devices, which hands every message to
//! `Native.midi` here.
use crate::{aaudio, midi, play, ui};
use winit::platform::android::activity::AndroidApp;
use jni::{objects::{JClass, JObject, JString}, sys::{jboolean, jint}, JNIEnv, JavaVM};
use std::sync::{Arc, Mutex, OnceLock};

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
    if opts.probe { aaudio::probe("early", &opts) }
    let data = app.internal_data_path();
    // The world as it was left; the first launch (or a save that no longer loads) starts on the default world.
    let saved = data.as_ref().and_then(|d| std::fs::read_to_string(d.join("world.json")).ok()).and_then(|s| cogs_sim::World::from_str(&s).ok());
    let world = saved.unwrap_or_else(|| cogs_sim::World::from_str(crate::WORLDS[0].1).unwrap_or_else(|_| cogs_sim::World::demo()));
    // The stream first: the engine runs at the rate it is granted. The engine is made here, off the audio thread.
    let stream = aaudio::open_best(&opts, None);
    let sr = stream.as_ref().map_or(48000, |s| s.info.rate) as f32;
    let (sh, render) = play::start(world, sr);
    let _ = SHARED.set(sh.clone());
    if opts.stats || opts.probe { aaudio::log_stats(sh.clone()) }
    let held: Arc<Mutex<Option<aaudio::Stream>>> = Arc::new(Mutex::new(None));
    if opts.probe {
        // again with the window up and the app in front, with nothing else of ours open; the real stream after that
        drop(stream);
        let (h, s, o, st) = (held.clone(), sh.clone(), opts.clone(), status.clone());
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(3));
            aaudio::probe("late", &o);
            log::info!("audio probe done");
            match aaudio::open_best(&o, Some(sr as i32)).and_then(|x| x.start(render, s, o.bursts)) {
                Some(x) => *h.lock().unwrap() = Some(x),
                None => *st.lock().unwrap() = "Audio failed to open".into(),
            }
        });
    } else {
        match stream.and_then(|x| x.start(render, sh.clone(), opts.bursts)) {
            Some(x) => *held.lock().unwrap() = Some(x),
            None => *status.lock().unwrap() = "Audio failed to open".into(),
        }
    }
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
    drop(held);
}
