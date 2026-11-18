//! Headless snapshots of the stage: an EGL pbuffer (Mesa's software GL is enough), egui painted by egui_glow, read back
//! to a PNG. For looking at the stage without a phone.
//!
//! cargo run -p cogs-app --example snapshot -- [--world PATH] [--out PNG] [--size 1080x2340] [--ppp 3] [--t SECS]
//!   [--zoom Z --view X,Y] [--sel cog:2] [--paused] [--do "tap 100,200" --do "drag 100,200 300,400" ...]
//! Points in --do are in stage points (pixels / ppp). Each --do runs over a few frames before the snapshot.

use cogs::stage::{Sel, Stage, StageIo};
use egui::{pos2, vec2, Event, RawInput, Rect, TouchDeviceId, TouchId, TouchPhase, ViewportId};
use glow::HasContext;
use khronos_egl as egl;
use std::sync::Arc;

fn arg<'a>(args: &'a [String], k: &str) -> Option<&'a str> {
    args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).map(|s| s.as_str())
}
fn pair(s: &str) -> (f32, f32) {
    let mut it = s.split(|c| c == ',' || c == 'x').map(|v| v.trim().parse::<f32>().unwrap());
    (it.next().unwrap(), it.next().unwrap())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../cogs");
    let world_path = arg(&args, "--world").map(|s| s.to_string()).unwrap_or(format!("{root}/src/default-world.json"));
    let out = arg(&args, "--out").unwrap_or("/tmp/stage.png").to_string();
    let (pw, ph) = pair(arg(&args, "--size").unwrap_or("1080x2340"));
    let (pw, ph) = (pw as i32, ph as i32);
    let ppp: f32 = arg(&args, "--ppp").and_then(|s| s.parse().ok()).unwrap_or(3.0);
    let t_end: f64 = arg(&args, "--t").and_then(|s| s.parse().ok()).unwrap_or(1.0);
    let size = vec2(pw as f32 / ppp, ph as f32 / ppp);

    // GL: an ES 3 context on a pbuffer
    // no window system here: Mesa's surfaceless platform
    if std::env::var_os("EGL_PLATFORM").is_none() {
        std::env::set_var("EGL_PLATFORM", "surfaceless");
    }
    let egl = egl::Instance::new(egl::Static);
    let display = unsafe { egl.get_display(egl::DEFAULT_DISPLAY) }.expect("egl display");
    egl.initialize(display).expect("egl init");
    egl.bind_api(egl::OPENGL_ES_API).expect("bind api");
    let attrs = [egl::RED_SIZE, 8, egl::GREEN_SIZE, 8, egl::BLUE_SIZE, 8, egl::ALPHA_SIZE, 8, egl::RENDERABLE_TYPE, egl::OPENGL_ES3_BIT,
        egl::SURFACE_TYPE, egl::PBUFFER_BIT, egl::NONE];
    let config = egl.choose_first_config(display, &attrs).expect("config").expect("no config");
    let ctx_attrs = [egl::CONTEXT_MAJOR_VERSION, 3, egl::NONE];
    let context = egl.create_context(display, config, None, &ctx_attrs).expect("context");
    let surface = egl.create_pbuffer_surface(display, config, &[egl::WIDTH, pw, egl::HEIGHT, ph, egl::NONE]).expect("pbuffer");
    egl.make_current(display, Some(surface), Some(surface), Some(context)).expect("make current");
    let gl = unsafe {
        glow::Context::from_loader_function(|s| egl.get_proc_address(s).map(|p| p as *const std::ffi::c_void).unwrap_or(std::ptr::null()))
    };
    let gl = Arc::new(gl);
    let mut painter = egui_glow::Painter::new(gl.clone(), "", None, false).expect("painter");

    // the world, run to t
    let text = std::fs::read_to_string(&world_path).expect("world");
    let mut w = cogs_sim::World::from_str(&text).expect("load");
    w.set_paused(false);
    let mut t = 0.0;
    while t < t_end {
        t = (t + 0.01).min(t_end);
        w.advance(t, &mut |_| {});
    }
    w.clock.view = t_end;
    if args.iter().any(|a| a == "--paused") {
        w.set_paused(true);
    }
    // --safe TOP,BOTTOM: the phone's insets in points (the Moto G57: 47,48 at ppp 2.5)
    let (safe_top, safe_bottom) = arg(&args, "--safe").map(pair).unwrap_or((0.0, 0.0));
    let io = StageIo { heard: t_end, safe_top, safe_bottom, ..Default::default() };
    let old_rows = args.iter().any(|a| a == "--old-rows");
    let mut stage = Stage::new();
    match arg(&args, "--zoom") {
        Some(z) => {
            w.s.zoom = z.parse().unwrap();
            if let Some(v) = arg(&args, "--view") {
                let (x, y) = pair(v);
                w.view.x = x as f64;
                w.view.y = y as f64;
            }
        }
        None => stage.fit_now(&mut w, size, &io),
    }
    if let Some(s) = arg(&args, "--sel") {
        let (k, id) = s.split_once(':').unwrap();
        let id: u32 = id.parse().unwrap();
        stage.sel = Some(match k {
            "cog" => Sel::Cog(id),
            "pad" => Sel::Pad(id),
            "reader" => Sel::Reader(id),
            "arm" => Sel::Arm(id),
            "cam" => Sel::Cam(id),
            "pit" => Sel::Pit(id),
            "probe" => Sel::Probe(id),
            "field" => Sel::Field(id),
            "string" => Sel::String(id),
            _ => panic!("sel kind"),
        });
    }

    match arg(&args, "--open") {
        Some("notes") => stage.open_notes(&w),
        Some("panel") => stage.toggle_panel(),
        _ => {}
    }
    if args.iter().any(|a| a == "--vel") {
        if let Some(r) = &mut stage.roll {
            r.vel = true;
        }
    }

    // scripted touches: each op becomes frames of touch events
    let mut frames: Vec<Vec<Event>> = vec![vec![]; 2];
    let touch = |id: u64, phase, x: f32, y: f32| Event::Touch { device_id: TouchDeviceId(0), id: TouchId(id), phase, pos: pos2(x, y), force: None };
    for (i, a) in args.iter().enumerate() {
        if a != "--do" {
            continue;
        }
        let op = &args[i + 1];
        let mut parts = op.split_whitespace();
        match parts.next().unwrap() {
            "tap" => {
                let (x, y) = pair(parts.next().unwrap());
                frames.push(vec![touch(1, TouchPhase::Start, x, y)]);
                frames.push(vec![touch(1, TouchPhase::End, x, y)]);
                frames.extend(std::iter::repeat_with(Vec::new).take(30));
            }
            "dbl" => {
                let (x, y) = pair(parts.next().unwrap());
                for _ in 0..2 {
                    frames.push(vec![touch(1, TouchPhase::Start, x, y)]);
                    frames.push(vec![touch(1, TouchPhase::End, x, y)]);
                    frames.extend(std::iter::repeat_with(Vec::new).take(3));
                }
                frames.extend(std::iter::repeat_with(Vec::new).take(30));
            }
            "drag" | "hold" => {
                let (x0, y0) = pair(parts.next().unwrap());
                let (x1, y1) = parts.next().map(pair).unwrap_or((x0, y0));
                frames.push(vec![touch(1, TouchPhase::Start, x0, y0)]);
                if op.starts_with("hold") {
                    frames.extend(std::iter::repeat_with(Vec::new).take(40));
                }
                for k in 1..=20 {
                    let f = k as f32 / 20.0;
                    frames.push(vec![touch(1, TouchPhase::Move, x0 + (x1 - x0) * f, y0 + (y1 - y0) * f)]);
                }
                if !parts.next().map_or(false, |s| s == "keep") {
                    frames.push(vec![touch(1, TouchPhase::End, x1, y1)]);
                }
                frames.extend(std::iter::repeat_with(Vec::new).take(5));
            }
            "pinch" => {
                // pinch CX,CY D0 D1: two fingers about a centre spread from d0 to d1
                let (cx, cy) = pair(parts.next().unwrap());
                let d0: f32 = parts.next().unwrap().parse().unwrap();
                let d1: f32 = parts.next().unwrap().parse().unwrap();
                frames.push(vec![touch(1, TouchPhase::Start, cx - d0 / 2.0, cy)]);
                frames.push(vec![touch(2, TouchPhase::Start, cx + d0 / 2.0, cy)]);
                for k in 1..=20 {
                    let d = d0 + (d1 - d0) * k as f32 / 20.0;
                    frames.push(vec![touch(1, TouchPhase::Move, cx - d / 2.0, cy), touch(2, TouchPhase::Move, cx + d / 2.0, cy)]);
                }
                frames.push(vec![touch(2, TouchPhase::End, cx + d1 / 2.0, cy), touch(1, TouchPhase::End, cx - d1 / 2.0, cy)]);
                frames.extend(std::iter::repeat_with(Vec::new).take(5));
            }
            // the Android Back button, through Stage::back
            "back" => {
                frames.push(vec![Event::Text("__back".into())]);
                frames.extend(std::iter::repeat_with(Vec::new).take(3));
            }
            "undo" => {
                frames.push(vec![Event::Text("__undo".into())]);
                frames.extend(std::iter::repeat_with(Vec::new).take(3));
            }
            // what the app does when a world is picked in the panel (ui.rs load)
            "load" => {
                frames.push(vec![Event::Text(format!("__load{}", parts.next().unwrap()))]);
                frames.extend(std::iter::repeat_with(Vec::new).take(3));
            }
            "wait" => frames.extend(std::iter::repeat_with(Vec::new).take(parts.next().unwrap().parse().unwrap())),
            o => panic!("unknown op {o}"),
        }
    }
    frames.extend(std::iter::repeat_with(Vec::new).take(25));

    // --editor TAB: the wave synth editor open on the selected part (or the first reader), on that tab
    let mut editor = arg(&args, "--editor").and_then(|tab| {
        let part = match stage.sel { Some(Sel::Reader(i)) => cogs_sim::Part::Reader(i), Some(Sel::Pad(i)) => cogs_sim::Part::Pad(i), _ => cogs_sim::Part::Reader(w.readers.iter().find(|r| r.voice.kind == cogs_sim::VoiceKind::Wave)?.id) };
        let mut e = cogs::wave_ui::WaveEditor::open(&w, part, Some(std::path::PathBuf::from("/tmp/cogs-patches")))?;
        e.set_tab(tab);
        Some(e)
    });
    let ctx = egui::Context::default();
    let mut foreign = std::collections::HashSet::new();
    let mut time = 0.0;
    let n = frames.len();
    for (k, events) in frames.into_iter().enumerate() {
        // as egui-winit does on a phone: the first finger is also the pointer, so widgets take it
        let mut events = events;
        if events.iter().any(|e| matches!(e, Event::Text(t) if t == "__back")) {
            events.clear();
            eprintln!("frame {k}: back -> {}", stage.back(&mut w));
        }
        if events.iter().any(|e| matches!(e, Event::Text(t) if t == "__undo")) {
            events.clear();
            let v0 = w.readers.iter().map(|r| r.voice.ver).max();
            stage.undo(&mut w);
            eprintln!("frame {k}: undo said {:?}; newest reader version {v0:?} -> {:?}", stage.edits.last_change, w.readers.iter().map(|r| r.voice.ver).max());
        }
        if let Some(i) = events.iter().find_map(|e| match e { Event::Text(t) => t.strip_prefix("__load").and_then(|n| n.parse::<usize>().ok()), _ => None }) {
            events.clear();
            let mut nw = cogs_sim::World::from_str(cogs::WORLDS[i].1).unwrap();
            cogs::play::bump_versions(&w, &mut nw);
            let paused = w.clock.paused;
            w = nw;
            w.set_paused(paused);
            stage.loaded(&w);
            stage.fit_now(&mut w, size, &io);
        }
        let mut sim = Vec::new();
        for e in &events {
            if let Event::Touch { id: TouchId(1), phase, pos, .. } = e {
                sim.push(Event::PointerMoved(*pos));
                match phase {
                    TouchPhase::Start => sim.push(Event::PointerButton { pos: *pos, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() }),
                    TouchPhase::End | TouchPhase::Cancel => { sim.push(Event::PointerButton { pos: *pos, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() }); sim.push(Event::PointerGone) }
                    _ => {}
                }
            }
        }
        events.extend(sim);
        let mut raw = RawInput { screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), size)), time: Some(time), events, ..Default::default() };
        raw.viewports.entry(ViewportId::ROOT).or_default().native_pixels_per_point = Some(ppp);
        time += 1.0 / 60.0;
        let mut outs = Vec::new();
        let full = ctx.run_ui(raw, |ui| {
            if editor.is_some() { cogs::wave_ui::strip_foreign(ui.ctx(), &mut foreign); }
            // rows like the app's in the panel (ui.rs); --old-rows sizes the chips as ui.rs did at b11f9b2
            outs = stage.show_with(ui, &mut w, &io, &mut |ui, _w| {
                ui.label(egui::RichText::new("No keyboard · tap Connect").size(12.0));
                ui.horizontal_wrapped(|ui| {
                    let _ = ui.add_sized([150.0, 40.0], egui::Button::new("connect keyboard"));
                    for (i, (name, _)) in cogs::WORLDS.iter().enumerate() {
                        let b = egui::Button::new(*name).selected(i == 0);
                        let _ = if old_rows { ui.add_sized([0.0, 40.0], b) } else { ui.add(b.min_size(vec2(0.0, 40.0))) };
                    }
                });
                ui.label(egui::RichText::new("audio 48000 Hz · buffer 40.0 ms · out 113.0 ms · glitches 0 · busy 11% · keys 0 · notes 38250").size(11.0));
            });
            if let Some(e) = editor.as_mut() { let ctx = ui.ctx().clone(); let _ = e.show_in(&ctx, &mut |f| f(&mut w)); }
        });
        for o in outs {
            eprintln!("frame {k}: {o:?}");
        }
        // paint every frame: it keeps the textures (fonts, icons) current, and the last one is read back
        let _ = (k, n);
        let prims = ctx.tessellate(full.shapes, full.pixels_per_point);
        let mut td = full.textures_delta;
        unsafe {
            gl.viewport(0, 0, pw, ph);
            gl.clear_color(0.0, 0.0, 0.0, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
        }
        painter.paint_and_update_textures([pw as u32, ph as u32], full.pixels_per_point, &prims, &mut td);
    }
    let mut buf = vec![0u8; (pw * ph * 4) as usize];
    unsafe {
        gl.read_pixels(0, 0, pw, ph, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(Some(&mut buf)));
    }
    let row = (pw * 4) as usize;
    let mut flipped = vec![0u8; buf.len()];
    for y in 0..ph as usize {
        flipped[y * row..(y + 1) * row].copy_from_slice(&buf[(ph as usize - 1 - y) * row..(ph as usize - y) * row]);
    }
    let f = std::fs::File::create(&out).expect("png");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), pw as u32, ph as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&flipped).unwrap();
    painter.destroy();
    eprintln!("wrote {out}: {} parts, zoom {:.3}, sel {:?}, group {}, undo {}, view {:.0},{:.0}", w.cogs.len() + w.pads.len() + w.readers.len(), w.s.zoom, stage.sel, stage.group.len(), stage.edits.can_undo(), w.view.x, w.view.y);
}
