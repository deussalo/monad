// The wave synth: its worklet rendered offline (filters, modulation, every table), then its editor in desk.html:
// knobs, routing by arming a source, the matrix, presets, the library, JSON export and import, undo and saving.
const { chromium } = require(process.env.COGS_PW || '/srv/rig/rig/webapp/node_modules/.pnpm/playwright-core@1.64.0/node_modules/playwright-core');
const path = require('path');
const FILE = process.argv[2] || ('file://' + path.join(__dirname, 'desk.html'));
const SHOTS = process.env.COGS_SHOTS || '';
(async () => {
  const b = await chromium.launch({ executablePath: (process.env.MONAD_CHROME || '/srv/rig/.cache/ms-playwright/chromium-1248/chrome-linux64/chrome') });
  let pass = true;
  const check = (label, ok, got) => { if (!ok) pass = false; console.log(('  ' + label).padEnd(70), JSON.stringify(got), ok ? '✓' : '✗'); };
  const is = (label, got, want) => check(label, JSON.stringify(got) === JSON.stringify(want), got);
  const wait = ms => new Promise(r => setTimeout(r, ms));
  const ctx = await b.newContext({ viewport: { width: 1440, height: 900 }, permissions: ['clipboard-read', 'clipboard-write'] });
  const p = await ctx.newPage(), errs = [];
  p.on('pageerror', e => errs.push(e.message));
  const probe = (fn, arg) => p.evaluate(fn, arg);
  const settle = async () => { await p.waitForFunction(() => !__cogs.pending); await wait(50); };
  const shot = async name => { if (SHOTS) await p.screenshot({ path: path.join(SHOTS, name + '.png') }); };
  await p.goto(FILE); await wait(500);
  await p.mouse.click(40, 860); await wait(300);
  await p.waitForFunction(() => __cogs.engine().state === 'ready' && __cogs.engine().ac === 'running', null, { timeout: 5000 });

  console.log('WAVE WORKLET');
  const off = await probe(async () => {
    const url = 'data:text/javascript;charset=utf-8,' + encodeURIComponent(document.querySelector('#engine').textContent);
    const render = async (patch, notes, secs = .5) => {
      const ac = new OfflineAudioContext(2, Math.round(48000 * secs), 48000);
      await ac.audioWorklet.addModule(url);
      const n = new AudioWorkletNode(ac, 'cogs-wave', { numberOfInputs: 0, outputChannelCount: [2] });
      n.connect(ac.destination);
      // messages reach the audio thread while it is suspended, so the first note is in hand before it renders
      ac.suspend(128 / 48000).then(async () => { n.port.postMessage({ type: 'patch', id: 1, p: patch });
        for (const [at, midi, hold] of notes) n.port.postMessage({ type: 'note', id: 1, at: 256 + at * 48000, key: midi, midi, amp: .8, vel: .8, pan: 0, hold });
        await new Promise(r => setTimeout(r, 100)); ac.resume(); });
      const buf = await ac.startRendering(), L = buf.getChannelData(0), R = buf.getChannelData(1);
      let e = 0, d = 0, pk = 0, side = 0; for (let i = 1; i < L.length; i++) { e += L[i] * L[i]; d += (L[i] - L[i - 1]) ** 2; pk = Math.max(pk, Math.abs(L[i]), Math.abs(R[i])); side += (L[i] - R[i]) ** 2; }
      const win = k => { let s = 0; for (let i = k * 2400; i < (k + 1) * 2400; i++)s += L[i] * L[i]; return Math.sqrt(s / 2400); };
      return { rms: Math.sqrt(e / L.length), bright: Math.sqrt(d / L.length) / Math.max(1e-9, Math.sqrt(e / L.length)), peak: pk, side: Math.sqrt(side / L.length), wins: Array.from({ length: Math.floor(L.length / 2400) }, (_, k) => win(k)), finite: [...L].every(Number.isFinite) };
    };
    const base = () => { const p = waveInit(); p.osc[0].pos = .667; return p; };
    const out = {};
    out.open = await render(base(), [[0, 57, 0]]);
    const dark = base(); dark.osc[0].cut = 300; out.dark = await render(dark, [[0, 57, 0]]);
    const lfo = base(); lfo.osc[0].cut = 300; lfo.lfo[0].rate = 6; lfo.mods = [{ src: 'lfo1', dst: 'osc1.cut', amt: .7 }]; out.lfo = await render(lfo, [[0, 57, 1]], 1);
    const uni = base(); uni.osc[0].uni = 7; uni.osc[0].detune = .5; uni.osc[0].spread = 1; out.uni = await render(uni, [[0, 57, 0]]);
    out.tables = {}; for (const t of WAVE_TABLES) { const q = base(); q.osc[0].table = t; q.osc[0].filter = 'off'; out.tables[t] = +(await render(q, [[0, 60, 0]], .2)).rms.toFixed(4); }
    out.filters = {}; for (const f of WAVE_FILTERS) { const q = base(); q.osc[0].filter = f; q.osc[0].cut = 800; q.osc[0].res = .5; out.filters[f] = +(await render(q, [[0, 48, 0]], .2)).rms.toFixed(4); }
    const gate = base(); gate.gate = .1; gate.osc[0].r = .05; out.gate = await render(gate, [[0, 57, 0]], .5);
    const all = base(); all.osc.forEach(o => { o.on = true; o.level = .5; o.uni = 4; }); all.sub.level = .5; all.noise.level = .3; out.all = await render(all, [[0, 45, 0], [0, 52, 0], [0, 57, 0], [0, 61, 0]]);
    // a sine (wt pos 0) with a short release, so distortion shows as new harmonics and a tail is the rack's own
    const dry = () => { const q = base(); q.osc[0].pos = 0; q.osc[0].filter = 'off'; q.osc[0].r = .03; q.gate = .1; return q; };
    const fxp = (k, set) => { const q = dry(); q.fx[k].on = true; set && set(q); return q; };
    out.fx = {};
    for (const k of WAVE_FX) out.fx[k] = await render(fxp(k, q => { q.fx.dist.drive = .8; q.fx.delay.ping = false; }), [[0, 57, .2]], .8);
    out.fx.none = await render(dry(), [[0, 57, .2]], .8);
    out.fx.crush = await render(fxp('dist', q => { q.fx.dist.mode = 'crush'; q.fx.dist.drive = .8; }), [[0, 57, .2]], .4);
    out.delaySync = await render(fxp('delay', q => { q.fx.delay.sync = '1/4'; q.fx.delay.mix = 1; q.fx.delay.fb = 0; q.fx.delay.ping = false; q.osc[0].d = .02; q.osc[0].s = 0; q.osc[0].r = .02; q.gate = .02; }), [[0, 57, 0]], 1);
    const macro = fxp('dist', q => { q.fx.dist.drive = 0; q.macro[0] = 1; q.mods = [{ src: 'm1', dst: 'dist.drive', amt: 1 }]; });
    out.macroFx = await render(macro, [[0, 57, .2]], .3); out.macroFxOff = await render(fxp('dist', q => { q.fx.dist.drive = 0; }), [[0, 57, .2]], .3);
    const sync = base(); sync.osc[0].cut = 300; sync.lfo[0].sync = '1/4'; sync.lfo[0].shape = 'square'; sync.mods = [{ src: 'lfo1', dst: 'osc1.cut', amt: .8 }]; out.lfoSync = await render(sync, [[0, 57, 2]], 2);
    // voicing: two overlapping notes; poly keeps both, mono plays only the second, gliding up to it
    const pitchOf = (L, a, b) => { let z = 0, first = -1, last = -1; for (let i = Math.round(a * 48000); i < b * 48000; i++) if (L[i - 1] < 0 && L[i] >= 0) { if (first < 0) first = i; last = i; z++; } return (z - 1) / ((last - first) / 48000); };
    const raw = async (patch, notes, secs) => { const ac = new OfflineAudioContext(2, Math.round(48000 * secs), 48000); await ac.audioWorklet.addModule(url);
      const n = new AudioWorkletNode(ac, 'cogs-wave', { numberOfInputs: 0, outputChannelCount: [2] }); n.connect(ac.destination);
      ac.suspend(128 / 48000).then(async () => { n.port.postMessage({ type: 'patch', id: 1, p: patch }); for (const [at, midi, hold] of notes) n.port.postMessage({ type: 'note', id: 1, at: 256 + at * 48000, key: midi, midi, amp: .8, vel: .8, pan: 0, hold }); await new Promise(r => setTimeout(r, 100)); ac.resume(); });
      return (await ac.startRendering()).getChannelData(0); };
    const sine = () => { const q = waveInit(); q.osc[0].pos = 0; q.osc[0].filter = 'off'; q.osc[0].s = 1; return q; };
    const poly = await raw(sine(), [[0, 57, 1], [.2, 69, 1]], .6);
    const mono = sine(); mono.voicing.mode = 'mono'; mono.glide = .05; mono.voicing.always = false; const monoL = await raw(mono, [[0, 57, 1], [.2, 69, 1]], .6);
    const rms = (L, a, b) => { let e = 0, n = 0; for (let i = Math.round(a * 48000); i < b * 48000; i++) { e += L[i] * L[i]; n++; } return Math.sqrt(e / n); };
    const tr = (o, se) => { const q = sine(); q.osc[0].oct = o; q.osc[0].semi = se; return q; };
    out.oscPitch = [pitchOf(await raw(tr(0, 0), [[0, 57, 1]], .3), .05, .28), pitchOf(await raw(tr(1, 0), [[0, 57, 1]], .3), .05, .28), pitchOf(await raw(tr(-1, 7), [[0, 57, 1]], .3), .05, .28), pitchOf(await raw((q => { q.tune.oct = 1; q.tune.semi = -12; q.osc[0].semi = 7; return q; })(sine()), [[0, 57, 1]], .3), .05, .28)].map(Math.round);
    out.voicing = { polyRms: rms(poly, .4, .58), monoRms: rms(monoL, .4, .58), monoHz: pitchOf(monoL, .4, .58), monoEarly: pitchOf(monoL, .206, .236) };
    const aux = base(); aux.osc[0].cut = 300; aux.lfo[0].rate = 6; aux.macro[0] = 0; aux.mods = [{ src: 'lfo1', dst: 'osc1.cut', amt: .7, aux: 'm1' }]; out.aux0 = await render(aux, [[0, 57, 1]], 1);
    aux.macro[0] = 1; out.aux1 = await render(aux, [[0, 57, 1]], 1);
    const bl = v => { const q = sine(); q.osc[0].uni = 3; q.osc[0].blend = v; q.osc[0].detune = .6; return q; }; out.blend = [await render(bl(0), [[0, 57, 0]], .3), await render(bl(1), [[0, 57, 0]], .3)];
    out.curve = [modCurve(.5, 0), modCurve(.5, 1), modCurve(.5, -1), modCurve(0, 1), modCurve(1, -1)];
    // order matters: a 110 Hz sine cut 15 dB by the low shelf before distortion drives it less than cutting after it
    const ord = o => fxp('dist', q => { q.fx.dist.drive = .8; q.fx.eq.on = true; q.fx.eq.low = -15; q.fxOrder = o; });
    out.order = [await render(ord(['dist', 'eq']), [[0, 45, .2]], .3), await render(ord(['eq', 'dist']), [[0, 45, .2]], .3)];
    // every warp mode, at 0.7 in slot 1 and slot 2, on a saw: each changes the sound and stays finite
    out.warps = {};
    for (const wm of WARP_MODES) for (const slot of [1, 2]) { const q = base(); q.osc[0].filter = 'off';
      if (slot === 1) { q.osc[0].warpMode = wm; q.osc[0].warp = .7; } else { q.osc[0].warpMode = 'off'; q.osc[0].warp2Mode = wm; q.osc[0].warp2 = .7; }
      const r = await render(q, [[0, 57, .2]], .25); out.warps[wm + slot] = [+r.rms.toFixed(4), +r.bright.toFixed(4), r.finite]; }
    { const q = base(); q.osc[0].filter = 'off'; out.warps.none = (r => [+r.rms.toFixed(4), +r.bright.toFixed(4), r.finite])(await render(q, [[0, 57, .2]], .25)); }
    out.presets = {}; for (const [k, f] of Object.entries(WAVE_PRESETS)) out.presets[k] = +(await render(waveVoice(f()), [[0, 57, .3]], .4)).rms.toFixed(4);
    return out;
  });
  check('a saw note sounds, finite and below clipping', off.open.rms > .01 && off.open.finite && off.open.peak < 1, [off.open.rms, off.open.peak]);
  check('cutoff 300 Hz darkens it', off.dark.bright < off.open.bright * .5, [off.open.bright, off.dark.bright]);
  const sw = off.lfo.wins.slice(1, 18), spread = Math.max(...sw) / Math.max(1e-9, Math.min(...sw));
  check('LFO1 routed to cutoff makes the level swing', spread > 1.5, +spread.toFixed(2));
  check('7-voice unison with stereo spread is wide', off.uni.side > off.open.side * 5 + 1e-4, [off.open.side, off.uni.side]);
  check('every table sounds', Object.values(off.tables).every(r => r > .005), off.tables);
  check('every filter type sounds and stays finite', Object.entries(off.filters).every(([k, r]) => r > .001 && r < 1), off.filters);
  const g = off.gate.wins;
  check('the gate closes the note: silent after gate plus release', g[0] > .01 && g[g.length - 1] < 1e-4, [g[0], g[g.length - 1]]);
  check('three oscillators with unison, sub and noise, four notes: no clipping (random unison phases peak near 1)', off.all.peak < 1.1 && off.all.rms > .02, [off.all.rms, off.all.peak]);
  const tail = r => r.wins.slice(-3).reduce((a, b) => a + b, 0);
  check('delay and chorus leave a tail after the note; the dry patch does not', tail(off.fx.delay) > 1e-3 && tail(off.fx.none) < 1e-4, [tail(off.fx.delay), tail(off.fx.none)]);
  check('every FX module sounds, finite and below clipping', Object.values(off.fx).every(r => r.finite && r.rms > .002 && r.peak < 1.2), Object.fromEntries(Object.entries(off.fx).map(([k, r]) => [k, +r.rms.toFixed(4)])));
  for (const k of ['hyper', 'flanger', 'phaser', 'chorus']) check(`${k} changes the sound`, Math.abs(off.fx[k].bright - off.fx.none.bright) > .01 || Math.abs(off.fx[k].side - off.fx.none.side) > 1e-3, [+off.fx[k].bright.toFixed(4), +off.fx[k].side.toFixed(4)]);
  check('the rack runs in the patch\'s order: dist→eq differs from eq→dist', Math.abs(off.order[0].bright - off.order[1].bright) > .01, off.order.map(r => +r.bright.toFixed(3)));
  check('distortion brightens', off.fx.dist.bright > off.fx.none.bright * 1.2, [off.fx.none.bright, off.fx.dist.bright]);
  check('crush is harsher still', off.fx.crush.bright > off.fx.none.bright * 1.2, off.fx.crush.bright);
  // at 120 bpm (no tempo sent) a 1/4 delay repeats 0.5 s after the note: windows are 50 ms
  const ds = off.delaySync.wins, echo = ds.slice(9, 13).reduce((a, b) => Math.max(a, b), 0), gap = ds.slice(4, 9).reduce((a, b) => Math.max(a, b), 0);
  check('delay synced to 1/4 at 120 bpm echoes at 0.5 s, silent between', echo > .01 && gap < echo * .2, [gap, echo]);
  check('a macro routed to distortion drive drives it', off.macroFx.bright > off.macroFxOff.bright * 1.2, [off.macroFxOff.bright, off.macroFx.bright]);
  const ls = off.lfoSync.wins; const hiW = ls.map((x, i) => x > (Math.max(...ls.slice(2)) + Math.min(...ls.slice(2))) / 2 ? 1 : 0);
  const edges = hiW.slice(2).reduce((a, x, i, A) => a + (i && x !== A[i - 1] ? 1 : 0), 0);
  check('a square LFO synced to 1/4 at 120 bpm switches every 0.25 s (about 7 edges in 1.9 s)', edges >= 6 && edges <= 9, edges);
  // two notes, A3 then A4: a sine's zero crossings give the pitch
  check('osc octave and semi knobs move its pitch: A3, +1 oct, -1 oct +7 semi; the synth\'s own +1 oct -12 semi with A +7', off.oscPitch.every((h, i) => Math.abs(h - [220, 440, 165, 330][i]) <= 2), off.oscPitch);
  check('poly: the overlapping notes both sound, louder than mono\'s one', off.voicing.polyRms > off.voicing.monoRms * 1.2, [off.voicing.polyRms, off.voicing.monoRms].map(x => +x.toFixed(4)));
  check('mono: one voice, landing on A4', Math.abs(off.voicing.monoHz - 440) < 3, +off.voicing.monoHz.toFixed(1));
  check('mono with glide: just after the second note it is still on its way up', off.voicing.monoEarly > 225 && off.voicing.monoEarly < 420, +off.voicing.monoEarly.toFixed(1));
  const sw0 = off.aux0.wins.slice(1, 18), sw1 = off.aux1.wins.slice(1, 18), swing = w => Math.max(...w) / Math.max(1e-9, Math.min(...w));
  check('aux source: macro 1 at 0 holds the LFO route shut, at 1 lets it swing', swing(sw1) > swing(sw0) * 1.5, [+swing(sw0).toFixed(2), +swing(sw1).toFixed(2)]);
  check('unison blend changes the sound, at the same loudness', Math.abs(off.blend[0].bright - off.blend[1].bright) > .01 || Math.abs(off.blend[0].side - off.blend[1].side) > 1e-3, off.blend.map(r => [+r.rms.toFixed(3), +r.side.toFixed(4)]));
  is('route curves: straight at 0, +1 bows up, -1 bows down, ends fixed', off.curve.map(x => +x.toFixed(3)), [.5, .938, .063, 0, 1]);
  const wn = off.warps.none, wv = Object.entries(off.warps).filter(([k]) => k !== 'none' && !k.startsWith('off'));
  check('every warp mode, in either slot, sounds and changes the saw', wv.every(([, r]) => r[2] && r[0] > .003 && (Math.abs(r[1] - wn[1]) > .002 || Math.abs(r[0] - wn[0]) > .002)), Object.fromEntries(wv));
  check('warp off leaves it as it was', off.warps.off1[1] === wn[1] && off.warps.off2[1] === wn[1], [off.warps.off1, off.warps.off2, wn]);
  check('every factory preset sounds', Object.values(off.presets).every(r => r > .003), off.presets);

  console.log('WAVE EDITOR');
  await probe(() => { const j = __cogs.serialize(); j.cogs = []; j.arms = []; j.belts = []; j.strings = []; j.readers = [];
    j.pads = [{ id: 1, x: 0, y: 0, r: 30, voice: { kind: 'wave', note: 62, level: .7, velSense: .7 } }]; j.view = { x: 0, y: 0 }; __cogs.deserialize(j); });
  await p.click('[data-rail="select"]'); await wait(80);
  const at = await probe(() => { const s = __cogs.toScreen({ x: 0, y: 0 }); return [s.x, s.y]; });
  await p.mouse.click(...at); await wait(200);
  const voice = () => probe(() => __cogs.serialize().pads[0].voice);
  check('a pad with a wave voice selected: the editor shows', await probe(() => !!document.querySelector('#inspector .wave .knob')), await probe(() => __cogs.sel));
  is('the saved voice was filled out to a full patch, keeping its note', await probe(() => { const v = __cogs.serialize().pads[0].voice; return [v.note, v.osc.length, v.lfo.length, v.macro.length, v.env.length]; }), [62, 3, 4, 8, 4]);
  const knobAt = async path => probe(pa => { const k = document.querySelector(`#inspector .knob[data-path="${pa}"] canvas`); k.scrollIntoView({ block: 'center' }); const r = k.getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2]; }, path);
  const dragKnob = async (path, dy) => { const [x, y] = await knobAt(path); await p.mouse.move(x, y); await p.mouse.down(); await p.mouse.move(x, y - dy, { steps: 8 }); await p.mouse.up(); await settle(); };
  const u0 = await probe(() => __cogs.undos);
  await dragKnob('osc1.res', 60);
  const res = (await voice()).osc[0].res;
  check('drag the A res knob up 60 px: res rises by 0.4', Math.abs(res - .5) < .03, res);
  is('one knob drag is one undo step', await probe(() => __cogs.undos) - u0, 1);
  await probe(() => __cogs.undo()); await wait(100);
  is('undo puts it back', (await voice()).osc[0].res, .1);
  await p.click('#inspector .wave-src button[data-src="lfo1"]');
  is('tap LFO1: it is armed', await probe(() => document.querySelector('#inspector .wave-src button[data-src="lfo1"]').ariaPressed), 'true');
  await dragKnob('osc1.pos', 45);
  const m1 = (await voice()).mods;
  check('armed, dragging wt pos draws a route LFO1 -> A wt pos', m1.length === 1 && m1[0].src === 'lfo1' && m1[0].dst === 'osc1.pos' && Math.abs(m1[0].amt - .3) < .03, m1);
  is('the knob value is untouched by routing (init is a saw, wt pos 2/3)', (await voice()).osc[0].pos, 2 / 3);
  await p.click('#inspector .wave-src button[data-src="lfo1"]');
  // drag a chip onto a knob: Serum's drag and drop
  const tgt = await knobAt('osc1.cut');
  const chip2 = await probe(() => { const r = document.querySelector('#inspector .wave-src button[data-src="env2"]').getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2]; });
  await p.mouse.move(...chip2); await p.mouse.down(); await p.mouse.move(...tgt, { steps: 10 }); await p.mouse.up(); await settle();
  const m2 = (await voice()).mods;
  check('the sources stay in view while the knobs scroll; drag the ENV2 chip onto cutoff: a route at half depth', m2.length === 2 && m2[1].src === 'env2' && m2[1].dst === 'osc1.cut' && m2[1].amt === .5, m2);
  await shot('wave-01-osc');
  await p.click('#inspector .wave-tabs button[data-tab="FX"]'); await wait(100);
  await p.click('#inspector .wave-toggle:text("delay")'); await settle();
  await p.selectOption('#inspector .wave-choice select[aria-label="sync"]', '1/4'); await settle();
  is('FX tab: turn the delay on and sync it to 1/4', [(await voice()).fx.delay.on, (await voice()).fx.delay.sync], [true, '1/4']);
  const order0 = (await voice()).fxOrder;
  await p.click('#inspector button[aria-label="move delay up"]'); await settle();
  const order1 = (await voice()).fxOrder, i0 = order0.indexOf('delay');
  is('↑ on the delay moves it one place up the rack', [order1.indexOf('delay'), order1[i0]], [i0 - 1, order0[i0 - 1]]);
  await dragKnob('delay.fb', 30); is('drag the feedback knob', (await voice()).fx.delay.fb > .4, true);
  await shot('wave-04-fx');
  await p.click('#inspector .wave-tabs button[data-tab="MATRIX"]'); await wait(100);
  is('the matrix lists both routes', await probe(() => document.querySelectorAll('#inspector .wave-route').length), 2);
  await shot('wave-02-matrix');
  await p.click('#inspector .wave-route button[aria-label="delete route"]'); await settle();
  is('delete one in the matrix: one left', (await voice()).mods.map(m => m.src), ['env2']);
  await p.click('#inspector .wave-tabs button[data-tab="PATCH"]'); await wait(100);
  await p.selectOption('#inspector .wave-choice select[aria-label="factory"]', 'wobble'); await settle();
  const wob = await voice();
  is('load the factory wobble: its routes and macro name, the pad keeps its note', [wob.name, wob.mods.length, wob.macroNames[0], wob.note], ['wobble', 2, 'wobble', 62]);
  await p.click('#inspector button:text("save to library")'); await wait(100);
  is('save to library: stored by name', await probe(() => Object.keys(JSON.parse(localStorage.getItem(Object.keys(localStorage).find(k => k.endsWith('.patches')))))), ['wobble']);
  await p.click('#inspector button:text("init")'); await settle();
  is('init clears it', [(await voice()).name, (await voice()).mods.length], ['init', 0]);
  await p.click('#inspector .wave-lib button:text("wobble")'); await settle();
  is('load it back from the library', [(await voice()).name, (await voice()).mods.length], ['wobble', 2]);
  await p.click('#inspector button:text("export")'); await wait(100);
  const text = await p.inputValue('#inspector .wave textarea');
  const exported = JSON.parse(text);
  is('export writes the patch as JSON, without the part\'s pitch', [exported.cogsWave, exported.name, 'note' in exported, exported.osc.length], [1, 'wobble', false, 3]);
  await p.fill('#inspector .wave textarea', JSON.stringify({ ...exported, name: 'edited', vol: .5 }));
  await p.click('#inspector button:text("import text")'); await settle();
  is('import text: the edited patch loads', [(await voice()).name, (await voice()).vol, (await voice()).mods.length], ['edited', .5, 2]);
  await p.fill('#inspector .wave textarea', '{"nope":1}');
  await p.click('#inspector button:text("import text")'); await wait(100);
  check('a bad import is refused and said so', /import failed/.test(await probe(() => document.querySelector('#inspector .wave p.wave-hint:last-child').textContent)) && (await voice()).name === 'edited', await probe(() => document.querySelector('#inspector .wave p.wave-hint:last-child').textContent));
  const again = await probe(() => { const j = __cogs.serialize(); __cogs.deserialize(JSON.parse(JSON.stringify(j))); return __cogs.serialize().pads[0].voice; });
  is('the patch survives a save and load of the machine', JSON.stringify(again) === JSON.stringify(await voice()), true);
  await shot('wave-03-patch');
  await p.mouse.click(...at); await wait(200);
  await p.click('#inspector .wave-play'); await wait(400);
  check('play sounds it in the live engine', (await probe(() => __cogs.engine().wave)) > 0, await probe(() => __cogs.engine().wave));
  // the synth transposes itself: its octave and semi knobs (SUB tab, voice) move every note into it, keyboard or part
  await p.click('#inspector .wave-tabs button[data-tab="SUB"]'); await wait(100);
  await dragKnob('tune.oct', 19); await dragKnob('tune.semi', 12);
  const tv = await voice();
  check('the synth\'s octave and semi knobs turn: +1 octave, a few semitones up', tv.tune.oct === 1 && tv.tune.semi > 0, tv.tune);
  const kb = await p.$('#inspector .wave-keys'); await kb.scrollIntoViewIfNeeded(); const kr = await kb.boundingBox();
  await p.mouse.move(kr.x + kr.width / 28, kr.y + kr.height * .85); await p.mouse.down(); await wait(80); await p.mouse.up(); await wait(80);
  is('the keyboard sends its key as is (the synth transposes it) and its label says by how much', [await probe(() => __cogs.engine().waveLast), await probe(() => document.querySelector('#inspector .wave-oct').textContent)], [60, 'C4 +' + (12 * tv.tune.oct + tv.tune.semi)]);
  is('no octave or semitones rows for the part: they are the synth\'s', await probe(() => ['octave', 'semitones'].map(k => !!document.querySelector(`#inspector .row[data-k="${k}"]`))), [false, false]);
  const mig = await probe(() => { const j = __cogs.serialize(); j.pads[0].voice = { ...j.pads[0].voice, octave: -1, semi: 3, tune: { oct: 0, semi: 0 } }; __cogs.deserialize(j); return __cogs.serialize().pads[0].voice; });
  is('a part saved with its own octave and semitones: they fold into the synth\'s', [mig.octave, mig.semi, mig.tune.oct, mig.tune.semi], [0, 0, -1, 3]);
  await p.mouse.click(...at); await wait(200);
  // pin knobs to the part's dial: arm ◎ DIAL, tap knobs; tap again to take one off
  await p.click('#inspector .wave-pin'); await p.click('#inspector .wave-tabs button[data-tab="A"]'); await wait(100);
  for (const k of ['osc1.cut', 'osc1.res', 'osc1.cut']) { const [x, y] = await knobAt(k); await p.mouse.click(x, y); await settle(); }
  const [cx, cy] = await knobAt('osc1.res'); await p.mouse.click(cx, cy); await settle();
  is('◎ DIAL armed: a tap pins a knob, a second takes it off; the knob turns not at all', [(await voice()).dial, (await voice()).osc[0].res === tv.osc[0].res], [[], true]);
  { const [x, y] = await knobAt('osc1.cut'); await p.mouse.click(x, y); await settle(); }
  is('pinned: saved with the patch, marked on the knob', [(await voice()).dial, await probe(() => document.querySelector('#inspector .knob[data-path="osc1.cut"]').classList.contains('pinned'))], [['osc1.cut'], true]);
  await p.click('#inspector .wave-pin');
  check('page errors', errs.length === 0, errs.slice(0, 3));
  console.log(pass ? 'COGS WAVE: PASS' : 'COGS WAVE: FAIL');
  await b.close(); process.exit(pass ? 0 : 1);
})().catch(e => { console.error(e); process.exit(1); });
