// Drives desk.html with a mouse and keyboard in a 1440x900 browser: toolbar, parts, inspector controls, roll, undo, wheel zoom.
const { chromium } = require(process.env.COGS_PW || '/srv/rig/rig/webapp/node_modules/.pnpm/playwright-core@1.64.0/node_modules/playwright-core');
const path = require('path');
const FILE = process.argv[2] || ('file://' + path.join(__dirname, 'desk.html'));
const SHOTS = process.env.COGS_SHOTS || '';
(async () => {
  const b = await chromium.launch({ executablePath: (process.env.MONAD_CHROME || '/srv/rig/.cache/ms-playwright/chromium-1248/chrome-linux64/chrome') });
  let pass = true;
  const check = (label, ok, got) => { if (!ok) pass = false; console.log(('  ' + label).padEnd(64), JSON.stringify(got), ok ? '✓' : '✗'); };
  const is = (label, got, want) => check(label, JSON.stringify(got) === JSON.stringify(want), got);
  const near = (a, z, tol) => Math.abs(a - z) <= tol;
  const wait = ms => new Promise(r => setTimeout(r, ms));
  const ctx = await b.newContext({ viewport: { width: 1440, height: 900 }, permissions: ['clipboard-read', 'clipboard-write'] });
  const p = await ctx.newPage(), errs = [];
  p.on('pageerror', e => errs.push(e.message));
  const probe = (fn, arg) => p.evaluate(fn, arg);
  const scr = ([x, y]) => probe(([x, y]) => { const s = __cogs.toScreen({ x, y }); return [s.x, s.y]; }, [x, y]);
  // An edit becomes an undo step when the pointer lifts or the edits go quiet.
  const settle = async () => { await p.waitForFunction(() => !__cogs.pending); await wait(50); };
  const click = async w => { await p.mouse.click(...await scr(w)); await wait(120); await settle(); };
  const drag = async (a, z) => { await p.mouse.move(...await scr(a)); await p.mouse.down(); await p.mouse.move(...await scr(z), { steps: 12 }); await p.mouse.up(); await settle(); };
  const adv = () => probe(() => { document.querySelector('#panel details').open = true; });
  const rail = async id => { await p.click(`[data-rail="${id}"]`); await wait(80); await adv(); };
  const key = () => probe(() => __cogs.worldKey());
  const undos = () => probe(() => __cogs.undos);
  const shot = async name => { if (SHOTS) await p.screenshot({ path: path.join(SHOTS, name + '.png') }); };

  await p.goto(FILE); await wait(500);
  await p.mouse.click(40, 860); await wait(300); await adv();
  await p.waitForFunction(() => __cogs.engine().state === 'ready' && __cogs.engine().ac === 'running', null, { timeout: 5000 });
  const smooth = await probe(() => new Promise(r => { const s = []; const f = ts => { s.push([ts, __cogs.clock.view * 1000]); if (s.length < 40) requestAnimationFrame(f); else {
    const k = s.slice(1).map(([t, v], i) => (v - s[i][1]) / (t - s[i][0])).filter(Number.isFinite); r(Math.max(...k.map(x => Math.abs(x - 1)))); } }; requestAnimationFrame(f); }));
  check('drawn motion advances with the frame clock (worst frame within 5%)', smooth < .05, +smooth.toFixed(4));
  await shot('desk-01-demo');

  console.log('ENGINE');
  const steal = await probe(async () => {
    const url = 'data:text/javascript;charset=utf-8,' + encodeURIComponent(document.querySelector('#engine').textContent);
    const strike = (at, k, amp) => [at, k, amp, .0008, 0, 0, 3, 220, 330, 440, 1, .5, .3, 2, 2, 2];
    const render = async (voices, strikes) => {
      const ac = new OfflineAudioContext(2, 128 * 40, 48000);
      await ac.audioWorklet.addModule(url);
      const n = new AudioWorkletNode(ac, 'cogs-engine', { outputChannelCount: [2], processorOptions: { voices, lines: 16, space: { ...SPACE, wet: 0 } } });
      n.connect(ac.destination);
      ac.suspend(256 / 48000).then(async () => { n.port.postMessage({ type: 'strikes', d: new Float64Array(strikes.flat()) }); await new Promise(r => setTimeout(r, 100)); ac.resume(); });
      return (await ac.startRendering()).getChannelData(0);
    };
    const at = 128 * 10 + 20, alone = await render(64, [strike(at, 2, .5)]), stolen = await render(1, [strike(512, 1, .002), strike(at, 2, .5)]);
    let gap = 0; for (let i = 128 * 11; i < alone.length; i++) gap = Math.max(gap, Math.abs(alone[i] - stolen[i]));
    return gap;
  });
  is('a strike that steals the only voice sounds at its own sample, as if alone', steal, 0);

  console.log('TOOLBAR');
  const bar = () => probe(() => ({ open: __cogs.open, drawer: document.querySelector('#drawer').offsetWidth, stageR: Math.round(__cogs.stage().r) }));
  const opened = await bar();
  check('opens with the panel out, the stage ends at the toolbar', opened.open && opened.drawer === 360 && opened.stageR === 1440 - 72 - 360 - 1, opened);
  await rail('panel'); await wait(300);
  const closed = await bar();
  check('the labelled toggle slides the panel in, leaving the rail', !closed.open && closed.drawer === 0 && closed.stageR === 1440 - 72 - 1, closed);
  await shot('desk-02-closed');
  await rail('panel'); await wait(300);
  is('and out again', (await bar()).drawer, 360);
  await p.keyboard.press('?'); await wait(300); const byKey = (await bar()).drawer; await p.keyboard.press('?'); await wait(300);
  is('? hides and shows it', [byKey, (await bar()).drawer], [0, 360]);

  console.log('SPEED FIELD UNDER TEMPO LOCK');
  const c1 = await probe(() => { const s = __cogs.toScreen(__cogs.W.cogs[0]); return [s.x + 5, s.y + 5]; });
  await p.mouse.click(...c1); await settle();
  if (!(await probe(() => __cogs.open))) await rail('panel');
  await p.fill('#inspector [data-k="rpm"] input[type=number]', '40'); await settle();
  is('type 40 rpm on a cog of the tempo-locked default machine: it turns at 40', await probe(() => +(__cogs.W.cogs[0].w * 60 / (2 * Math.PI)).toFixed(6)), 40);

  const c2 = await probe(() => { const s = __cogs.toScreen(__cogs.W.cogs[1]); return [s.x + 5, s.y + 5]; });
  await p.mouse.click(...c2); await settle();
  await p.fill('#inspector [data-k="rpm"] input[type=number]', '30'); await settle();
  is('type 30 rpm on the cog it drives: that cog takes over, nothing jams', await probe(() => __cogs.W.cogs.slice(0, 2).map(c => [+(c.w * 60 / (2 * Math.PI)).toFixed(6), c.jam])), [[-30, false], [30, false]]);

  console.log('PARTS FROM THE TOOLBAR');
  await p.click('button[data-act=clear]');
  await p.fill('#panel [data-k="zoom"] input[type=number]', '1'); await p.fill('#panel [data-k="bpm lock"] input[type=number]', '0'); await settle();
  const keys = [await key()], steps = [await undos()];
  const mark = async () => { keys.push(await key()); steps.push(await undos()); };
  await rail('motor'); is('a part button arms that part', await probe(() => __cogs.armed), 'motor');
  await click([-100, 0]); await mark();
  await rail('cog'); await click([100, 0]); await mark();
  await rail('pad'); await click([0, -100]); await mark();
  await rail('select');
  const placed = await probe(() => [__cogs.armed, __cogs.W.cogs.map(c => [c.x, c.y]), __cogs.W.pads.map(o => [o.x, o.y])]);
  is('a motor, a cog and a pad land where clicked; select disarms', placed, [null, [[-100, 0], [100, 0]], [[0, -100]]]);
  await drag([-100, 0], [-100, 40]); await mark();
  is('the cog dragged to y 40', await probe(() => __cogs.W.cogs[0].y), 40);

  console.log('INSPECTOR');
  await click([-110, 55]);
  is('click a cog: the inspector shows it', await probe(() => __cogs.sel), { type: 'cog', id: await probe(() => __cogs.W.cogs[0].id) });
  await p.fill('#inspector [data-k="rpm"] input[type=number]', '60'); await settle(); await mark();
  is('typing 60 into rpm turns it at 60 rpm', await probe(() => +(__cogs.W.cogs[0].w * 60 / (2 * Math.PI)).toFixed(6)), 60);
  const teeth = () => probe(() => __cogs.W.cogs[0].n);
  const range = await probe(() => { const r = document.querySelector('#inspector [data-k="teeth"] input[type=range]').getBoundingClientRect(); return { x: r.left, y: r.top + r.height / 2, w: r.width }; });
  const thumb = range.x + (12 - 6) / (240 - 6) * (range.w - 9) + 4.5;
  await p.mouse.move(thumb, range.y); await p.mouse.down(); await p.mouse.move(thumb + 60, range.y, { steps: 10 }); await p.mouse.up(); await settle(); await mark();
  const dragged = await teeth();
  check('drag the teeth slider right: more teeth', dragged > 12, [12, dragged]);
  const num = await probe(() => { const r = document.querySelector('#inspector [data-k="teeth"] input[type=number]').getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2]; });
  await p.mouse.move(...num); for (let i = 0; i < 3; i++) await p.mouse.wheel(0, -100);
  await settle(); await mark();
  is('three wheel notches up: +3 x 2 (1% of 6..240)', await teeth(), dragged + 6);
  await p.keyboard.down('Shift'); await p.mouse.wheel(0, 100); await p.keyboard.up('Shift'); await settle(); await mark();
  is('shift + wheel down: one tooth', await teeth(), dragged + 5);
  await p.fill('#inspector [data-k="teeth"] input[type=number]', '24'); await settle(); await mark();
  await click([0, -100]);
  const tune = () => probe(() => __cogs.W.pads[0].voice.tune);
  const tuneAt = await probe(() => { const r = document.querySelector('#inspector [data-k="tune Hz"] input[type=range]').getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2]; });
  await p.mouse.move(...tuneAt); await p.mouse.wheel(0, -100); await p.mouse.wheel(0, -100); await settle(); await mark();
  const tuned = await tune();
  check('wheel over the pad tune slider raises it', tuned > 52, tuned);
  await p.dblclick('#inspector [data-k="tune Hz"] span'); await settle(); await mark();
  is('double-click the row: back to the kick default, 52', await tune(), 52);

  console.log('UNDO AND REDO');
  is('every action above is exactly one step', steps.slice(1).map((s, i) => s - steps[i]), keys.slice(1).map(() => 1));
  check('each step changed the world', keys.slice(1).every((k, i) => k !== keys[i]), keys.length);
  const phase = () => probe(() => { const c = __cogs.W.cogs[0]; return { th: c.th, w: c.w, t: __cogs.simT }; });
  const ph0 = await phase(); await p.keyboard.press('Control+z'); const ph1 = await phase();
  const drift = Math.abs(((ph1.th - ph0.th - ph0.w * (ph1.t - ph0.t)) % (2 * Math.PI) + 3 * Math.PI) % (2 * Math.PI) - Math.PI);
  check('undo keeps the train spinning in phase', drift < 1e-6 && ph1.w === ph0.w, drift);
  is('ctrl+z: back one step', await key() === keys[keys.length - 2], true);
  const back = [];
  for (let i = keys.length - 3; i >= 0; i--) { await p.keyboard.press('Control+z'); back.push(await key() === keys[i]); }
  is('ctrl+z walks back through every step to the start', back, back.map(() => true));
  is('the start: no parts', await probe(() => __cogs.W.cogs.length + __cogs.W.pads.length), 0);
  const fwd = [];
  for (let i = 1; i < keys.length; i++) { if (i % 3 === 0) await p.keyboard.press('Control+y'); else if (i % 3 === 1) await p.keyboard.press('Control+Shift+z'); else await rail('redo'); fwd.push(await key() === keys[i]); }
  is('ctrl+shift+z, ctrl+y and the redo button walk forward', fwd, fwd.map(() => true));
  await wait(100); is('nothing left to redo: the button is disabled', await probe(() => document.querySelector('[data-rail=redo]').disabled), true);
  await rail('undo'); is('the undo button steps back', await key() === keys[keys.length - 2], true);
  await wait(100); is('and redo is available again', await probe(() => document.querySelector('[data-rail=redo]').disabled), false);
  await click([0, -100]); await p.fill('#inspector [data-k="tune Hz"] input[type=number]', '70'); await settle();
  is('a new action clears redo', await probe(() => __cogs.redos), 0);

  console.log('ROLL');
  await rail('disc'); await click([-300, -200]); await rail('motor'); await click([-300 + (48 + 12) * 12 / (2 * Math.PI), -200]);
  await rail('reader'); await click([-300, -245]); await rail('select');
  await click([-350, -160]);
  const disc = await probe(() => { const c = __cogs.W.cogs.find(o => o.track), r = __cogs.W.readers[0]; return { id: c.id, n: c.n, reader: r && [r.x1, r.x2].map(v => +v.toFixed(6)), sel: __cogs.sel }; });
  check('disc placed, reader along its radius, disc selected', disc.n === 48 && disc.reader && disc.reader.every(x => x === -300) && disc.sel.id === disc.id, disc);
  await p.fill('#inspector [data-k="rpm"] input[type=number]', '30'); await settle();
  const roll = await probe(() => __cogs.roll);
  check('selecting a disc docks its roll strip', roll && roll.id === disc.id && roll.rings === 12 && roll.div === 16 && roll.row >= 20, roll);
  await p.mouse.click(roll.x + roll.label + 4.5 * roll.cw, roll.y + (roll.rings - 1 - 3 + .5) * roll.row); await settle();
  const pin = await probe(() => __cogs.W.cogs.find(o => o.track).track.dots.map(d => [d.ring, +d.a.toFixed(6)]));
  is('click column 4, ring 3: one pin, a quarter turn behind', pin, [[3, +(1.5 * Math.PI).toFixed(6)]]);
  await shot('desk-03-roll');
  await p.click('button[data-act="reset hits"]'); const t0 = await probe(() => __cogs.simT); await wait(4300);
  const played = await probe(([t0]) => {
    const c = __cogs.W.cogs.find(o => o.track), r = __cogs.W.readers[0], d = c.track.dots[0], phi = Math.atan2(r.y1 - c.y, r.x1 - c.x), TAU = 2 * Math.PI;
    const ev = __cogs.stats.log.filter(l => l.src.endsWith('.ring3>reader' + r.id) && l.t >= t0 && l.t < t0 + 4);
    return { n: ev.length, gap: ev.length > 1 ? ev[1].t - ev[0].t : 0,
      miss: ev.map(l => { const a = c.th - c.w * (__cogs.simT - l.t) + d.a - phi; return Math.abs(((a % TAU) + TAU + Math.PI) % TAU - Math.PI); }) };
  }, [t0]);
  check('30 rpm: the pin plays twice in 4 s, 2 s apart', played.n === 2 && near(played.gap, 2, 1e-6), [played.n, played.gap]);
  check('each at the moment the pin crosses the reader', played.miss.length === 2 && played.miss.every(m => m < 1e-3), played.miss);

  console.log('ROLL SELECTION');
  await p.click('text=clear pins'); await settle();
  const R = await probe(() => __cogs.roll), grid = (gx, gy) => [R.x + R.label + gx * R.cw, R.y + gy * R.row];
  const cellAt = (col, ring) => grid(col + .5, R.rings - 1 - ring + .5), pins = async () => (await probe(() => __cogs.roll)).pins.map(q => q.slice(0, 3));
  const rk = [await key()], rs = [await undos()], rmark = async () => { await settle(); rk.push(await key()); rs.push(await undos()); };
  for (const [col, ring] of [[0, 2], [2, 3], [4, 2], [10, 6]]) { await p.mouse.click(...cellAt(col, ring)); await rmark(); }
  is('four clicks paint four pins', await pins(), [[0, 2, 0], [2, 3, 0], [4, 2, 0], [10, 6, 0]]);
  await p.keyboard.down('Control'); await p.mouse.move(...grid(.1, R.rings - 1 - 3 + .1)); await p.mouse.down();
  await p.mouse.move(...grid(4.9, R.rings - 1 - 2 + .9), { steps: 10 }); await shot('desk-04-roll-marquee'); await p.mouse.up(); await p.keyboard.up('Control');
  is('ctrl-drag a box over columns 0-4, rings 2-3: exactly those three pins', await pins(), [[0, 2, 1], [2, 3, 1], [4, 2, 1], [10, 6, 0]]);
  is('selecting is not an undo step', await undos(), rs[rs.length - 1]);
  await p.keyboard.down('Control'); await p.mouse.click(...cellAt(10, 6)); await p.mouse.click(...cellAt(10, 6)); await p.keyboard.up('Control');
  is('ctrl-click toggles a pin in and out', (await pins())[3], [10, 6, 0]);
  await shot('desk-05-roll-selection');
  await p.mouse.move(...cellAt(2, 3)); await p.mouse.down(); await p.mouse.move(...cellAt(5, 4), { steps: 10 }); await p.mouse.up(); await rmark();
  is('drag a selected pin 3 columns right, 1 ring up: the set moves', await pins(), [[3, 3, 1], [5, 4, 1], [7, 3, 1], [10, 6, 0]]);
  await p.click('button[data-act="reset hits"]'); const m0 = await probe(() => __cogs.simT); await wait(4300);
  const moved = await probe(([t0]) => {
    const c = __cogs.W.cogs.find(o => o.track), r = __cogs.W.readers[0], phi = Math.atan2(r.y1 - c.y, r.x1 - c.x), TAU = 2 * Math.PI, rings = {}, miss = [];
    for (const l of __cogs.stats.log.filter(l => l.src.endsWith('>reader' + r.id) && l.t >= t0 && l.t < t0 + 4)) {
      const ring = +l.src.match(/ring(\d+)/)[1]; rings[ring] = (rings[ring] || 0) + 1;
      miss.push(Math.min(...c.track.dots.filter(d => d.ring === ring).map(d => { const a = c.th - c.w * (__cogs.simT - l.t) + d.a - phi; return Math.abs(((a % TAU) + TAU + Math.PI) % TAU - Math.PI); })));
    }
    return { rings, worst: Math.max(...miss) };
  }, [m0]);
  is('the reader plays the moved pins: 2 on ring 3, 1 on ring 4, 1 on ring 6, twice each in 4 s', moved.rings, { 3: 4, 4: 2, 6: 2 });
  check('each hit lands where a pin crosses the reader', moved.worst < 1e-3, moved.worst);
  await p.keyboard.press('ArrowRight'); await rmark();
  await p.keyboard.press('ArrowUp'); await rmark();
  await p.keyboard.press('Shift+ArrowUp'); await rmark();
  is('right, up, shift+up (an octave of the chromatic disc, stopped at the top lane): one column, one ring, up to lane 11', await pins(), [[4, 10, 1], [6, 11, 1], [8, 10, 1], [10, 6, 0]]);
  await p.keyboard.press('c'); is('part keys stay off the canvas while the roll has a selection', await probe(() => __cogs.armed), null);
  await p.keyboard.press('Control+c'); await p.mouse.click(...cellAt(12, 0)); await settle();
  is('a click on empty space clears the selection and paints nothing', await pins(), [[4, 10, 0], [6, 11, 0], [8, 10, 0], [10, 6, 0]]);
  await p.keyboard.press('Control+v'); await rmark();
  is('ctrl+c, ctrl+v: a copy lands at the clicked column, wrapping', await pins(), [[0, 10, 1], [4, 10, 0], [6, 11, 0], [8, 10, 0], [10, 6, 0], [12, 10, 1], [14, 11, 1]]);
  await p.keyboard.press('Delete'); await rmark();
  is('Delete removes the pasted set, not the disc', [await pins(), (await probe(() => __cogs.sel)).type], [[[4, 10, 0], [6, 11, 0], [8, 10, 0], [10, 6, 0]], 'cog']);
  await p.keyboard.press('Control+a'); is('ctrl+a selects every pin', (await pins()).map(q => q[2]), [1, 1, 1, 1]);
  await p.keyboard.press('Control+x'); await rmark(); const cut = await pins();
  await p.keyboard.press('Control+v'); await rmark();
  is('ctrl+x empties the roll, ctrl+v puts the set back at column 12', [cut, await pins()], [[], [[0, 10, 1], [2, 6, 1], [12, 10, 1], [14, 11, 1]]]);
  await p.fill('#dock [data-k="brush p"] input[type=number]', '0.5'); await rmark();
  is('the brush p slider sets the selection', await probe(() => __cogs.W.cogs.find(o => o.track).track.dots.map(d => d.p)), [.5, .5, .5, .5]);
  await p.focus('#dock canvas'); await p.keyboard.press('Escape');
  is('Esc clears the selection', (await pins()).map(q => q[2]), [0, 0, 0, 0]);
  await p.keyboard.press('d'); is('a focused roll keeps part keys too', await probe(() => __cogs.armed), null);
  is('each roll edit is exactly one undo step', rs.slice(1).map((s, i) => s - rs[i]), rs.slice(1).map(() => 1));
  const walk = [];
  for (let i = rk.length - 2; i >= 0; i--) { await p.keyboard.press('Control+z'); walk.push(await key() === rk[i]); }
  is('ctrl+z walks every roll edit back exactly', walk, walk.map(() => true));
  await click([150, -250]);
  is('deselect: the dock goes', await probe(() => __cogs.roll), null);

  console.log('WHEEL ZOOM');
  const under = at => probe(([x, y]) => { const v = __cogs.serialize().view; return [(x - innerWidth / 2) / __cogs.S.zoom + v.x, (y - innerHeight / 2) / __cogs.S.zoom + v.y]; }, at);
  const wAt = [700, 300], z0 = await probe(() => __cogs.S.zoom), u0 = await under(wAt);
  await p.mouse.move(...wAt); await p.mouse.wheel(0, -100); await wait(200);
  const z1 = await probe(() => __cogs.S.zoom), u1 = await under(wAt);
  check('wheel up zooms in', z1 > z0 * 1.1, [z0, z1]);
  check('the point under the cursor stays put', near(u0[0], u1[0], .5) && near(u0[1], u1[1], .5), [u0, u1]);
  is('zoom is not an undo step', await probe(() => __cogs.pending), false);

  console.log('ROOM AND TEMPO EDITS');
  const roomU0 = await undos();
  await p.fill('#panel [data-k="wet"] input[type=number]', '0.9'); await settle();
  await p.fill('#panel [data-k="mesh tol"] input[type=number]', '9'); await settle();
  is('panel room and tempo edits are undo steps', (await undos()) - roomU0, 2);
  await p.keyboard.press('Control+z'); await settle();
  is('undo reverts the last of them only', await probe(() => [__cogs.SPACE.wet, __cogs.S.meshTol]), [0.9, 4]);
  await p.keyboard.press('Control+Shift+z'); await settle(); await wait(600);
  await p.reload(); await wait(600); await adv();
  is('they survive a reload', await probe(() => [__cogs.SPACE.wet, __cogs.S.meshTol]), [0.9, 9]);

  console.log('LOADING AND SAVING HONESTLY');
  if (!(await probe(() => __cogs.open))) await rail('panel');
  const before = await key();
  const bad = await probe(() => { const j = JSON.parse(JSON.stringify(__cogs.serialize())); j.strings = []; j.pads[0].voice.kind = 'bogus'; return JSON.stringify(j); });
  await p.fill('#panel textarea', bad); await p.click('#panel button:text-is("import")'); await settle();
  is('an import with a bad voice is refused whole: the world is untouched', [await key() === before, await probe(() => __cogs.W.strings.length > 0 || __cogs.W.pads.length > 0)], [true, true]);
  await probe(() => localStorage.setItem('cogs.v1', '{not json'));
  await p.reload(); await wait(600); await adv();
  is('a corrupt save is kept aside and announced', await probe(() => [localStorage.getItem('cogs.v1.rejected'), !document.querySelector('#notice').hidden && /could not be loaded/.test(document.querySelector('#notice').textContent)]), ['{not json', true]);
  await p.click('#notice');
  const filled = await probe(() => { let n = 0; for (let size = 1 << 16; size >= 1; size >>= 1) for (;;) { try { localStorage.setItem('fill' + n, 'x'.repeat(size)); n++; } catch (e) { break; } } return [n]; });
  if (!(await probe(() => __cogs.open))) await rail('panel');
  const big = await probe(() => { const j = JSON.parse(JSON.stringify(__cogs.serialize())); for (let i = 0; i < 300; i++) j.cogs.push({ id: 5000 + i, x: 3000 + i * 60, y: 3000, n: 12, motor: 0, revs: 0, layer: 0, th: 0, track: null }); return JSON.stringify(j); });
  await p.fill('#panel textarea', big); await p.click('#panel button:text-is("import")'); await settle();
  await p.waitForFunction(() => /not saved/.test(document.querySelector('#notice').textContent) && /off/.test(document.querySelector('#panel [data-k="storage"]').textContent), null, { timeout: 5000 }).catch(() => {});
  is('a save that does not fit is announced, and the panel says storage is off', await probe(() => [!document.querySelector('#notice').hidden && /not saved/.test(document.querySelector('#notice').textContent), [...document.querySelectorAll('#panel .ro')].find(r => r.textContent.startsWith('storage'))?.textContent.includes('off')]), [true, true]);
  await probe(n => { for (let i = 0; i < n; i++) localStorage.removeItem('fill' + i); }, filled[0]);

  console.log('BOUND');
  const bound = await probe(() => { for (let i = 0; i < 520; i++) { __cogs.W.pads[0].x += 1; __cogs.commit(); } return __cogs.undos; });
  is('520 more steps: history holds the last 500', bound, 500);
  console.log('RESIZE KEEPS THE MESH');
  await p.click('button[data-act=clear]');
  await p.fill('#panel [data-k="zoom"] input[type=number]', '1'); await p.fill('#panel [data-k="bpm lock"] input[type=number]', '0'); await settle();
  await rail('motor'); await click([-100, 0]); await rail('cog'); await click([-54, 0]); await click([-8, 0]); await rail('select');
  await click([-100, 0]); await p.fill('#inspector [data-k="rpm"] input[type=number]', '60'); await settle();
  const train = () => probe(() => __cogs.W.cogs.map(c => ({ x: +c.x.toFixed(6), y: +c.y.toFixed(6), n: c.n, rpm: +(c.w * 60 / (2 * Math.PI)).toFixed(6), jam: c.jam })));
  const meshed = t => t.slice(1).every((c, i) => near(Math.hypot(c.x - t[i].x, c.y - t[i].y), (c.n + t[i].n) * 12 / (2 * Math.PI), 1e-5));
  const g0 = await train();
  check('three cogs placed in a meshing chain', g0.length === 3 && meshed(g0), g0);
  await click([g0[1].x, g0[1].y]); await p.fill('#inspector [data-k="teeth"] input[type=number]', '24'); await settle();
  const g1 = await train();
  check('teeth 12 -> 24: the driver stays put', g1[0].x === g0[0].x && g1[0].y === g0[0].y, g1[0]);
  check('both meshes kept: the middle cog grows away, the last slides along', g1[1].n === 24 && meshed(g1) && g1.every(c => !c.jam), g1);
  is('the train still turns at the right ratios', g1.map(c => c.rpm), [60, -30, 60]);
  const rB = 24 * 12 / (2 * Math.PI), rimFrom = [g1[1].x, g1[1].y - rB], rimTo = [g1[1].x, g1[1].y - rB - 23];
  await drag(rimFrom, rimTo);
  const g2 = await train(), tx = g1[0].x + 12 * 12 / (2 * Math.PI), qx = rimTo[0] - tx, qy = rimTo[1];
  const wantN = Math.round((qx * qx + qy * qy) / (2 * qx) * 2 * Math.PI / 12);
  check('pull the top of the rim up 23 px: the rim passes through the pointer, the contact stays', g2[1].n === wantN && wantN > 24, [g2[1].n, wantN]);
  check('rim pull keeps both meshes and the driver', meshed(g2) && g2[0].x === g0[0].x && g2.every(c => !c.jam), g2);
  await p.keyboard.press('Control+z'); await settle();
  is('one ctrl+z undoes the whole rim pull', await train(), g1);
  await shot('desk-06-resize');

  console.log('COPY AND PASTE');
  await p.mouse.click(...await scr([0, 250])); await settle();
  const orig = await train(), [bx0, by0] = await scr([-160, -90]), [bx1, by1] = await scr([150, 90]);
  await p.keyboard.down('Control'); await p.mouse.move(bx0, by0); await p.mouse.down(); await p.mouse.move(bx1, by1, { steps: 8 }); await p.mouse.up(); await p.keyboard.up('Control');
  is('ctrl-drag a box round the train: three cogs selected', await probe(() => __cogs.group.length), 3);
  await p.keyboard.press('Control+c'); await p.mouse.move(...await scr([0, 220])); await p.keyboard.press('Control+v'); await settle();
  const both = await train(), cxs = orig.map(c => c.x), mx = (Math.min(...cxs) + Math.max(...cxs)) / 2;
  check('ctrl+c, ctrl+v: the originals stay and a copy lands centred under the pointer, still meshed and turning alike',
    JSON.stringify(both.slice(0, 3)) === JSON.stringify(orig) && both.length === 6 && both.slice(3).every((c, i) => near(c.x, orig[i].x - mx, 1e-6) && near(c.y, orig[i].y + 220, 1e-6) && c.n === orig[i].n && c.rpm === orig[i].rpm && !c.jam), both.slice(3));
  is('the copy is selected', await probe(() => __cogs.group), ['cog', 'cog', 'cog']);
  await shot('desk-07-paste');
  await p.keyboard.press('Control+x'); await settle();
  is('ctrl+x takes the copy away', await train(), orig);
  await p.mouse.move(...await scr([0, -220])); await p.keyboard.press('Control+v'); await settle();
  is('and ctrl+v puts it back under the pointer', (await train()).slice(3).map(c => Math.round(c.y)), [-220, -220, -220]);
  await p.keyboard.press('Control+z'); await settle(); await p.keyboard.press('Control+z'); await settle(); await p.keyboard.press('Control+z'); await settle();
  is('three ctrl+z: paste, cut and paste undone', await train(), orig);

  check('page errors', errs.length === 0, errs.slice(0, 3));
  console.log(pass ? '\nCOGS DESK: PASS' : '\nCOGS DESK: FAIL');
  await b.close(); process.exit(pass ? 0 : 1);
})();
