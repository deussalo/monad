// The probe: placing it on what moves (arm, cog, cam, ball), what it reads, driving a synth's macro through the
// worklet and a drum's parameter at each strike, its dial rows, saving, and losing its host.
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
  const settle = async () => { await p.waitForFunction(() => !__cogs.pending); await wait(50); };
  await p.goto(FILE); await wait(500);
  await p.mouse.click(40, 860); await wait(300);
  await p.waitForFunction(() => __cogs.engine().state === 'ready' && __cogs.engine().ac === 'running', null, { timeout: 5000 });

  // a motor with an arm pointing right, a synth reader, a kick pad, a cam on a second cog, a box with a ball
  await probe(() => { setPaused(true); const j = __cogs.serialize();
    Object.assign(j, { cogs: [{ id: 1, x: 0, y: 0, n: 24, motor: { rpm: 60, revs: 0 }, layer: 0, th: 0, track: null }], arms: [{ id: 2, cog: 1, a: 0, r0: 0, len: 60, flex: .5 }],
      pads: [{ id: 3, x: -150, y: 120, r: 12, voice: { kind: 'snare', level: .5, velSense: .5 } }],
      readers: [{ id: 4, on: null, x1: 150, y1: -40, x2: 150, y2: 40, voice: { kind: 'wave', preset: 'pluck', note: 60, level: .7, velSense: .7 }, tines: 0, combBase: 60, combScale: 'major' }],
      belts: [], strings: [], cams: [], pits: [], probes: [], view: { x: 0, y: 0 } });
    j.S.zoom = 1.5; __cogs.deserialize(j); __cogs.commit(); });
  await wait(100);

  console.log('PLACING');
  const tip = await probe(() => { const t = armTip(W.arms[0]); return [t.x, t.y]; });
  await p.keyboard.press('y'); await p.mouse.click(...await scr([tip[0] * .5, tip[1] * .5])); await settle(); await p.keyboard.press('Escape');
  const made = await probe(() => ({ n: W.probes.length, sel: __cogs.sel?.type, host: W.probes[0]?.host, on: W.probes[0]?.on.id, r: +W.probes[0]?.r.toFixed(1), axis: W.probes[0]?.axis,
    rows: [...document.querySelectorAll('#inspector .row, #inspector .ro')].map(r => r.dataset.k) }));
  check('key y, then a click halfway along an arm: a probe pins there, reading up–down, selected', made.n === 1 && made.sel === 'probe' && made.host === 'arm' && made.on === 2 && Math.abs(made.r - 30) < 2 && made.axis === 'y', made);
  is('its sheet: what it rides, axis, what it drives, low, high, value', made.rows, ['rides', 'axis', 'drives', 'low', 'high', 'value']);
  const tab = await probe(() => { const pr = W.probes[0], t = probeTab(pr); return { n: t.length, lo: Math.min(...t), hi: Math.max(...t), at0: t[0], quarter: t[96] }; });
  // th = 0: the arm points right (mid height); th = 3π/2: it points up (screen y is down), the top of its travel
  check('one turn as a table: full travel 0..1, mid height pointing right, top pointing up', tab.n === 128 && tab.lo === 0 && tab.hi === 1 && Math.abs(tab.at0 - .5) < 1e-3 && Math.abs(tab.quarter - 1) < 1e-3, tab);
  const near = await probe(() => { const pr = W.probes[0], q = probePoint(pr), c = W.cogs[0], th = renderTh(c); return { d: Math.hypot(q.x - (c.x + Math.cos(th) * 30), q.y - (c.y + Math.sin(th) * 30)) }; });
  check('it rides the arm: drawn on the arm, 30 px out', near.d < 1e-3, near);

  console.log('A SYNTH');
  await probe(() => { window.__motions = []; const port = waveNode.port, post = port.postMessage.bind(port); port.postMessage = (m, t) => { if (m.type === 'motion') __motions.push({ ...m, prog: m.prog && { ...m.prog, table: [...m.prog.table] } }); return post(m, t); }; setPaused(false); });
  await p.selectOption('#inspector .row[data-k="drives"] select', 'reader:4'); await settle(); await wait(200);
  const link = await probe(() => ({ to: __cogs.serialize().probes[0].to, macro: W.probes[0].macro, sel: __cogs.sel, arm: waveUi.arm, last: __motions.filter(m => m.prog).at(-1), rps: W.cogs[0].w / (2 * Math.PI) }));
  check('drive the synth reader: macro 1, its sheet opens with M1 armed', link.to === 'reader:4' && link.macro === 1 && link.sel.type === 'reader' && link.arm === 'm1', [link.to, link.macro, link.sel, link.arm]);
  check('the worklet gets one turn\'s table, turning forwards at the cog\'s speed', link.last && link.last.slot === 0 && link.last.prog.table.length === 128 && Math.abs(link.last.prog.rate - link.rps) < 1e-9 && link.rps > 0, link.last && [link.last.slot, link.last.prog.rate, link.rps]);
  const sync = await probe(() => { const pr = W.probes[0], q = __motions.filter(x => x.prog).at(-1).prog, F = (clock.view + clock.off - clock.shift) * ac.sampleRate,
    u = q.ph0 + q.rate * (F - q.f0) / ac.sampleRate, d = u - renderTh(W.cogs[0]) / (2 * Math.PI); return Math.abs(d - Math.round(d)); });
  check('the worklet\'s probe and the drawn probe agree at the heard time', sync < 2e-3, +sync.toFixed(5));
  is('steady running: nothing more is posted', await probe(async () => { const n = __motions.length; await new Promise(r => setTimeout(r, 500)); return __motions.length - n; }), 0);
  is('the synth\'s M1 knob says the probe drives it', await probe(() => macroDriver(W.readers[0].voice, 1)?.label === 'probe ' + W.probes[0].id), true);
  await probe(() => { W.probes[0].axis = 'x'; W.probes[0]._key = null; }); await wait(150);
  check('another axis: a new table is posted (left–right is a quarter turn on from up–down)', await probe(() => { const t = __motions.filter(m => m.prog).at(-1).prog.table; return Math.abs(t[0] - 1) < 1e-3 && Math.abs(t[64]) < 1e-3; }), 'x');

  console.log('A DRUM');
  await probe(() => { select('probe', W.probes[0]); }); await wait(50);
  await p.selectOption('#inspector .row[data-k="drives"] select', 'pad:3'); await settle(); await wait(150);
  const drum = await probe(() => ({ param: W.probes[0].param, gone: __motions.at(-1).prog, rows: [...document.querySelectorAll('#inspector .row')].map(r => r.dataset.k) }));
  check('drive the snare pad: it takes its cutoff, the synth\'s motion is withdrawn, a param row shows', drum.param === 'cutoff' && drum.gone === null && drum.rows.includes('param'), drum);
  const hit = await probe(() => { const pr = W.probes[0], v = W.pads[0].voice, t = simT + .1, y = probeAt(pr, t), o = probed(v, t), d = probeDef(pr);
    return { y: +y.toFixed(4), want: +(d[2] * Math.pow(d[3] / d[2], y)).toFixed(2), got: +o.cutoff.toFixed(2), same: v.cutoff, copy: o !== v }; });
  check('a strike gets the cutoff from where the probe is at that instant (a copy; the pad keeps its own)', hit.got === hit.want && hit.copy && hit.same === 1800, hit);
  await probe(() => { W.probes[0].lo = .25; W.probes[0].hi = .25; });
  is('low = high: every strike gets that one point of the range', await probe(() => { const d = probeDef(W.probes[0]); return +probed(W.pads[0].voice, simT).cutoff.toFixed(2) === +(d[2] * Math.pow(d[3] / d[2], .25)).toFixed(2); }), true);
  await probe(() => { W.probes[0].lo = 0; W.probes[0].hi = 1; });
  is('a voice no probe drives is passed through untouched', await probe(() => { const v = W.readers[0].voice; return probed(v, simT) === v; }), true);

  console.log('OTHER HOSTS');
  const hosts = await probe(() => {
    W.cogs.push({ id: 20, x: 0, y: 200, n: 24, motor: null, layer: 0, th: 0, w: 0, turns: 0, track: null }); __cogs.solve();
    const m = { ...newCam(21), cog: W.cogs.find(c => c.id === 20), ang: -Math.PI / 2, shape: 'triangle', prof: camShape('triangle') }; W.cams.push(m);
    const pr = newProbe(22); probeAttach(pr, camTip(m)); W.probes.push(pr);
    let worst = 0; for (let i = 0; i < 16; i++) { const th = i * Math.PI / 8; worst = Math.max(worst, Math.abs(probeTabAt(pr, th) - camAt(m, camU(m, th)))); }
    const pc = newProbe(23); probeAttach(pc, { x: 10, y: 205 });
    const pit = newPit(24, { x: 400, y: 0 }); W.pits.push(pit); pit.balls.push(makeBall(pit, 60, 400, -40, 0, 0, 1));
    const pb = newProbe(25); probeAttach(pb, { x: 400, y: -40 }); W.probes.push(pb);
    return { cam: [pr.host, pr.axis], worst: +worst.toFixed(4), cog: [pc.host, pc.on?.id, +pc.r.toFixed(1)], ball: [pb.host, pb.on?.id, pb.ball, +probeNow(pb).toFixed(3)] };
  });
  check('on a cam\'s follower: it reads its reach, which is the cam\'s own shape', hosts.cam[0] === 'cam' && hosts.cam[1] === 'reach' && hosts.worst < .02, hosts);
  check('on a bare cog: it rides that point of the cog', hosts.cog[0] === 'cog' && hosts.cog[1] === 20 && Math.abs(hosts.cog[2] - 11.2) < .2, hosts.cog);
  check('on a ball: its height in the sphere (40 px above the middle of 220: 0.682)', hosts.ball[0] === 'ball' && hosts.ball[1] === 24 && hosts.ball[2] === 1 && Math.abs(hosts.ball[3] - 150 / 220) < .01, hosts.ball);
  const ballProg = await probe(() => { const pb = W.probes.find(p => p.id === 25); pb.to = W.readers[0]; pb.macro = 3; const q = probeProgram(pb); return q && [q.p.rate, q.p.table.length, q.p.glide, q.p.step]; });
  is('a ball on a synth streams its value, gliding 30 ms', ballProg, [0, 1, .03, true]);

  console.log('KEEPING');
  const round = await probe(() => { const j = __cogs.serialize(); __cogs.deserialize(JSON.parse(JSON.stringify(j))); const k = __cogs.serialize(); return [JSON.stringify(j.probes) === JSON.stringify(k.probes), k.probes.map(p => [p.id, p.host, p.to])]; });
  check('save and load: the probes, their hosts and links come back', round[0] && round[1].length === 3, round);
  const loose = await probe(() => { const j = __cogs.serialize(); j.pads = []; j.cams = []; __cogs.deserialize(j); return __cogs.serialize().probes.map(p => [p.host, p.to]); });
  is('a save without the pad loads it loose; without the cam its probe is dropped', loose, [['arm', null], ['ball', 'reader:4']]);
  await probe(() => { select('arm', W.arms[0]); removeSel(); });
  is('delete the arm: its probe goes with it', await probe(() => W.probes.map(p => p.id)), [25]);
  await probe(() => { W.pits[0].balls = []; pruneProbes(); });
  is('take its ball out: that probe goes too', await probe(() => W.probes.length), 0);
  is('page errors', errs, []);
  await b.close();
  console.log(pass ? 'COGS PROBE: PASS' : 'COGS PROBE: FAIL');
  process.exit(pass ? 0 : 1);
})();
