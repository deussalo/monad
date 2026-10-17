// Builds contraptions by touch on a 390x844 phone viewport and checks the mechanism and the audio it drives.
const { chromium } = require(process.env.COGS_PW || '/srv/rig/rig/webapp/node_modules/.pnpm/playwright-core@1.64.0/node_modules/playwright-core');
const path = require('path');
const FILE = process.argv[2] || ('file://' + path.join(__dirname, 'cogs.html'));
const SHOTS = process.env.COGS_SHOTS || '';
const PHONE = { viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true, deviceScaleFactor: SHOTS ? 2 : 1 };
(async () => {
  const b = await chromium.launch({ executablePath: (process.env.MONAD_CHROME || '/srv/rig/.cache/ms-playwright/chromium-1248/chrome-linux64/chrome'),
    args: [] });
  let pass = true;
  const check = (label, ok, got) => { if (!ok) pass = false; console.log(('  ' + label).padEnd(58), JSON.stringify(got), ok ? '✓' : '✗'); };
  const is = (label, got, want) => check(label, JSON.stringify(got) === JSON.stringify(want), got);
  const near = (a, b, tol) => Math.abs(a - b) <= tol;
  const wait = ms => new Promise(r => setTimeout(r, ms));

  // One phone page, driven by real touch events through CDP.
  async function phone() {
    const ctx = await b.newContext({ ...PHONE, permissions: ['clipboard-read', 'clipboard-write'] });
    const p = await ctx.newPage(), errs = [];
    p.on('pageerror', e => errs.push(e.message));
    const cdp = await ctx.newCDPSession(p);
    const T = (type, pts) => cdp.send('Input.dispatchTouchEvent', { type, touchPoints: pts.map(([x, y], id) => ({ x, y, id })) });
    const probe = (fn, arg) => p.evaluate(fn, arg);
    const scr = w => probe(([x, y]) => { const s = __cogs.toScreen({ x, y }); return [s.x, s.y]; }, w);
    const lerp = (a, z, k) => [a[0] + (z[0] - a[0]) * k, a[1] + (z[1] - a[1]) * k];
    const t = {
      p, errs, probe, scr,
      // Touch points with explicit ids, for checks that add and lift particular fingers.
      touch: (type, pts) => cdp.send('Input.dispatchTouchEvent', { type, touchPoints: pts.map(([id, x, y]) => ({ id, x, y })) }),
      // A path of screen points, with a frame between moves so velocities read like a finger.
      async stroke(pts, { steps = 8, frame = 16, hold = 0, mid = null } = {}) {
        await T('touchStart', [pts[0]]);
        if (hold) await wait(hold);
        for (let i = 1; i < pts.length; i++) for (let k = 1; k <= steps; k++) { await T('touchMove', [lerp(pts[i - 1], pts[i], k / steps)]); if (frame) await wait(frame); }
        if (mid) await mid();
        await T('touchEnd', []); await p.waitForFunction(() => !__cogs.flying);
      },
      async tap(s, gap = 460) { await T('touchStart', [s]); await T('touchEnd', []); await wait(gap); await p.waitForFunction(() => !__cogs.flying); },
      async dbl(s) { await T('touchStart', [s]); await T('touchEnd', []); await T('touchStart', [s]); await T('touchEnd', []); await wait(100); await p.waitForFunction(() => !__cogs.flying); },
      async pinch(c, d0, d1) {
        await T('touchStart', [[c[0] - d0 / 2, c[1]]]);
        await T('touchStart', [[c[0] - d0 / 2, c[1]], [c[0] + d0 / 2, c[1]]]);
        for (let k = 1; k <= 10; k++) { const d = d0 + (d1 - d0) * k / 10; await T('touchMove', [[c[0] - d / 2, c[1]], [c[0] + d / 2, c[1]]]); }
        await T('touchEnd', []); await wait(200);
      },
      shelf: kind => probe(k => [innerWidth / __cogs.SHELF.length * (__cogs.SHELF.indexOf(k) + .5), __cogs.shelfTop() + 38], kind),
      async fromShelf(kind, ...worldPath) { const pts = [await t.shelf(kind)]; for (const w of worldPath) pts.push(await scr(w)); await t.stroke(pts); await wait(100); },
      async drag(a, z) { await t.stroke([await scr(a), await scr(z)]); await wait(100); },
      async room() { if (await probe(() => !!__cogs.sel)) await t.tap(await t.empty()); if (!(await probe(() => !!__cogs.activeBezel()))) await t.tap(await t.empty()); },
      async glyph() { await t.tap(await probe(() => { const g = __cogs.glyphAt(); return [g.x, g.y]; })); },
      async dev(open) {
        if (open === await probe(() => !document.querySelector('#dev').hidden)) return;
        if (open) { const g = await probe(() => { const g = __cogs.glyphAt(); return [g.x, g.y]; }); await T('touchStart', [g]); await p.waitForFunction(() => !document.querySelector('#dev').hidden); await T('touchEnd', []); await wait(150); await probe(() => { document.querySelector('#panel details').open = true; }); }
        else await p.tap('button[data-act=close]');
      },
      async act(label) { await t.dev(true); await p.tap(`button[data-act="${label}"]`); },
      // Reset the hit counters and return the sim time it happened at: counts are taken over exact sim windows from it.
      async reset() { await t.act('reset hits'); const at = await probe(() => __cogs.simT); await t.dev(false); return at; },
      count: (src, t0, span) => probe(([src, t0, span]) => __cogs.stats.log.filter(l => l.src.includes(src) && l.t >= t0 && l.t < t0 + span).length, [src, t0, span]),
      async field(k, v) { await t.dev(true); await p.fill(`#inspector [data-k="${k}"] input[type=number]`, String(v)); },
      async world(k, v) { await t.dev(true); await p.fill(`#panel [data-k="${k}"] input[type=number]`, String(v)); },
      async pick(k, v) { await t.dev(true); await p.selectOption(`#inspector [data-k="${k}"] select`, v); },
      async shot(name) { if (SHOTS) await p.screenshot({ path: path.join(SHOTS, name + '.png') }); },
      // A screen point with no part under it, for pans and taps on empty ground.
      empty: () => probe(() => {
        const { W, radius, toScreen, shelfTop } = __cogs, ok = (sx, sy) => {
          const seg = (x1, y1, x2, y2) => { const a = toScreen({ x: x1, y: y1 }), b = toScreen({ x: x2, y: y2 }), ex = b.x - a.x, ey = b.y - a.y, u = Math.max(0, Math.min(1, ((sx - a.x) * ex + (sy - a.y) * ey) / (ex * ex + ey * ey || 1))); return Math.hypot(sx - a.x - ex * u, sy - a.y - ey * u) < 40; };
          const near = (x, y, r) => Math.hypot(toScreen({ x, y }).x - sx, toScreen({ x, y }).y - sy) < r * __cogs.S.zoom + 40;
          return !W.cogs.some(c => near(c.x, c.y, radius(c) + 4)) && !W.pads.some(o => near(o.x, o.y, o.r))
            && ![...W.readers, ...W.strings].some(o => seg(o.x1, o.y1, o.x2, o.y2)) && !W.belts.some(o => __cogs.beltLines(o).some(l => seg(...l)))
            && !W.arms.some(a => near(a.cog.x, a.cog.y, a.r0 + a.len)) && !(b && Math.abs(Math.hypot(sx - b.cx, sy - b.cy) - b.R) < 50);
        }, b = __cogs.activeBezel();
        for (let sy = 90; sy < shelfTop() - 60; sy += 20) for (let sx = 30; sx < innerWidth - 30; sx += 20) if (ok(sx, sy)) return [sx, sy];
        return null;
      }),
    };
    return t;
  }

  const m = await phone(), { p, probe } = m;
  await p.goto(FILE); await wait(500);
  await m.tap(await m.empty());
  await p.waitForFunction(() => __cogs.engine().state === 'ready' && __cogs.engine().ac === 'running', null, { timeout: 5000 });
  const smooth = await probe(() => new Promise(r => { const s = []; const f = ts => { s.push([ts, __cogs.clock.view * 1000]); if (s.length < 40) requestAnimationFrame(f); else {
    const k = s.slice(1).map(([t, v], i) => (v - s[i][1]) / (t - s[i][0])).filter(Number.isFinite); r(Math.max(...k.map(x => Math.abs(x - 1)))); } }; requestAnimationFrame(f); }));
  check('drawn motion advances with the frame clock (worst frame within 5%)', smooth < .05, +smooth.toFixed(4));
  await m.shot('01-demo');

  console.log('DEFAULT WORLD');
  const WANT = JSON.parse(require('fs').readFileSync(path.join(__dirname, 'src', 'default-world.json'), 'utf8'));
  const parts = w => JSON.stringify({ ...w, S: { ...w.S, zoom: 0 }, view: 0, cogs: w.cogs.map(c => ({ ...c, th: 0 })) });
  check('a fresh page opens the default world exactly (camera and phase aside)', parts(await probe(() => __cogs.serialize())) === parts(WANT), 'see src/default-world.json');
  const g0 = await m.reset();
  is('holding the glyph open over the panel presses nothing beneath it', await probe(() => [__cogs.W.cogs.length, __cogs.clock.paused]), [WANT.cogs.length, false]);
  await wait(4800);
  const heard = await probe(([t0, span]) => [...new Set(__cogs.stats.log.filter(l => l.t >= t0 && l.t < t0 + span).map(l => l.src.split('>')[1]))].sort(), [g0, 4 * 60 / WANT.S.bpm * 2]);
  is('over two bars every driven pad and reader sounds', heard, [...WANT.pads.map(o => 'pad' + o.id), ...WANT.readers.map(o => 'reader' + o.id)].sort());
  check('default world: no event scheduled late', (await probe(() => __cogs.stats.late)) === 0, await probe(() => __cogs.stats.late));
  console.log('CRANK ON A BUSY RIM');
  await m.glyph();
  const armsNow = () => probe(() => JSON.stringify(__cogs.W.arms.map(a => [a.id, a.a, a.len])));
  const arms0 = await armsNow(), rpm0 = await probe(() => __cogs.W.cogs[0].w * 60 / (2 * Math.PI));
  const rim = await probe(() => { const c = __cogs.W.cogs[0], a = __cogs.W.arms.find(a => a.cog === c), ang = __cogs.renderTh(c) + a.a, s = __cogs.toScreen(c), R = __cogs.radius(c) * __cogs.S.zoom, tip = __cogs.toScreen(__cogs.armTip(a));
    return { cx: s.x, cy: s.y, R, ang, tipGap: Math.hypot(tip.x - s.x - Math.cos(ang) * R, tip.y - s.y - Math.sin(ang) * R) }; });
  const onRim = a => [rim.cx + Math.cos(a) * rim.R, rim.cy + Math.sin(a) * rim.R];
  await m.stroke([0, 1, 2, 3, 4, 5, 6].map(k => onRim(rim.ang + k * .25)), { steps: 3, frame: 16, mid: async () => { rim.during = await probe(() => __cogs.W.cogs[0].w * 60 / (2 * Math.PI)); } });
  const rpm1 = await probe(() => __cogs.W.cogs[0].w * 60 / (2 * Math.PI));
  check(`grab the rim where an arm tip passes ${rim.tipGap.toFixed(0)} px away: the rim cranks, the arm is untouched`, (await armsNow()) === arms0 && !near(rim.during, rpm0, 1e-6), [rpm0, rim.during]);
  const DETS = [1/8, 1/6, 1/4, 1/3, 1/2, 2/3, 3/4, 1, 4/3, 3/2, 2, 3, 4, 6, 8].map(r => r * WANT.S.bpm / 4), nearest = Math.min(...DETS.map(d => Math.abs(Math.log(Math.abs(rim.during) / d))));
  check('let go mid-crank: the speed stays where it was left, unless within 6% of a step', nearest < Math.log(1.06) ? DETS.some(d => near(Math.abs(rpm1), d, 1e-6)) : near(rpm1 / rim.during, 1, .15) && !near(rpm1, rpm0, 1e-6), [rim.during, rpm1]);
  await m.glyph();

  await m.act('clear'); await m.world('bpm lock', 0); await m.world('zoom', 1.6); await m.dev(false);

  console.log('PANEL SHEET');
  await m.dev(true);
  const sheet = await probe(() => { const r = document.querySelector('#dev').getBoundingClientRect(); return { top: r.top, h: innerHeight, open: !document.querySelector('#dev').hidden }; });
  check('hold the glyph: the panel opens as a sheet, the machine visible above it', sheet.open && sheet.top >= .28 * sheet.h - 1, sheet);
  const world0 = await probe(() => JSON.stringify(__cogs.W.cogs.map(c => [c.x, c.y])));
  await m.tap([195, 60]);
  is('tap above the sheet: the panel closes and nothing else happens', await probe(w0 => [document.querySelector('#dev').hidden, JSON.stringify(__cogs.W.cogs.map(c => [c.x, c.y])) === w0, __cogs.sel], world0), [true, true, null]);
  await m.dev(true); await p.tap('button[data-act=close]');
  is('close in the sheet closes it', await probe(() => document.querySelector('#dev').hidden), true);

  console.log('GEARS');
  await m.stroke([await m.shelf('motor'), await m.scr([30, 60]), await m.scr([0, 0])]);
  await m.drag([22.92, 0], [46, 0]);
  await m.stroke([await m.shelf('cog'), await m.scr([70, 0])], { mid: () => m.shot('02-carry-snap') });
  const gears = await probe(() => __cogs.W.cogs.map(c => ({ n: c.n, x: c.x, y: c.y, rpm: c.w * 60 / (2 * Math.PI), jam: c.jam })));
  check('motor rim pulled out to 46: 24 teeth; a fresh cog: 12', gears[0].n === 24 && gears[1].n === 12, gears.map(g => g.n));
  check('second cog snapped to pitch distance 68.75', near(Math.hypot(gears[1].x - gears[0].x, gears[1].y - gears[0].y), 36 * 12 / (2 * Math.PI), 1e-9), gears[1].x - gears[0].x);
  const [c1, c2] = gears.map(g => [g.x, g.y]);
  await m.tap(await m.scr([c1[0] - 20, c1[1] - 15])); await m.field('rpm', 60); await m.dev(false);
  const rpm = () => probe(() => __cogs.W.cogs.map(c => +(c.w * 60 / (2 * Math.PI)).toFixed(6)));
  check('60 rpm drives -120 rpm', JSON.stringify(await rpm()) === '[60,-120]', await rpm());

  console.log('CRANK');
  const top2 = [c2[0], c2[1] - 22.92];
  const turn = async (from, by, hold) => { const [cx, cy] = await m.scr(c2), [x, y] = await m.scr(top2), r = Math.hypot(x - cx, y - cy), a0 = Math.atan2(y - cy, x - cx) + from;
    await m.stroke([0, .25, .5, .75, 1].map(k => [cx + Math.cos(a0 + by * k) * r, cy + Math.sin(a0 + by * k) * r]), { steps: 4, frame: 12, mid: hold ? () => wait(hold) : null }); };
  const dial = rpm => Math.sign(rpm) * Math.log(Math.abs(rpm) / 2.5) / Math.log(120) * 1.7 * Math.PI, undial = a => Math.sign(a) * 2.5 * Math.pow(120, Math.abs(a) / (1.7 * Math.PI));
  await turn(0, .8, 400);
  const crank = await probe(() => __cogs.W.cogs.map(c => ({ motor: !!c.motor, rpm: c.w * 60 / (2 * Math.PI) })));
  check('turn the driven cog\'s rim clockwise and hold still before letting go: the motor stays the motor and turns it .8 rad along the dial from -120', crank[0].motor && !crank[1].motor && near(crank[1].rpm, undial(dial(-120) + .8), 4), [crank.map(c => c.rpm), undial(dial(-120) + .8)]);
  check('the motor turns at half that, the other way', near(crank[0].rpm, -crank[1].rpm / 2, 1e-9), crank.map(c => c.rpm));
  await turn(.8, -dial((await rpm())[1]));
  check('turn on clockwise to the middle of the dial: the train stops', JSON.stringify(await rpm()) === '[0,0]', await rpm());
  await turn(.8, 1.2);
  const onward = await rpm();
  check('and on round: it runs clockwise', onward[1] > 3 && near(onward[0], -onward[1] / 2, 1e-6), onward);
  if (SHOTS) await m.stroke([await m.scr(top2), await m.scr([c2[0] + 14, top2[1] + 4])], { steps: 6, frame: 8, mid: () => m.shot('03-crank') });
  await m.tap(await m.scr([c1[0] - 20, c1[1] - 15])); await m.field('rpm', 60); await m.dev(false);

  console.log('ARMS AND PADS');
  await m.glyph();
  is('glyph tap pauses', await probe(() => __cogs.clock.paused), true);
  await m.fromShelf('arm', [c1[0] + 30, c1[1] + 60], [c1[0], c1[1] - 30]);
  await m.drag([c1[0], c1[1] - 30], [c1[0], c1[1] - 80]);
  await m.fromShelf('arm', [c2[0], c2[1] + 15]);
  await m.drag([c2[0], c2[1] + 15], [c2[0], c2[1] + 40]);
  await m.fromShelf('pad', [-60, -30]);
  await m.fromShelf('pad', [98.75, 20]);
  const built = await probe(() => ({ arms: __cogs.W.arms.map(a => [a.cog.id, +a.len.toFixed(1)]), pads: __cogs.W.pads.length }));
  check('arms dropped on each cog, tips swung out to 80 and 40', built.arms.length === 2 && near(built.arms[0][1], 80, 1) && near(built.arms[1][1], 40, 1), built.arms);
  await m.shot('04-built-paused');
  await m.glyph();
  const h0 = await m.reset(); await wait(4300);
  const hitsIn4 = [await m.count('>pad' + (await probe(() => __cogs.W.pads[0].id)), h0, 4), await m.count('>pad' + (await probe(() => __cogs.W.pads[1].id)), h0, 4)];
  const hits = await probe(() => {
    const [a, b] = __cogs.W.pads, log = __cogs.stats.log, c = __cogs.W.cogs[0];
    const times = id => log.filter(l => l.src.endsWith('>pad' + id)).map(l => l.t);
    const gaps = ts => ts.slice(1).map((t, i) => t - ts[i]);
    const D = Math.hypot(a.x - c.x, a.y - c.y);
    return { a: a.hits, b: b.hits, gapsA: gaps(times(a.id)), gapsB: gaps(times(b.id)), vA: a.lastV, vTangent: Math.abs(c.w) * Math.sqrt(D * D - a.r * a.r), late: __cogs.stats.late };
  });
  check('60 rpm arm: 4 hits in 4 s', hitsIn4[0] === 4, hitsIn4[0]);
  check('-120 rpm arm: 8 hits in 4 s', hitsIn4[1] === 8, hitsIn4[1]);
  check('hit period 1 s to 1e-6', hits.gapsA.length > 1 && hits.gapsA.every(g => near(g, 1, 1e-6)), hits.gapsA);
  check('hit period 0.5 s to 1e-6', hits.gapsB.length > 1 && hits.gapsB.every(g => near(g, .5, 1e-6)), hits.gapsB);
  check('impact speed = w * tangent radius', near(hits.vA, hits.vTangent, 1e-6), [hits.vA, hits.vTangent]);
  check('no event scheduled late', hits.late === 0, hits.late);

  console.log('SPEED ON A DRIVEN COG');
  await m.tap(await m.empty());
  await m.tap(await m.scr([c2[0] - 12, c2[1]])); await m.field('rpm', 100); await m.dev(false); await wait(150);
  is('give the driven cog a speed: the motor takes it through the ratio, nothing jams', await probe(() => __cogs.W.cogs.map(c => [c.jam, !!c.motor, +(c.w * 60 / (2 * Math.PI)).toFixed(6)])), [[false, true, -50], [false, false, 100]]);
  await m.field('rpm', -120); await m.dev(false);
  is('and back', await rpm(), [60, -120]);

  console.log('PAD BEZEL');
  await m.tap(await m.scr([-60, -30]));
  const bz = await probe(() => { const b = __cogs.activeBezel(); return b && { cx: b.cx, cy: b.cy, R: b.R, n: b.sectors.length }; });
  check('tap a pad: its bezel blooms: kind, the four kick params, level', bz && bz.n === 6, bz);
  await m.shot('06-pad-bezel');
  const sector = (b, i, f) => { const a = -Math.PI / 2 + (i + f) * 2 * Math.PI / b.n; return [b.cx + Math.cos(a) * b.R, b.cy + Math.sin(a) * b.R]; };
  const tune0 = await probe(() => __cogs.W.pads[0].voice.tune);
  const up = (at, px) => m.stroke([at, [at[0], at[1] - px]]);
  await up(sector(bz, 1, .5), 60);
  const tune1 = await probe(() => __cogs.W.pads[0].voice.tune);
  check('drag up from the tune sector raises the tune', tune1 > tune0, [tune0, tune1]);
  await m.tap(sector(bz, 0, .5));
  is('tap the kind sector: kick becomes snare', await probe(() => __cogs.W.pads[0].voice.kind), 'snare');
  await m.tap(await m.empty());
  is('tap empty ground: deselect, bezel gone', await probe(() => !__cogs.sel && !__cogs.activeBezel()), true);

  console.log('DISC AND READER');
  await m.fromShelf('disc', [0, 170]);
  const discXY = await probe(() => { const c = __cogs.W.cogs[2]; return [c.x, c.y, c.n]; });
  check('disc dropped clear of the train, 48 teeth', discXY[2] === 48 && near(discXY[1], 170, 1), discXY);
  await m.dbl(await m.scr([discXY[0] - 20, discXY[1] - 20]));
  is('double tap the disc: flies in until its face is the score', await probe(() => { const c = __cogs.W.cogs[2]; return c.track.range === 1 && __cogs.radius(c) * .65 / 11 * __cogs.S.zoom >= 12; }), true);
  for (const [dx, dy] of [[-40, 0], [0, -70], [-80, 0]]) await m.tap(await m.scr([discXY[0] + dx, discXY[1] + dy]));
  is('three taps set three pins', await probe(() => __cogs.W.cogs[2].track.dots.length), 3);
  await m.shot('07-disc-notes');
  await m.dbl(await m.empty()); await wait(200);
  await m.fromShelf('reader', [discXY[0], discXY[1] - 45]);
  const rd0 = await probe(() => { const r = __cogs.W.readers[0]; return r && [r.x1, r.y1, r.x2, r.y2].map(v => +v.toFixed(1)); });
  check('reader dropped on the disc lies along its radius', rd0 && near(rd0[0], discXY[0], 1e-6) && near(rd0[2], discXY[0], 1e-6) && rd0[1] > rd0[3], rd0);
  await m.tap(await m.scr([discXY[0] - 60, discXY[1] + 20]));
  await m.shot('08-disc-bezel');
  is('tap the disc: its tuning bezel (scale, root, octave, range, steps)', await probe(() => [__cogs.activeBezel()?.sectors.length, __cogs.sel?.id === __cogs.W.cogs[2].id]), [5, true]);
  await m.fromShelf('motor', [discXY[0] + 60 * 12 / (2 * Math.PI), discXY[1]]);
  await m.tap(await m.scr([discXY[0] - 60, discXY[1] + 20])); await m.field('rpm', 30); await m.dev(false);
  const disc = await probe(() => { const c = __cogs.W.cogs[2]; return { n: c.n, dots: c.track.dots.length, rpm: c.w * 60 / (2 * Math.PI), strikes: __cogs.engine().strikes }; });
  check('disc: 48 teeth, 3 dots, 30 rpm', disc.n === 48 && disc.dots === 3 && near(disc.rpm, 30, 1e-9), disc);
  const d0 = await m.reset(); await wait(4300);
  const rd = await probe(() => ({ hits: 0, rings: [...new Set(__cogs.stats.log.filter(l => l.src.includes('>reader')).map(l => l.src.split('>')[0]))], eng: __cogs.engine() }));
  rd.hits = await m.count('>reader', d0, 4);
  check('reader: 3 dots x 2 revolutions', rd.hits === 6, rd.hits);
  check('three distinct rings sounded', rd.rings.length === 3, rd.rings);
  check('engine consumed the synth strikes', rd.eng.strikes - disc.strikes >= rd.hits - 1 && rd.eng.drops === 0, { strikes: rd.eng.strikes - disc.strikes, drops: rd.eng.drops });
  await m.dev(true); await wait(300);
  const peak = await probe(() => document.querySelector('#panel [data-k="peak dB"] b').textContent);
  check('output reaches the destination', parseFloat(peak) > -60, peak);

  console.log('ROLL');
  const box = await p.evaluate(() => { const cv = document.querySelector('#inspector canvas'); cv.scrollIntoView({ block: 'center' }); const r = cv.getBoundingClientRect(); return { x: r.left, y: r.top, w: r.width, h: r.height }; });
  const rows = Math.round(box.h / 11), cw = (box.w - 30) / 16, cell = (col, row) => [box.x + 30 + (col + .5) * cw, box.y + (row + .5) * 11];
  const dots = () => probe(() => __cogs.W.cogs[2].track.dots.map(d => [d.ring, +d.a.toFixed(6)]));
  const n0 = (await dots()).length;
  await m.tap(cell(2, rows - 1));
  const added = (await dots()).slice(-1)[0];
  check('roll cell col 2, bottom row -> ring 0 at -2 steps', (await dots()).length === n0 + 1 && added[0] === 0 && near(added[1], 2 * Math.PI - 2 * Math.PI / 16 * 2, 1e-6), added);
  await m.tap(cell(2, rows - 1), 60); await m.tap(cell(2, rows - 1));
  check('double tap removes it', (await dots()).length === n0, (await dots()).length - n0);
  for (let col = 0; col < 4; col++) await m.tap(cell(col, rows - 6));
  const painted = (await dots()).filter(d => d[0] === 5);
  check('four taps set 4 cells on ring 5', painted.length === 4, painted);
  const strip = () => probe(() => { const r = document.querySelector('#inspector .roll'), cv = r.querySelector('canvas'); return { w: cv.clientWidth, h: cv.clientHeight, sl: r.scrollLeft }; });
  const strip0 = await strip(), nDots = (await dots()).length, [mx, my] = cell(8, rows - 3);
  await m.touch('touchStart', [[1, mx - 30, my]]); await m.touch('touchStart', [[1, mx - 30, my], [2, mx + 30, my]]);
  for (let k = 1; k <= 6; k++) await m.touch('touchMove', [[1, mx - 30 - 12 * k, my], [2, mx + 30 + 12 * k, my]]);
  await m.touch('touchEnd', []); await wait(150);
  const zoomed = await strip();
  check('two fingers pinch the roll out: the strip grows, no pin is set', zoomed.w > strip0.w * 1.5 && zoomed.h > strip0.h * 1.5 && (await dots()).length === nDots, [strip0, zoomed]);
  await m.touch('touchStart', [[1, mx - 30, my]]); await m.touch('touchStart', [[1, mx - 30, my], [2, mx + 30, my]]);
  for (let k = 1; k <= 5; k++) await m.touch('touchMove', [[1, mx - 30 - 15 * k, my], [2, mx + 30 - 15 * k, my]]);
  await m.touch('touchEnd', []); await wait(150);
  check('two fingers drag left: the strip scrolls, no pin is set', (await strip()).sl > zoomed.sl && (await dots()).length === nDots, [zoomed.sl, (await strip()).sl]);
  const p5 = await m.reset(); await wait(2300);
  const r5 = await m.count('.ring5>reader', p5, 2);
  check('painted ring 5: 4 dots in one 2 s revolution', r5 === 4, r5);

  console.log('ROOM');
  await m.dev(false); await m.room();
  const room = await probe(() => { const b = __cogs.activeBezel(); return b && { cx: b.cx, cy: b.cy, R: b.R, n: b.sectors.length }; });
  check('tap empty ground with nothing selected: the room bezel, at any zoom', room && room.n === 11, room);
  await m.shot('09-room');
  const wet0 = await probe(() => __cogs.SPACE.wet);
  await up(sector(room, 0, .5), -60);
  const wet1 = await probe(() => __cogs.SPACE.wet);
  check('drag down from the wet sector: drier room', wet1 < wet0, [wet0, wet1]);
  await m.tap(sector(room, 10, .5)); await wait(400);
  const clip = await probe(() => navigator.clipboard.readText().catch(e => 'clipboard: ' + e.name));
  check('tap the seal: a #w= link reaches the clipboard', /#w=[\w-]+$/.test(clip), clip.slice(0, 40) + '…');

  console.log('PAUSE AND PERSIST');
  await m.glyph(); const before = await probe(() => __cogs.stats.events);
  await wait(1000); const after = await probe(() => __cogs.stats.events);
  check('paused: no events', after - before <= 1, after - before);
  await m.glyph();
  await wait(600); await p.reload(); await wait(400);
  const world = await probe(() => __cogs.W.cogs.length + ' ' + __cogs.W.arms.length + ' ' + __cogs.W.pads.length + ' ' + __cogs.W.readers.length);
  check('world restored after reload', world === '4 2 2 1', world);

  console.log('PINCH AND SCRAP');
  const tp = await phone();
  await tp.p.goto(FILE); await wait(500);
  const c0 = await tp.probe(() => { const c = __cogs.W.cogs[0], s = __cogs.toScreen(c); return { s: [s.x, s.y + __cogs.radius(c) * .5 * __cogs.S.zoom], x: c.x, y: c.y, zoom: __cogs.S.zoom, view: { ...__cogs.serialize().view }, parts: __cogs.W.cogs.length + __cogs.W.pads.length }; });
  await tp.pinch(c0.s, 100, 200);
  const c1p = await tp.probe(() => { const c = __cogs.W.cogs[0]; return { x: c.x, y: c.y, zoom: __cogs.S.zoom, parts: __cogs.W.cogs.length + __cogs.W.pads.length, anchor: __cogs.toScreen({ x: 0, y: 0 }) }; });
  const a0 = await tp.probe(([x, y, z, vx, vy]) => ({ x: (x - innerWidth / 2) / z + vx, y: (y - innerHeight / 2) / z + vy }), [...c0.s, c0.zoom, c0.view.x, c0.view.y]);
  const a1 = await tp.scr([a0.x, a0.y]);
  check('pinch 100 -> 200 px doubles zoom', near(c1p.zoom / c0.zoom, 2, .02), [c0.zoom, c1p.zoom]);
  check('world under the fingers stays put', near(a1[0], c0.s[0], .5) && near(a1[1], c0.s[1], .5), [c0.s, a1]);
  check('pinch that starts on a cog neither moves nor makes parts', c1p.x === c0.x && c1p.y === c0.y && c1p.parts === c0.parts, [c1p.x - c0.x, c1p.parts - c0.parts]);
  await tp.dbl(await tp.empty());
  await tp.glyph();
  const pad = await tp.probe(() => { const o = __cogs.W.pads[__cogs.W.pads.length - 1], s = __cogs.toScreen(o); return { id: o.id, s: [s.x, s.y], n: __cogs.W.pads.length }; });
  await tp.stroke([pad.s, [pad.s[0], 700], await tp.shelf('pad')]);
  is('drag a pad onto the shelf: gone', await tp.probe(id => [__cogs.W.pads.every(p => p.id !== id), __cogs.W.pads.length], pad.id), [true, pad.n - 1]);
  const zoomNow = () => tp.probe(() => __cogs.S.zoom);
  // CDP touchEnd lifts the points it lists; a finger that does not move sends no pointermove.
  await tp.touch('touchStart', [[1, 120, 400]]); await tp.touch('touchStart', [[1, 120, 400], [2, 270, 400]]);
  for (let k = 1; k <= 6; k++) await tp.touch('touchMove', [[1, 120 - 8 * k, 400], [2, 270 + 8 * k, 400]]);
  await wait(100); const z1 = await zoomNow();
  await tp.touch('touchStart', [[1, 72, 400], [2, 318, 400], [3, 195, 600]]); await wait(50);
  await tp.touch('touchEnd', [[1, 72, 400]]); await wait(50);
  await tp.touch('touchMove', [[2, 318, 400], [3, 196, 600]]); await wait(50);
  const z2 = await zoomNow();
  check('a palm lands mid-pinch, a pinch finger lifts, the palm moves 1 px: zoom moves under 1%', Math.abs(z2 / z1 - 1) < .01, [z1, z2]);
  await tp.touch('touchEnd', [[2, 318, 400]]); await wait(50);
  const v0 = await tp.probe(() => __cogs.serialize().view.x);
  for (let k = 1; k <= 3; k++) await tp.touch('touchMove', [[3, 196 + 10 * k, 600]]);
  await wait(50);
  check('the finger left after a pinch pans', !near(await tp.probe(() => __cogs.serialize().view.x), v0, 1e-6), v0);
  await tp.touch('touchEnd', [[3, 226, 600]]); await wait(100);
  await tp.p.waitForFunction(() => !__cogs.pending); await tp.act('undo');
  is('the panel undo button brings it back', await tp.probe(id => __cogs.W.pads.some(p => p.id === id), pad.id), true);
  await tp.act('redo'); await tp.dev(false);
  is('and redo scraps it again', await tp.probe(id => __cogs.W.pads.some(p => p.id === id), pad.id), false);

  console.log('DELETE AND START');
  const centre = s => tp.probe(sel => { const r = document.querySelector(sel).getBoundingClientRect(); return r.width ? [r.x + r.width / 2, r.y + r.height / 2] : null; }, s);
  is('the play glyph sits bottom right, just above the shelf', await tp.probe(() => { const g = __cogs.glyphAt(); return [innerWidth - g.x, __cogs.shelfTop() - g.y]; }), [32, 32]);
  is('nothing selected: no delete button', await centre('#delete-part'), null);
  const victim = await tp.probe(() => { const o = __cogs.W.readers[__cogs.W.readers.length - 1], s = __cogs.toScreen({ x: (o.x1 * 3 + o.x2) / 4, y: (o.y1 * 3 + o.y2) / 4 }); return { id: o.id, s: [s.x, s.y] }; });
  await tp.tap(await tp.empty());
  await tp.tap(victim.s);
  await tp.tap(await centre('#delete-part'));
  is('tap a reader, then delete: it is gone and nothing is selected', await tp.probe(id => [__cogs.W.readers.some(r => r.id === id), __cogs.sel], victim.id), [false, null]);
  const motorAngle = () => tp.probe(() => { const m = __cogs.W.cogs.find(c => c.motor && c.w); return m && +((__cogs.renderTh(m) % (2 * Math.PI) + 2 * Math.PI) % (2 * Math.PI)).toFixed(4); });
  const train = () => tp.probe(() => __cogs.W.cogs.filter(c => c.w).map(c => +((__cogs.renderTh(c) % (2 * Math.PI) + 2 * Math.PI) % (2 * Math.PI)).toFixed(4)));
  if (!await tp.probe(() => __cogs.clock.paused)) await tp.glyph();
  await tp.tap(await centre('#to-start'), 0); const first = await train(), angle = await motorAngle();
  check('set to start: the motor stands at angle 0', angle !== null && (angle < 1e-3 || angle > 2 * Math.PI - 1e-3), angle);
  await wait(700); await tp.tap(await centre('#to-start'), 0);
  is('pressed again later: every turning cog lands where it did the first time', await train(), first);

  console.log('BOX SELECT ON THE CANVAS');
  await tp.dbl(await tp.empty()); await wait(500); await tp.tap(await tp.empty());
  const e0 = await tp.empty();
  const box2 = await tp.probe(e => { const c = __cogs.W.cogs.find(c => c.track), s = __cogs.toScreen(c); return [e, [s.x + Math.sign(s.x - e[0]) * 20, s.y + Math.sign(s.y - e[1]) * 20]]; }, e0);
  const v0b = await tp.probe(() => JSON.stringify(__cogs.serialize().view));
  await tp.stroke(box2, { steps: 8, hold: 550 });
  const boxed = await tp.probe(() => __cogs.group);
  check('hold empty ground, then drag: a box takes the parts in it, the view stays', boxed.includes('cog') && await tp.probe(() => JSON.stringify(__cogs.serialize().view)) === v0b, boxed);
  await tp.tap(await tp.empty());
  await tp.stroke([box2[0], [box2[0][0] + 60, box2[0][1] + 40]], { steps: 6 });
  check('a slide on empty ground still pans', await tp.probe(() => JSON.stringify(__cogs.serialize().view)) !== v0b && (await tp.probe(() => __cogs.group)).length === 0, null);
  await tp.stroke(box2, { steps: 8, hold: 550 });
  const kept = await tp.probe(() => __cogs.group);
  await tp.stroke([box2[0], [box2[0][0] + 40, box2[0][1] + 30]], { steps: 6 });
  is('a slide on empty ground pans and keeps the selection', await tp.probe(() => __cogs.group), kept);
  const nParts = await tp.probe(() => __cogs.W.cogs.length);
  await tp.tap(await centre('#copy-parts')); await tp.tap(await centre('#paste-parts'));
  check('copy and paste buttons: the parts come back as a new selection', await tp.probe(() => __cogs.W.cogs.length) > nParts && (await tp.probe(() => __cogs.group)).length > 0, [nParts, await tp.probe(() => __cogs.W.cogs.length)]);
  await tp.probe(() => __cogs.undo());
  await tp.tap(await tp.empty());

  console.log('SEATED READERS');
  const seat = () => tp.probe(() => { const c = __cogs.W.cogs.find(c => c.id === 8), r = __cogs.W.readers.find(r => r.id === 9), R = __cogs.radius(c);
    return { on: r.on && r.on.id, f: [Math.hypot(r.x1 - c.x, r.y1 - c.y) / R, Math.hypot(r.x2 - c.x, r.y2 - c.y) / R].map(v => +v.toFixed(3)), ang: +Math.atan2((r.y1 + r.y2) / 2 - c.y, (r.x1 + r.x2) / 2 - c.x).toFixed(3) }; });
  const st0 = await seat();
  is('the default bass reader sits on its disc', st0.on, 8);
  await tp.probe(() => { const c = __cogs.W.cogs.find(c => c.id === 8); c.x += 40; c.y -= 25; __cogs.solve(); __cogs.commit(); });
  const st1 = await seat();
  is('move the disc: the reader comes with it, same radius and span', [st1.on, st1.f, st1.ang], [8, st0.f, st0.ang]);
  await tp.probe(() => { const c = __cogs.W.cogs.find(c => c.id === 8); c.n = 72; __cogs.solve(); __cogs.commit(); });
  is('resize the disc: the reader scales with it', (await seat()).f, st0.f);
  await tp.probe(() => __cogs.undo()); await tp.probe(() => __cogs.undo());
  await tp.dbl(await tp.empty()); await wait(500);
  const rpt = () => tp.probe(() => { const r = __cogs.W.readers.find(r => r.id === 9), c = __cogs.W.cogs.find(c => c.id === 8), m = __cogs.toScreen({ x: (r.x1 * .3 + r.x2 * .7), y: (r.y1 * .3 + r.y2 * .7) }), s = __cogs.toScreen(c), R = __cogs.radius(c) * __cogs.S.zoom; return { m: [m.x, m.y], c: [s.x, s.y], R }; });
  const g1 = await rpt(), around = k => [g1.c[0] + Math.cos(k) * g1.R * .7, g1.c[1] + Math.sin(k) * g1.R * .7];
  const k0 = Math.atan2(g1.m[1] - g1.c[1], g1.m[0] - g1.c[0]);
  await tp.stroke([g1.m, around(k0 + .4), around(k0 + .8)], { steps: 6 });
  const st2 = await seat();
  check('drag a seated reader: it slides around its disc and stays seated', st2.on === 8 && Math.abs(st2.ang - st0.ang) > .4, [st0.ang, st2.ang]);
  const g2 = await rpt();
  await tp.stroke([g2.m, [g2.c[0] + (g2.m[0] - g2.c[0]) * 3, g2.c[1] + (g2.m[1] - g2.c[1]) * 3]], { steps: 10 });
  is('pull it well past the rim: it comes free', (await seat()).on, null);
  await tp.probe(() => __cogs.undo()); await tp.probe(() => __cogs.undo());
  is('undo seats it again', (await seat()).on, 8);

  console.log('RATIO SNAP');
  const snapN = await tp.probe(() => { const { W, S } = __cogs; S.ratios = 1; const c = W.cogs.find(c => c.id === 1), d = W.cogs.find(c => c.id === 8);
    return [c.n, d.n]; });
  const rimR = await tp.probe(() => { const c = __cogs.W.cogs.find(c => c.id === 1), d = __cogs.W.cogs.find(c => c.id === 8), s = __cogs.toScreen(c), R = __cogs.radius(c) * __cogs.S.zoom, a = Math.atan2(c.y - d.y, c.x - d.x) + .9; return { p: [s.x + Math.cos(a) * R, s.y + Math.sin(a) * R], u: [Math.cos(a), Math.sin(a)] }; });
  await tp.stroke([rimR.p, [rimR.p[0] + rimR.u[0] * 9, rimR.p[1] + rimR.u[1] * 9]], { steps: 6 });
  const n1 = await tp.probe(() => __cogs.W.cogs.find(c => c.id === 1).n);
  check('ratio snap on: a pulled rim lands on a simple ratio to its neighbour', [1/4,1/3,1/2,2/3,3/4,1,4/3,3/2,2,3,4].some(r => Math.abs(n1 / snapN[1] - r) < 1e-9), [snapN, n1]);
  await tp.probe(() => { __cogs.S.ratios = 0; __cogs.undo(); });

  console.log('LISTENER');
  const mix = () => tp.probe(() => new Promise(r => setTimeout(() => r(__cogs.listener()), 700)));
  await tp.dbl(await tp.empty()); const near0 = await mix();
  await tp.pinch([195, 360], 240, 60); const far = await mix();
  check('zoomed out past the machine: quieter, darker, wetter', far.gain < near0.gain * .8 && far.cutoff < near0.cutoff / 3 && far.mix > near0.mix, [near0, far]);
  await tp.dbl(await tp.empty()); const full = await mix();
  check('back to the whole machine: the full mix', Math.abs(full.gain - near0.gain) < .02 && full.cutoff > 15000, full);

  console.log('NOTES SHEET');
  const hist = await tp.probe(() => ['#undo-all', '#redo-all'].map(s => { const b = document.querySelector(s), r = b.getBoundingClientRect(); return { at: [r.x + r.width / 2, r.y + r.height / 2], off: b.disabled, shown: r.width > 0 }; }));
  check('undo and redo sit in the thumb row, always shown', hist.every(h => h.shown && h.at[1] > 844 * .7), hist);
  await tp.probe(() => { __cogs.W.cogs[0].x += 12; __cogs.solve(); __cogs.commit(); });
  const wk0 = await tp.probe(() => __cogs.worldKey()), un = await tp.probe(() => __cogs.undos);
  await tp.tap(hist[0].at);
  check('the global undo steps the machine back', un > 0 && await tp.probe(() => __cogs.worldKey()) !== wk0, un);
  await tp.tap(hist[1].at);
  is('and redo steps it forward again', await tp.probe(() => __cogs.worldKey()) === wk0, true);
  is('the part buttons sit bottom left, level with set-to-start', await tp.probe(() => { const r = document.querySelector('#part-bar').getBoundingClientRect(), s = document.querySelector('#to-start').getBoundingClientRect(); return [r.left < 20, Math.abs(r.top - s.top) < 1.5]; }), [true, true]);
  const discAt = await tp.probe(() => { const c = __cogs.W.cogs.find(c => c.track), s = __cogs.toScreen(c), r = __cogs.radius(c) * __cogs.S.zoom; return { id: c.id, s: [s.x + r * .35, s.y - r * .35] }; });
  await tp.tap(await tp.empty()); await tp.tap(discAt.s);
  is('tap a disc: it is selected', await tp.probe(() => __cogs.sel && __cogs.sel.id), discAt.id);
  const ed = await centre('#edit-part');
  is('the notes button is a piano', await tp.probe(() => document.querySelector('#edit-part path').getAttribute('d').startsWith('M1 2.5')), true);
  check('its notes button is in thumb reach, at the bottom of the screen', ed && ed[1] > 844 * .7 && ed[0] < 80, ed);
  await tp.tap(ed);
  is('tap it: the notes sheet opens', await tp.probe(() => [document.querySelector('#dev').hidden, document.querySelector('#dev').dataset.editor]), [false, 'notes']);
  const trackOf = id => tp.probe(id => __cogs.W.cogs.find(c => c.id === id).track.dots.map(d => ({ ring: d.ring, len: d.len, a: +d.a.toFixed(5) })), id);
  await tp.probe(id => { const c = __cogs.W.cogs.find(c => c.id === id); c.track.dots = []; c.track.div = 16; }, discAt.id); await wait(150);
  const rg = await tp.probe(id => {
    const box = document.querySelector('#inspector .roll'); box.scrollLeft = box.scrollTop = 0;
    const cv = box.querySelectorAll('canvas')[1], r = cv.getBoundingClientRect();
    return { x: r.left + 28, y: r.top, cw: (r.width - 28) / 16, row: 32, rows: Math.round(r.height / 32) };
  }, discAt.id);
  const at = (col, row) => [rg.x + col * rg.cw, rg.y + (row + .5) * rg.row];
  await tp.tap(at(1.5, 0));
  is('tap a cell: one note, one step long', (await trackOf(discAt.id)).map(d => d.len), [1]);
  await tp.stroke([at(2 - 4 / rg.cw, 0), at(4 - 4 / rg.cw, 0)], { steps: 6 });
  const slid = await trackOf(discAt.id);
  check('drag the end of an unselected note: it moves, its length stays', slid.length === 1 && slid[0].len === 1, slid);
  await tp.tap(at(3.5, 0), 460);
  await tp.stroke([at(4, 0).map((v, i) => i ? v : v + 10), at(7, 0).map((v, i) => i ? v : v + 10)], { steps: 8 });
  await tp.shot('06-notes-grip');
  is('tap it to select, then drag its tab: three more steps', (await trackOf(discAt.id)).map(d => d.len), [4]);
  await tp.tap(at(9.5, 2)); await tp.tap(at(9.5, 2));
  const fresh = (await trackOf(discAt.id)).map(d => d.len).sort();
  is('the next new note takes the last touched length on this disc', fresh, [4, 4]);
  is('no brush length control', await tp.probe(() => !document.querySelector('[data-k="brush len"]') && !document.querySelector('.roll-hint')), true);
  await tp.stroke([at(0.5, 0), at(10.5, 2.4)], { steps: 8, hold: 550 });
  is('hold empty grid, then drag: a box selects both notes, none added', [await tp.probe(() => __cogs.rollSel), (await trackOf(discAt.id)).length], [2, 2]);
  const rbtn = name => tp.probe(n => { const b = document.querySelector(`.roll-sel [data-roll="${n}"]`), r = b.getBoundingClientRect(); return { at: [r.x + r.width / 2, r.y + r.height / 2], off: b.disabled }; }, name);
  await tp.tap((await rbtn('duplicate')).at);
  is('duplicate: four notes, the copies selected', [(await trackOf(discAt.id)).length, await tp.probe(() => __cogs.rollSel)], [4, 2]);
  await tp.tap((await rbtn('delete notes')).at);
  is('delete: back to two, nothing selected, the buttons rest', [(await trackOf(discAt.id)).length, await tp.probe(() => __cogs.rollSel), (await rbtn('delete notes')).off], [2, 0, true]);
  is('the velocity lane is hidden until its toggle shows it', await tp.probe(() => document.querySelector('#inspector .roll').querySelectorAll('canvas')[2].getBoundingClientRect().height), 0);
  await tp.tap((await rbtn('velocity')).at); await wait(100);
  const lane = await tp.probe(() => { const r = document.querySelector('#inspector .roll').querySelectorAll('canvas')[2].getBoundingClientRect(); return { x: r.left + 28, top: r.top, h: r.height }; });
  const lx = lane.x + 9.5 * rg.cw;
  await tp.p.evaluate(() => new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)))); await tp.stroke([[lx, lane.top + 16], [lx, lane.top + lane.h - 4]], { steps: 6 });
  const vels = (await tp.probe(id => __cogs.W.cogs.find(c => c.id === id).track.dots.map(d => [d.ring, d.v]), discAt.id)).sort();
  check('drag down the velocity lane under a note: only that note goes soft', vels.length === 2 && vels.find(v => v[0] === 2)[1] < .2 && vels.find(v => v[0] === 4)[1] === 1, vels);
  await tp.shot('08-notes-lane');
  const n0s = (await trackOf(discAt.id)).length, sl0 = await tp.probe(() => document.querySelector('#inspector .roll').scrollLeft);
  await tp.stroke([at(11.5, 1), at(7.5, 1)], { steps: 6 });
  const sl1 = await tp.probe(() => document.querySelector('#inspector .roll').scrollLeft);
  check('one finger slides on empty grid: the strip pans, no note', sl1 > sl0 + 40 && (await trackOf(discAt.id)).length === n0s, [sl0, sl1]);
  await tp.probe(() => { document.querySelector('#inspector .roll').scrollLeft = 0; });
  is('the chrome icons are Radix', await tp.probe(() => ['#delete-part', '#to-start', '#editor-undo', '#editor-close'].map(s => document.querySelector(s + ' svg')?.getAttribute('viewBox'))), ['0 0 15 15', '0 0 15 15', '0 0 15 15', '0 0 15 15']);

  const sheetRect = () => tp.probe(() => { const d = document.querySelector('#dev'), r = d.getBoundingClientRect(); return { top: Math.round(r.top), min: d.classList.contains('min'), hidden: d.hidden, scrim: document.querySelector('#scrim').hidden }; });
  const grab = await centre('#grab'), open0 = await sheetRect();
  await tp.stroke([grab, [grab[0], grab[1] + 320]], { steps: 10 }); await wait(400);
  const tucked = await sheetRect();
  check('drag the grab bar down: the sheet tucks to a strip, the machine is free', tucked.min && !tucked.hidden && tucked.scrim && tucked.top > 844 - 160, tucked);
  await tp.shot('07-notes-tucked');
  check('tucked: the notes button shows again', !!(await centre('#edit-part')), null);
  const paused0 = await tp.probe(() => __cogs.clock.paused); await tp.glyph();
  is('tucked: the play glyph still answers', await tp.probe(() => __cogs.clock.paused), !paused0);
  await tp.glyph();
  await tp.tap(await centre('#grab')); await wait(400);
  const back = await sheetRect();
  check('tap the grab bar: the sheet comes back up', !back.min && !back.hidden && Math.abs(back.top - open0.top) < 2, [open0, back]);
  const grab2 = await centre('#grab');
  await tp.stroke([grab2, [grab2[0], grab2[1] - 30]], { steps: 4 }); await wait(400);
  is('a short drag up leaves it up', (await sheetRect()).min, false);
  await tp.stroke([grab2, [grab2[0], grab2[1] + 320]], { steps: 10 }); await wait(400);
  const grab3 = await centre('#grab');
  await tp.stroke([grab3, [grab3[0], grab3[1] + 80]], { steps: 6 }); await wait(400);
  is('drag down again from the strip: the sheet closes', (await sheetRect()).hidden, true);
  await tp.dbl(await tp.empty()); await wait(500);
  const disc2 = await tp.probe(id => { const c = __cogs.W.cogs.find(c => c.track && c.id !== id), s = __cogs.toScreen(c), r = __cogs.radius(c) * __cogs.S.zoom; c.track.dots = []; c.track.div = 16; return { id: c.id, s: [s.x + r * .35, s.y - r * .35] }; }, discAt.id);
  for (let k = 0; k < 8 && await tp.probe(() => __cogs.sel && __cogs.sel.id) !== disc2.id; k++) {
    const pt = await tp.probe(([id, k]) => { const c = __cogs.W.cogs.find(c => c.id === id), s = __cogs.toScreen(c), r = __cogs.radius(c) * __cogs.S.zoom * .55, a = k * Math.PI / 4; return [s.x + Math.cos(a) * r, s.y + Math.sin(a) * r]; }, [disc2.id, k]);
    if (pt[0] < 10 || pt[0] > 380 || pt[1] < 60 || pt[1] > 640) continue;
    await tp.tap(await tp.empty()); await tp.tap(pt);
  }
  await tp.tap(await centre('#edit-part')); await wait(150);
  const rg2 = await tp.probe(() => { const r = document.querySelector('#inspector .roll').querySelectorAll('canvas')[1].getBoundingClientRect(); return { x: r.left + 28, y: r.top, cw: (r.width - 28) / 16 }; });
  await tp.tap([rg2.x + 1.5 * rg2.cw, rg2.y + 16]);
  is('another disc keeps its own pen: its first note is one step', [await tp.probe(() => __cogs.sel && __cogs.sel.id), (await trackOf(disc2.id)).map(d => d.len)], [disc2.id, [1]]);
  await tp.tap(await centre('#editor-close'));
  check('page errors', m.errs.length === 0 && tp.errs.length === 0, [...m.errs, ...tp.errs].slice(0, 3));

  console.log('BELTS, TEMPO, STRINGS, COMB, PROBABILITY, EUCLID, SHARE');
  await m.act('clear'); await m.world('zoom', 1.4); await m.dev(false);
  const rpms = () => probe(() => __cogs.W.cogs.map(c => +(c.w * 60 / (2 * Math.PI)).toFixed(6)));
  await m.fromShelf('motor', [-100, 0]); await m.drag([-100 + 22.92, 0], [-54, 0]);
  await m.tap(await m.scr([-110, 15])); await m.field('rpm', 60); await m.dev(false);
  await m.fromShelf('cog', [100, 0]);
  await m.fromShelf('belt', [-100, 20], [100, 5]);
  check('open belt: same direction, ratio 24/12', JSON.stringify(await rpms()) === '[60,120]', await rpms());
  const beltMid = await probe(() => { const l = __cogs.beltLines(__cogs.W.belts[0])[0], s = __cogs.toScreen({ x: (l[0] + l[2]) / 2, y: (l[1] + l[3]) / 2 }); return [s.x, s.y]; });
  await m.tap(beltMid);
  check('tap the belt: crossed, it reverses', JSON.stringify(await rpms()) === '[60,-120]', await rpms());
  await m.world('bpm lock', 240); await m.dev(false);
  await m.tap(await m.scr([-110, 15])); await m.pick('revs/bar', '1/2'); await m.dev(false);
  check('240 bpm, 4/4, 1/2 rev per bar = 30 rpm', JSON.stringify(await rpms()) === '[30,-60]', await rpms());
  await m.world('zoom', 1); await m.dev(false);
  await m.glyph();
  await m.fromShelf('string', [-100, -40]);
  await m.stroke([await m.scr([-160, -40]), await m.scr([-170, -60]), await m.scr([-180, -40])]);
  await m.stroke([await m.scr([-40, -40]), await m.scr([-30, -60]), await m.scr([-20, -40])]);
  await m.fromShelf('arm', [-100, -20]);
  await m.drag([-100, -20], [-100, -60]);
  await m.tap(await m.scr([-150, -40]));
  is('tap a string: its bezel', await probe(() => __cogs.activeBezel()?.sectors.length), 8);
  await m.shot('10-string-belt');
  await m.dev(true); await wait(300);
  const note = await probe(() => document.querySelector('#inspector [data-k="note"] b').textContent);
  check('160 px string at 220 Hz/200 px quantizes to C#4', note.startsWith('C#4 61'), note);
  await m.dev(false); await m.glyph();
  const sk0 = (await probe(() => __cogs.engine())).strikes || 0;
  const s0 = await m.reset(); await wait(2300);
  const sHits = await m.count('>string', s0, 2);
  const st = await probe(() => {
    const s = __cogs.W.strings[0], c = __cogs.W.cogs[0], a = __cogs.W.arms[0], L = a.r0 + a.len, h = Math.abs(s.y1 - c.y), dx = Math.sqrt(L * L - h * h);
    return { u: s.lastU, us: [c.x - dx, c.x + dx].map(x => (x - s.x1) / (s.x2 - s.x1)) };
  });
  check('tip crosses the string twice per revolution', sHits === 2, sHits);
  check('pluck point is where the tip circle cuts the string', st.us.some(u => near(st.u, u, 1e-3)), [st.u, st.us]);
  await wait(300);
  check('plucks reach the engine', ((await probe(() => __cogs.engine())).strikes || 0) > sk0, sk0);
  await m.fromShelf('disc', [0, 150]); await m.fromShelf('motor', [60 * 12 / (2 * Math.PI), 150]);
  await m.tap(await m.scr([30, 190])); await m.pick('revs/bar', '1'); await m.dev(false);
  await m.glyph();
  await m.dbl(await m.scr([0, 180]));
  const r2 = await probe(() => { const c = __cogs.W.cogs[2]; return __cogs.ringR(c, 2); });
  const onRing = k => m.scr([r2 * Math.cos(Math.PI / 2 + k * Math.PI / 8), 150 + r2 * Math.sin(Math.PI / 2 + k * Math.PI / 8)]);
  await m.stroke([await onRing(0), await onRing(.5), await onRing(1), await onRing(1.5), await onRing(2), await onRing(2.6)], { steps: 4, hold: 600 });
  const eu = await probe(() => { const t = __cogs.W.cogs[2].track; return t.dots.map(d => [d.ring, Math.round(d.a / (2 * Math.PI) * t.div)]); });
  check('hold an empty socket on ring 2, sweep 3 steps: euclid 3 in 16', eu.length === 3 && eu.every(([r]) => r === 2), eu);
  await m.shot('11-euclid');
  const pin = eu[0], pinS = await probe(([ring, k]) => { const c = __cogs.W.cogs[2], R = __cogs.ringR(c, ring), a = k * 2 * Math.PI / c.track.div + c.th - c.w * (__cogs.simT - __cogs.clock.view); const s = __cogs.toScreen({ x: c.x + Math.cos(a) * R, y: c.y + Math.sin(a) * R }); return [s.x, s.y]; }, pin);
  await m.stroke([pinS, [pinS[0] + .5, pinS[1]]], { steps: 1, hold: 600 });
  const ob = await probe(() => { const b = __cogs.activeBezel(); return b && { cx: b.cx, cy: b.cy, R: b.R, n: b.sectors.length, sel: __cogs.sel && __cogs.sel.type }; });
  check('hold a pin: its odds, velocity and length bezel', ob && ob.n === 3 && ob.sel === 'dot', ob);
  await m.shot('12-pin-odds');
  await up(sector(ob, 0, .5), -60);
  const odds = await probe(() => __cogs.W.cogs[2].track.dots.map(d => +d.p.toFixed(2)));
  check('drag down from the odds sector: that pin plays less often', odds.filter(x => x < 1).length === 1, odds);
  await m.pinch([195, 420], 200, 80);
  await m.glyph();
  await m.fromShelf('reader', [0, 105]);
  const rr = await probe(() => { const r = __cogs.W.readers[0]; return [r.x1, r.y1, r.x2, r.y2]; });
  await m.stroke([await m.scr([rr[0], rr[1]]), await m.scr([rr[0] + 10, 140]), await m.scr([0, 145])]);
  await m.stroke([await m.scr([rr[2], rr[3]]), await m.scr([rr[2] + 10, 70]), await m.scr([0, 60])]);
  await m.tap(await m.scr([0, 100])); await m.pick('reader mode', 'comb'); await m.field('comb tines', 4); await m.dev(false);
  await m.tap(await m.empty()); await m.tap(await m.scr([30, 190]));
  await m.dev(true); await p.fill('#inspector [data-k="brush p"] input[type=number]', '1');
  await p.fill('#inspector [data-k="euclid ring"] input[type=number]', '2'); await p.fill('#inspector [data-k="euclid k"] input[type=number]', '3'); await p.tap('text=euclid fill');
  const c0t = await m.reset(); await wait(2300);
  const rdr = { hits: await m.count('>reader', c0t, 2), midi: await probe(() => __cogs.W.readers[0].lastMidi) };
  check('reader: 3 dots x 2 revolutions', rdr.hits === 6, rdr.hits);
  check('comb: ring 2 crosses tine 1 of 4 -> D4 (62)', rdr.midi === 62, rdr.midi);
  await m.dev(true); await p.fill('#inspector [data-k="brush p"] input[type=number]', '0'); await p.tap('text=euclid fill');
  const z0 = await m.reset(); await wait(1700);
  const silent = await m.count('>reader', z0, 1.5);
  check('probability 0: silent', silent === 0, silent);
  await m.dev(true); await p.fill('#inspector [data-k="brush p"] input[type=number]', '0.5'); await p.tap('text=euclid fill');
  const q0 = await m.reset(); await wait(4300);
  const half = await m.count('>reader', q0, 4);
  check('probability 0.5: some of 12 play', half > 0 && half < 12, half);
  await m.dev(false); await m.room();
  const room2 = await probe(() => { const b = __cogs.activeBezel(); return b && { cx: b.cx, cy: b.cy, R: b.R, n: b.sectors.length }; });
  await m.tap(sector(room2, 10, .5)); await wait(400);
  const url = await probe(() => navigator.clipboard.readText());
  check('share yields a #w= url', /#w=[\w-]+$/.test(url), url.slice(0, 50) + '…');
  const sp = await (await b.newContext(PHONE)).newPage();
  await sp.goto(url); await wait(600);
  const counts = o => [o.cogs.length, o.arms.length, o.belts.length, o.strings.length, o.readers.length, o.cogs[2].track.dots.length];
  const here = counts(await probe(() => __cogs.serialize())), there = counts(await sp.evaluate(() => __cogs.serialize()));
  check('shared url rebuilds the same world', JSON.stringify(here) === JSON.stringify(there), [here, there]);

  console.log('DESKTOP: KEYS, MARQUEE, GROUP, WHEEL');
  const dk = await b.newContext({ viewport: { width: 1200, height: 800 }, deviceScaleFactor: SHOTS ? 2 : 1 });
  const d = await dk.newPage(), derrs = [];
  d.on('pageerror', e => derrs.push(e.message));
  await d.goto(FILE); await wait(500);
  const dscr = ([x, y]) => d.evaluate(([x, y]) => { const s = __cogs.toScreen({ x, y }); return [s.x, s.y]; }, [x, y]);
  const click = async (w, mods = []) => { for (const k of mods) await d.keyboard.down(k); await d.mouse.click(...await dscr(w)); for (const k of mods) await d.keyboard.up(k); await wait(450); };
  const ddrag = async (a, z, mods = []) => {
    for (const k of mods) await d.keyboard.down(k);
    await d.mouse.move(...await dscr(a)); await d.mouse.down(); await d.mouse.move(...await dscr(z), { steps: 10 }); await d.mouse.up();
    for (const k of mods) await d.keyboard.up(k); await wait(150);
  };
  if (SHOTS) await d.screenshot({ path: path.join(SHOTS, '15-desktop-demo.png') });
  await d.keyboard.press('?'); await d.evaluate(() => { document.querySelector('#panel details').open = true; }); await d.click('button[data-act=clear]');
  await d.fill('#panel [data-k="zoom"] input[type=number]', '1'); await d.click('button[data-act=close]'); await wait(200);
  const keyed = [];
  for (const k of ['c', 'd', 'a', 'b', 'p', 't', 'r', 's', 'c', 'Escape']) { await d.keyboard.press(k); keyed.push(await d.evaluate(() => __cogs.armed)); }
  is('one key per part; s and Esc disarm', keyed, ['cog', 'disc', 'arm', 'belt', 'pad', 'string', 'reader', null, 'cog', null]);
  await d.keyboard.press('c'); await click([-100, 0]); await click([100, 0]);
  await d.keyboard.press('p'); await click([0, -100]); await d.keyboard.press('s');
  const built2 = await d.evaluate(() => [__cogs.W.cogs.map(c => [c.x, c.y]), __cogs.W.pads.map(o => [o.x, o.y])]);
  check('armed key + click places a part where clicked', JSON.stringify(built2) === '[[[-100,0],[100,0]],[[0,-100]]]', built2);
  await ddrag([-160, -40], [160, 40], ['Control']);
  is('ctrl-drag box takes the two cogs, not the pad', await d.evaluate(() => __cogs.group), ['cog', 'cog']);
  await ddrag([-100, 0], [-100, 40]);
  const moved = await d.evaluate(() => [__cogs.W.cogs.map(c => c.y), __cogs.W.pads[0].y]);
  is('dragging a member moves the whole group', moved, [[40, 40], -100]);
  await click([100, 40], ['Control']);
  is('ctrl-click toggles a cog out', await d.evaluate(() => __cogs.group), ['cog']);
  await d.keyboard.press('Delete');
  is('Delete removes the group', await d.evaluate(() => [__cogs.W.cogs.map(c => c.x), __cogs.W.pads.length]), [[100], 1]);
  const under = at => d.evaluate(([x, y]) => { const r = document.querySelector('#c').getBoundingClientRect(), v = __cogs.serialize().view; return [(x - r.left - r.width / 2) / __cogs.S.zoom + v.x, (y - r.top - r.height / 2) / __cogs.S.zoom + v.y]; }, at);
  const wAt = [840, 260], z0w = await d.evaluate(() => __cogs.S.zoom), u0 = await under(wAt);
  await d.mouse.move(...wAt); await d.mouse.wheel(0, -100); await wait(200);
  const z1w = await d.evaluate(() => __cogs.S.zoom), u1 = await under(wAt);
  check('wheel up zooms in', z1w > z0w * 1.1, [z0w, z1w]);
  check('point under the cursor stays put', near(u0[0], u1[0], .5) && near(u0[1], u1[1], .5), [u0, u1]);
  check('desktop page errors', derrs.length === 0, derrs.slice(0, 3));

  if (SHOTS) { await m.world('zoom', 1); await m.dev(false); await m.dbl(await m.empty()); await m.shot('13-alpha-machine'); await tp.shot('14-pinched'); }
  check('page errors', m.errs.length === 0 && tp.errs.length === 0, [...m.errs, ...tp.errs].slice(0, 3));
  console.log(pass ? '\nCOGS: PASS' : '\nCOGS: FAIL');
  await b.close(); process.exit(pass ? 0 : 1);
})();
