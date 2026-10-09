// Drives desk.html with a mouse and keyboard in a 1440x900 browser: toolbar, parts, inspector controls, roll, undo, wheel zoom.
const { chromium } = require('/srv/rig/monad/node_modules/playwright-core');
const path = require('path');
const FILE = process.argv[2] || ('file://' + path.join(__dirname, 'desk.html'));
const SHOTS = process.env.COGS_SHOTS || '';
(async () => {
  const b = await chromium.launch({ executablePath: (process.env.MONAD_CHROME || '/srv/rig/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome') });
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
  const rail = async id => { await p.click(`[data-rail="${id}"]`); await wait(80); };
  const key = () => probe(() => __cogs.worldKey());
  const undos = () => probe(() => __cogs.undos);
  const shot = async name => { if (SHOTS) await p.screenshot({ path: path.join(SHOTS, name + '.png') }); };

  await p.goto(FILE); await wait(500);
  await p.mouse.click(40, 860); await wait(300);
  await p.waitForFunction(() => __cogs.engine().state === 'ready' && __cogs.engine().ac === 'running', null, { timeout: 5000 });
  const smooth = await probe(() => new Promise(r => { const s = []; const f = ts => { s.push([ts, __cogs.clock.view * 1000]); if (s.length < 40) requestAnimationFrame(f); else {
    const k = s.slice(1).map(([t, v], i) => (v - s[i][1]) / (t - s[i][0])).filter(Number.isFinite); r(Math.max(...k.map(x => Math.abs(x - 1)))); } }; requestAnimationFrame(f); }));
  check('drawn motion advances with the frame clock (worst frame within 5%)', smooth < .05, +smooth.toFixed(4));
  await shot('desk-01-demo');

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

  console.log('PARTS FROM THE TOOLBAR');
  await p.click('button[data-act=clear]');
  await p.fill('#panel [data-k="zoom"] input[type=number]', '1'); await p.fill('#panel [data-k="bpm lock"] input[type=number]', '0'); await settle();
  const keys = [await key()], steps = [await undos()];
  const mark = async () => { keys.push(await key()); steps.push(await undos()); };
  await rail('cog'); is('a part button arms that part', await probe(() => __cogs.armed), 'cog');
  await click([-100, 0]); await mark();
  await click([100, 0]); await mark();
  await rail('pad'); await click([0, -100]); await mark();
  await rail('select');
  const placed = await probe(() => [__cogs.armed, __cogs.W.cogs.map(c => [c.x, c.y]), __cogs.W.pads.map(o => [o.x, o.y])]);
  is('two cogs and a pad land where clicked; select disarms', placed, [null, [[-100, 0], [100, 0]], [[0, -100]]]);
  await drag([-100, 0], [-100, 40]); await mark();
  is('the cog dragged to y 40', await probe(() => __cogs.W.cogs[0].y), 40);

  console.log('INSPECTOR');
  await click([-110, 55]);
  is('click a cog: the inspector shows it', await probe(() => __cogs.sel), { type: 'cog', id: await probe(() => __cogs.W.cogs[0].id) });
  await p.fill('#inspector [data-k="motor rpm"] input[type=number]', '60'); await settle(); await mark();
  is('typing 60 into motor rpm turns it at 60 rpm', await probe(() => +(__cogs.W.cogs[0].w * 60 / (2 * Math.PI)).toFixed(6)), 60);
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
  await rail('disc'); await click([-300, -200]);
  await rail('reader'); await click([-300, -245]); await rail('select');
  await click([-350, -160]);
  const disc = await probe(() => { const c = __cogs.W.cogs.find(o => o.track), r = __cogs.W.readers[0]; return { id: c.id, n: c.n, reader: r && [r.x1, r.x2].map(v => +v.toFixed(6)), sel: __cogs.sel }; });
  check('disc placed, reader along its radius, disc selected', disc.n === 48 && disc.reader && disc.reader.every(x => x === -300) && disc.sel.id === disc.id, disc);
  await p.fill('#inspector [data-k="motor rpm"] input[type=number]', '30'); await settle();
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

  console.log('BOUND');
  const bound = await probe(() => { for (let i = 0; i < 520; i++) { __cogs.W.pads[0].x += 1; __cogs.commit(); } return __cogs.undos; });
  is('520 more steps: history holds the last 500', bound, 500);
  check('page errors', errs.length === 0, errs.slice(0, 3));
  console.log(pass ? '\nCOGS DESK: PASS' : '\nCOGS DESK: FAIL');
  await b.close(); process.exit(pass ? 0 : 1);
})();
