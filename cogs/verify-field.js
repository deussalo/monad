// The FX field: placing it, its send law and diffuseness, every kind of voice reaching its effect (drum graph, modal
// strike, synth note), the dry it takes, where in it things sound bending its parameters, its sheet, saving.
const { chromium } = require(process.env.COGS_PW || '/srv/rig/rig/webapp/node_modules/.pnpm/playwright-core@1.64.0/node_modules/playwright-core');
const path = require('path');
const FILE = process.argv[2] || ('file://' + path.join(__dirname, 'desk.html'));
(async () => {
  const b = await chromium.launch({ executablePath: (process.env.MONAD_CHROME || '/srv/rig/.cache/ms-playwright/chromium-1248/chrome-linux64/chrome') });
  let pass = true;
  const check = (label, ok, got) => { if (!ok) pass = false; console.log(('  ' + label).padEnd(84), JSON.stringify(got), ok ? '✓' : '✗'); };
  const is = (label, got, want) => check(label, JSON.stringify(got) === JSON.stringify(want), got);
  const wait = ms => new Promise(r => setTimeout(r, ms));
  const p = await (await b.newContext({ viewport: { width: 1440, height: 900 } })).newPage(), errs = [];
  p.on('pageerror', e => errs.push(e.message));
  const probe = (fn, arg) => p.evaluate(fn, arg);
  const scr = ([x, y]) => probe(([x, y]) => { const s = __cogs.toScreen({ x, y }); return [s.x, s.y]; }, [x, y]);
  await p.goto(FILE); await wait(500);
  await p.mouse.click(40, 860); await wait(300);
  await p.waitForFunction(() => __cogs.engine().state === 'ready' && __cogs.engine().ac === 'running', null, { timeout: 5000 });
  await probe(() => { setPaused(true); const j = __cogs.serialize();
    Object.assign(j, { cogs: [], arms: [], pads: [], readers: [], belts: [], strings: [], cams: [], pits: [], probes: [], fields: [], view: { x: 0, y: 0 } });
    j.S.zoom = 1; __cogs.deserialize(j); __cogs.commit(); });
  await wait(100);

  console.log('PLACING');
  await p.keyboard.press('f'); await p.mouse.click(...await scr([0, 0])); await wait(150); await p.keyboard.press('Escape');
  const made = await probe(() => ({ n: W.fields.length, sel: __cogs.sel?.type, x: Math.round(W.fields[0]?.x), y: Math.round(W.fields[0]?.y), mode: W.fields[0]?.mode, slot: W.fields[0]?.slot,
    rows: [...document.querySelectorAll('#inspector .row, #inspector .ro')].map(r => r.dataset.k) }));
  check('key f, then a click: a delay field there, selected, in slot 0', made.n === 1 && made.sel === 'field' && made.x === 0 && made.y === 0 && made.mode === 'delay' && made.slot === 0, made);
  is('its sheet: effect, the delay\'s knobs, then diffuse, return, takes dry, radius, now', made.rows, ['effect', 'time', 'free time s', 'feedback', 'tone', 'ping-pong', 'diffuse', 'return', 'takes dry', 'radius', 'now']);

  console.log('THE SEND');
  const law = await probe(() => { const f = W.fields[0], at = (u) => +fieldSend(f, { x: f.r * u, y: 0 }).toFixed(3), out = {};
    for (const d of [0, .5, 1]) { f.diffuse = d; out[d] = [0, .3, .45, .55, .75, .999, 1.01].map(at); } f.diffuse = .5; return out; });
  is('diffuse 0: a hard edge, all of it inside, none outside', law[0], [1, 1, 1, 1, 1, 1, 0]);
  is('diffuse 0.5: full in the inner half, fading to nothing at the rim', law['0.5'], [1, 1, 1, 0.972, 0.5, 0, 0]);
  check('diffuse 1: full only at the centre, falling all the way out', law[1][0] === 1 && law[1][1] < 1 && law[1][1] > law[1][4] && law[1][5] < .01 && law[1][6] === 0, law[1]);

  // a tap on the fields return: its energy over a window
  await probe(() => { const a = ac.createAnalyser(); a.fftSize = 2048; fieldsNode.connect(a); window.__fa = a; window.__fbuf = new Float32Array(2048);
    window.__energy = ms => new Promise(r => { let e = 0; const t0 = performance.now(); const tick = () => { __fa.getFloatTimeDomainData(__fbuf); for (const x of __fbuf) e += x * x; if (performance.now() - t0 < ms) setTimeout(tick, 20); else r(e); }; tick(); });
    window.__play = (kind, at) => { const v = kind === 'wave' ? makeVoice('wave') : makeVoice(kind); const when = ac.currentTime + .05;
      withFields(at, when, simT, () => playVoice(v, 60, when, .8, 0, 9000 + Math.floor(Math.random() * 1000), 1, kind === 'synth' || kind === 'wave' ? .3 : 0)); }; });
  await wait(300);
  // outside first, while the delay is silent; then inside
  const outE = {}; for (const kind of ['snare', 'synth', 'wave']) outE[kind] = await probe(async k => { await __energy(200); __play(k, { x: 400, y: 0 }); return await __energy(700); }, kind);
  for (const kind of ['snare', 'synth', 'wave']) {
    const inside = await probe(async k => { await __energy(200); __play(k, { x: 10, y: 0 }); return await __energy(700); }, kind), outside = outE[kind];
    check(`a ${kind === 'synth' ? 'modal' : kind === 'wave' ? 'synth' : 'snare'} struck inside reaches the delay; outside, nothing`, inside > 1e-3 && outside < 1e-6, [+inside.toFixed(4), +outside.toExponential(2)]);
  }
  const msg = await probe(() => { const sent = []; const port = waveNode.port, post = port.postMessage.bind(port); port.postMessage = (m, t) => { if (m.type === 'note') sent.push(m); return post(m, t); };
    __play('wave', { x: 0, y: 0 }); port.postMessage = post; const m = sent[0]; return { fx: m.fx, dry: m.dry }; });
  is('the synth note carries its send (slot 0, full at the centre) and its dry', msg, { fx: [0, 1], dry: 1 });

  console.log('TAKING THE DRY');
  const dry = await probe(() => { const f = W.fields[0]; f.insert = 1; const a = fieldSendsAt({ x: 0, y: 0 }), b = fieldSendsAt({ x: f.r * .75, y: 0 }); f.insert = 0; return [a.dry, b.dry, b.fx[1]]; });
  is('takes dry 1: at the centre none of the dry is left; halfway out, half', dry, [0, .5, .5]);
  await probe(() => { setFieldMode(W.fields[0], 'dist'); buildInspector(); }); await wait(100);
  const dist = await probe(() => ({ insert: W.fields[0].insert, rows: [...document.querySelectorAll('#inspector .row')].map(r => r.dataset.k).slice(0, 4) }));
  check('distortion: it takes most of the dry by default; its knobs are kind, drive, tone', dist.insert === .8 && JSON.stringify(dist.rows) === JSON.stringify(['effect', 'kind', 'drive', 'tone']), dist);
  await wait(150);
  const dE = await probe(async () => { await __energy(300); __play('snare', { x: 0, y: 0 }); return await __energy(500); });
  check('a snare in the distortion is heard through it', dE > 1e-3, +dE.toFixed(4));

  console.log('WHERE IT SOUNDS BENDS IT');
  await probe(() => { setFieldMode(W.fields[0], 'delay'); W.fields[0].diffuse = 1; W.fields[0].insert = 0; }); await wait(150);
  const qc = await probe(async () => { for (let i = 0; i < 4; i++) { __play('snare', { x: 0, y: 0 }); await new Promise(r => setTimeout(r, 120)); } await new Promise(r => setTimeout(r, 500)); return fieldsStats.q[0]; });
  const qe = await probe(async () => { for (let i = 0; i < 4; i++) { __play('snare', { x: 120, y: 0 }); await new Promise(r => setTimeout(r, 120)); } await new Promise(r => setTimeout(r, 500)); return fieldsStats.q[0]; });
  check('strikes at the centre move its parameters to the centre; near the rim, toward the rim', qc > .85 && qe < .4, [qc, qe]);
  const k = await probe(() => fieldsStats.k[0]);
  check('fully diffuse, its bend follows them (negative toward the rim)', k < -.2, k);

  console.log('MANY, SAVED');
  await probe(() => { for (let i = 0; i < 9; i++) W.fields.push(newField(W.next++, { x: 500 + i * 10, y: 0 })); postFields(); });
  const slots = await probe(() => W.fields.map(f => f.slot));
  is('fields past eight are silent (no slot)', slots, [0, 1, 2, 3, 4, 5, 6, 7, -1, -1]);
  await probe(() => { W.fields.splice(1); W.fields[0].mode = 'reverb'; W.fields[0].r = 222; W.fields[0].decay = 4; __cogs.deserialize(JSON.parse(JSON.stringify(__cogs.serialize()))); });
  const back = await probe(() => ({ n: W.fields.length, mode: W.fields[0].mode, r: W.fields[0].r, decay: W.fields[0].decay, diffuse: W.fields[0].diffuse }));
  is('it survives a save and load', back, { n: 1, mode: 'reverb', r: 222, decay: 4, diffuse: 1 });
  await wait(200);
  const rE = await probe(async () => { await __energy(300); __play('snare', { x: 0, y: 0 }); return await __energy(600); });
  check('a snare in a reverb field is heard through it', rE > 1e-3, +rE.toFixed(4));
  await probe(() => { select('field', W.fields[0]); }); await wait(50);
  const h = await probe(() => { const q = fieldHandle(W.fields[0]); const s = __cogs.toScreen(q); return [s.x, s.y]; });
  await p.mouse.move(...h); await p.mouse.down(); await p.mouse.move(h[0] + 40, h[1] + 40, { steps: 4 }); await p.mouse.up(); await wait(100);
  const r2 = await probe(() => Math.round(W.fields[0].r));
  check('the rim handle pulls its radius', Math.abs(r2 - Math.round(222 + 40 * Math.SQRT2)) <= 3, r2);
  const c = await scr([0, 0]);
  await p.mouse.move(...c); await p.mouse.down(); await p.mouse.move(c[0] + 50, c[1] + 20, { steps: 4 }); await p.mouse.up(); await wait(100);
  is('its centre drags it', await probe(() => [Math.round(W.fields[0].x), Math.round(W.fields[0].y)]), [50, 20]);
  const tapped = await probe(() => { const s = __cogs.toScreen({ x: 50 + 120, y: 20 }); return [s.x, s.y]; });
  await p.mouse.click(...tapped); await wait(100);
  is('a click inside it, away from the centre, does not pick it (it has no edge to grab)', await probe(() => __cogs.sel?.type ?? null), null);
  is('page errors', errs, []);
  console.log(pass ? 'COGS FIELD: PASS' : 'COGS FIELD: FAIL');
  await b.close(); process.exit(pass ? 0 : 1);
})();
