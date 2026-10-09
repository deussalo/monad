// Builds contraptions by touch on a 390x844 phone viewport and checks the mechanism and the audio it drives.
const { chromium } = require('/srv/rig/monad/node_modules/playwright-core');
const path = require('path');
const FILE = process.argv[2] || ('file://' + path.join(__dirname, 'cogs.html'));
const SHOTS = process.env.COGS_SHOTS || '';
const PHONE = { viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true, deviceScaleFactor: SHOTS ? 2 : 1 };
(async () => {
  const b = await chromium.launch({ executablePath: (process.env.MONAD_CHROME || '/srv/rig/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome'),
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
      async glyph() { await t.tap(await probe(() => { const g = __cogs.glyphAt(); return [g.x, g.y]; })); },
      async dev(open) {
        if (open === await probe(() => !document.querySelector('#dev').hidden)) return;
        if (open) { const g = await probe(() => { const g = __cogs.glyphAt(); return [g.x, g.y]; }); await T('touchStart', [g]); await wait(750); await T('touchEnd', []); await wait(100); }
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
        for (let sy = 90; sy < shelfTop() - 30; sy += 20) for (let sx = 30; sx < innerWidth - 30; sx += 20) if (ok(sx, sy)) return [sx, sy];
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
  await m.act('clear'); await m.world('zoom', 1.6); await m.dev(false);

  console.log('GEARS');
  await m.stroke([await m.shelf('cog'), await m.scr([30, 60]), await m.scr([0, 0])]);
  await m.drag([22.92, 0], [46, 0]);
  await m.stroke([await m.shelf('cog'), await m.scr([70, 0])], { mid: () => m.shot('02-carry-snap') });
  const gears = await probe(() => __cogs.W.cogs.map(c => ({ n: c.n, x: c.x, y: c.y, rpm: c.w * 60 / (2 * Math.PI), jam: c.jam })));
  check('rim pulled out to 46: 24 teeth; a fresh cog: 12', gears[0].n === 24 && gears[1].n === 12, gears.map(g => g.n));
  check('second cog snapped to pitch distance 68.75', near(Math.hypot(gears[1].x - gears[0].x, gears[1].y - gears[0].y), 36 * 12 / (2 * Math.PI), 1e-9), gears[1].x - gears[0].x);
  const [c1, c2] = gears.map(g => [g.x, g.y]);
  await m.tap(await m.scr([c1[0] - 20, c1[1] - 15])); await m.field('motor rpm', 60); await m.dev(false);
  const rpm = () => probe(() => __cogs.W.cogs.map(c => +(c.w * 60 / (2 * Math.PI)).toFixed(6)));
  check('60 rpm drives -120 rpm', JSON.stringify(await rpm()) === '[60,-120]', await rpm());

  console.log('CRANK');
  const top2 = [c2[0], c2[1] - 22.92];
  await m.stroke([await m.scr(top2), await m.scr([c2[0] + 8, top2[1]]), await m.scr([c2[0] + 22, top2[1] + 8])], { steps: 5, frame: 12 });
  const crank = await probe(() => __cogs.W.cogs.map(c => ({ motor: c.motor, rpm: c.w * 60 / (2 * Math.PI) })));
  const DET = [5, 7.5, 10, 12, 15, 20, 24, 30, 40, 45, 60, 72, 80, 90, 120, 144, 160, 180, 240, 300];
  check('flick the teeth clockwise: that cog drives at a detent', crank[0].motor === 0 && crank[1].motor > 0 && DET.includes(crank[1].motor), crank.map(c => c.motor));
  check('the train follows the new driver', near(crank[0].rpm, -crank[1].motor / 2, 1e-9), crank.map(c => c.rpm));
  await m.stroke([await m.scr(top2), await m.scr([c2[0] + 12, top2[1]])], { steps: 4, frame: 10, hold: 0 });
  await m.stroke([await m.scr(top2), await m.scr([c2[0] + 12, top2[1]]), await m.scr([c2[0] + 12.1, top2[1]])], { steps: 3, frame: 90 });
  check('crank, hold still, let go: the train stops', JSON.stringify(await rpm()) === '[0,0]', await rpm());
  if (SHOTS) await m.stroke([await m.scr(top2), await m.scr([c2[0] + 14, top2[1] + 4])], { steps: 6, frame: 8, mid: () => m.shot('03-crank') });
  await m.tap(await m.scr([c1[0] - 20, c1[1] - 15])); await m.field('motor rpm', 60); await m.dev(false);

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

  console.log('JAM');
  await m.tap(await m.empty());
  await m.tap(await m.scr([c2[0] - 12, c2[1]])); await m.field('motor rpm', 100); await m.dev(false); await wait(150);
  const jam = await probe(() => __cogs.W.cogs.map(c => [c.jam, c.w]));
  check('two disagreeing motors jam the train', jam.every(([j, w]) => j && w === 0), jam);
  await m.shot('05-jam');
  await m.field('motor rpm', -120);
  const ok = await probe(() => __cogs.W.cogs.map(c => [c.jam, c.w * 60 / (2 * Math.PI)]));
  check('agreeing motors turn', ok.every(([j]) => !j) && near(ok[1][1], -120, 1e-9), ok);
  await m.field('motor rpm', 0); await m.dev(false);

  console.log('PAD BEZEL');
  await m.tap(await m.scr([-60, -30]));
  const bz = await probe(() => { const b = __cogs.activeBezel(); return b && { cx: b.cx, cy: b.cy, R: b.R, n: b.sectors.length }; });
  check('tap a pad: its bezel blooms, one sector per kick param', bz && bz.n === 7, bz);
  await m.shot('06-pad-bezel');
  const sector = (b, i, f) => { const a = -Math.PI / 2 + (i + f) * 2 * Math.PI / b.n; return [b.cx + Math.cos(a) * b.R, b.cy + Math.sin(a) * b.R]; };
  const tune0 = await probe(() => __cogs.W.pads[0].voice.tune);
  await m.stroke([sector(bz, 1, .2), sector(bz, 1, .5), sector(bz, 1, .8), sector(bz, 1, 1.2)]);
  const tune1 = await probe(() => __cogs.W.pads[0].voice.tune);
  check('drag clockwise round the tune sector raises the tune', tune1 > tune0, [tune0, tune1]);
  await m.tap(sector(bz, 0, .5));
  is('tap the kind sector: kick becomes snare', await probe(() => __cogs.W.pads[0].voice.kind), 'snare');
  await m.tap(await m.empty());
  is('tap empty ground: deselect, bezel gone', await probe(() => !__cogs.sel && !__cogs.activeBezel()), true);

  console.log('DISC AND READER');
  await m.fromShelf('disc', [0, 170]);
  const discXY = await probe(() => { const c = __cogs.W.cogs[2]; return [c.x, c.y, c.n]; });
  check('disc dropped clear of the train, 48 teeth', discXY[2] === 48 && near(discXY[1], 170, 1), discXY);
  await m.dbl(await m.scr([discXY[0] - 20, discXY[1] - 20]));
  is('double tap the disc: flies in until its face is the score', await probe(() => { const c = __cogs.W.cogs[2]; return c.track.rings > 1 && __cogs.radius(c) * .65 / 11 * __cogs.S.zoom >= 12; }), true);
  for (const [dx, dy] of [[40, 0], [0, -70], [-80, 0]]) await m.tap(await m.scr([discXY[0] + dx, discXY[1] + dy]));
  is('three taps set three pins', await probe(() => __cogs.W.cogs[2].track.dots.length), 3);
  await m.shot('07-disc-notes');
  await m.dbl(await m.empty()); await wait(200);
  await m.fromShelf('reader', [discXY[0], discXY[1] - 45]);
  const rd0 = await probe(() => { const r = __cogs.W.readers[0]; return r && [r.x1, r.y1, r.x2, r.y2].map(v => +v.toFixed(1)); });
  check('reader dropped on the disc lies along its radius', rd0 && near(rd0[0], discXY[0], 1e-6) && near(rd0[2], discXY[0], 1e-6) && rd0[1] > rd0[3], rd0);
  await m.tap(await m.scr([discXY[0] - 60, discXY[1] + 20]));
  await m.shot('08-disc-bezel');
  is('tap the disc: its tuning bezel (scale, root, divisions, rings)', await probe(() => __cogs.activeBezel()?.sectors.length), 4);
  await m.field('motor rpm', 30); await m.dev(false);
  const disc = await probe(() => { const c = __cogs.W.cogs[2]; return { n: c.n, dots: c.track.dots.length, rpm: c.w * 60 / (2 * Math.PI), strikes: __cogs.engine().strikes }; });
  check('disc: 48 teeth, 3 dots, 30 rpm', disc.n === 48 && disc.dots === 3 && near(disc.rpm, 30, 1e-9), disc);
  const d0 = await m.reset(); await wait(4300);
  const rd = await probe(() => ({ hits: 0, rings: [...new Set(__cogs.stats.log.filter(l => l.src.includes('>reader')).map(l => l.src.split('>')[0]))], eng: __cogs.engine() }));
  rd.hits = await m.count('>reader', d0, 4);
  check('reader: 3 dots x 2 revolutions', rd.hits === 6, rd.hits);
  check('three distinct rings sounded', rd.rings.length === 3, rd.rings);
  check('engine consumed the modal strikes', rd.eng.strikes - disc.strikes >= rd.hits - 1 && rd.eng.drops === 0, { strikes: rd.eng.strikes - disc.strikes, drops: rd.eng.drops });
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
  await m.tap(cell(2, rows - 1));
  check('second tap removes it', (await dots()).length === n0, (await dots()).length - n0);
  await m.stroke([cell(0, rows - 6), cell(3, rows - 6)], { steps: 9 });
  const painted = (await dots()).filter(d => d[0] === 5);
  check('drag paints 4 cells on ring 5', painted.length === 4, painted);
  const p5 = await m.reset(); await wait(2300);
  const r5 = await m.count('.ring5>reader', p5, 2);
  check('painted ring 5: 4 dots in one 2 s revolution', r5 === 4, r5);

  console.log('ROOM');
  await m.dev(false); await m.tap(await m.empty()); await m.world('zoom', .2); await m.dev(false); await wait(250);
  const room = await probe(() => { const b = __cogs.activeBezel(); return b && { cx: b.cx, cy: b.cy, R: b.R, n: b.sectors.length }; });
  check('zoom out past the machine: the room bezel', room && room.n === 10, room);
  await m.shot('09-room');
  const wet0 = await probe(() => __cogs.SPACE.wet);
  await m.stroke([sector(room, 0, .8), sector(room, 0, .5), sector(room, 0, .2)]);
  const wet1 = await probe(() => __cogs.SPACE.wet);
  check('drag the wet sector anticlockwise: drier room', wet1 < wet0, [wet0, wet1]);
  await m.tap(sector(room, 9, .5)); await wait(400);
  const clip = await probe(() => navigator.clipboard.readText().catch(e => 'clipboard: ' + e.name));
  check('tap the seal: a #w= link reaches the clipboard', /#w=[\w-]+$/.test(clip), clip.slice(0, 40) + '…');

  console.log('PAUSE AND PERSIST');
  await m.glyph(); const before = await probe(() => __cogs.stats.events);
  await wait(1000); const after = await probe(() => __cogs.stats.events);
  check('paused: no events', after - before <= 1, after - before);
  await m.glyph();
  await wait(600); await p.reload(); await wait(400);
  const world = await probe(() => __cogs.W.cogs.length + ' ' + __cogs.W.arms.length + ' ' + __cogs.W.pads.length + ' ' + __cogs.W.readers.length);
  check('world restored after reload', world === '3 2 2 1', world);

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
  const pad = await tp.probe(() => { const o = __cogs.W.pads[1], s = __cogs.toScreen(o); return { id: o.id, s: [s.x, s.y], n: __cogs.W.pads.length }; });
  await tp.stroke([pad.s, [pad.s[0], 700], await tp.shelf('pad')]);
  is('drag a pad onto the shelf: gone', await tp.probe(id => [__cogs.W.pads.every(p => p.id !== id), __cogs.W.pads.length], pad.id), [true, pad.n - 1]);
  check('page errors', m.errs.length === 0 && tp.errs.length === 0, [...m.errs, ...tp.errs].slice(0, 3));

  console.log('BELTS, TEMPO, STRINGS, COMB, PROBABILITY, EUCLID, SHARE');
  await m.act('clear'); await m.world('zoom', 1.4); await m.dev(false);
  const rpms = () => probe(() => __cogs.W.cogs.map(c => +(c.w * 60 / (2 * Math.PI)).toFixed(6)));
  await m.fromShelf('cog', [-100, 0]); await m.drag([-100 + 22.92, 0], [-54, 0]);
  await m.tap(await m.scr([-110, 15])); await m.field('motor rpm', 60); await m.dev(false);
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
  await m.fromShelf('disc', [0, 150]);
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
  check('hold a pin: its odds bezel', ob && ob.n === 2 && ob.sel === 'dot', ob);
  await m.shot('12-pin-odds');
  await m.stroke([sector(ob, 0, .7), sector(ob, 0, .45), sector(ob, 0, .2)]);
  const odds = await probe(() => __cogs.W.cogs[2].track.dots.map(d => +d.p.toFixed(2)));
  check('drag the odds sector back: that pin plays less often', odds.filter(x => x < 1).length === 1, odds);
  await m.pinch([195, 420], 200, 80);
  await m.glyph();
  await m.fromShelf('reader', [0, 105]);
  const rr = await probe(() => { const r = __cogs.W.readers[0]; return [r.x1, r.y1, r.x2, r.y2]; });
  await m.stroke([await m.scr([rr[0], rr[1]]), await m.scr([rr[0] + 10, 140]), await m.scr([0, 145])]);
  await m.stroke([await m.scr([rr[2], rr[3]]), await m.scr([rr[2] + 10, 70]), await m.scr([0, 60])]);
  await m.tap(await m.scr([0, 100])); await m.field('comb tines', 4); await m.dev(false);
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
  await m.dev(false); await m.tap(await m.empty()); await m.world('zoom', .12); await m.dev(false); await wait(250);
  const room2 = await probe(() => { const b = __cogs.activeBezel(); return b && { cx: b.cx, cy: b.cy, R: b.R, n: b.sectors.length }; });
  await m.tap(sector(room2, 9, .5)); await wait(400);
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
  await d.keyboard.press('?'); await d.click('button[data-act=clear]');
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
