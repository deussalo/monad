// The ball pit (the box): its physics stepped by hand, then the part in desk.html: placing it, dropping and throwing
// balls, taking one out, the palette, the voice, saving and undo.
const { chromium } = require(process.env.COGS_PW || '/srv/rig/rig/webapp/node_modules/.pnpm/playwright-core@1.64.0/node_modules/playwright-core');
const path = require('path');
const FILE = process.argv[2] || ('file://' + path.join(__dirname, 'desk.html'));
(async () => {
  const b = await chromium.launch({ executablePath: (process.env.MONAD_CHROME || '/srv/rig/.cache/ms-playwright/chromium-1248/chrome-linux64/chrome') });
  let pass = true;
  const check = (label, ok, got) => { if (!ok) pass = false; console.log(('  ' + label).padEnd(78), JSON.stringify(got), ok ? '✓' : '✗'); };
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

  console.log('PHYSICS');
  // a world with one box and whatever the case needs, stepped by hand at the pit's rate while the clock is paused
  const run = (setup) => probe(setup => {
    setPaused(true);
    const j = __cogs.serialize(); Object.assign(j, { cogs: [], arms: [], pads: [], readers: [], belts: [], strings: [], cams: [], view: { x: 0, y: 0 } });
    j.pits = [{ id: 9, x: 0, y: 0, w: 300, h: 200, voice: { kind: 'synth', level: .7, velSense: .7 }, notes: [60, 64, 67], balls: [] }];
    const f = new Function('j', setup.world || ''); f(j); __cogs.deserialize(j);
    const pit = W.pits[0], log = () => __cogs.stats.log.filter(l => l.src.startsWith('ball9.')).length, n0 = log();
    let t = simT; const dt = PIT.dt;
    for (let i = 0; i < Math.round(setup.secs / dt); i++) { pitStep(pitReach(pit), dt, t); for (const c of W.cogs) turn(c, dt); t += dt; }
    const { L, R, T, B } = pitRect(pit);
    return { hits: log() - n0, srcs: [...new Set(__cogs.stats.log.slice(-40).filter(l => l.src.startsWith('ball9.')).map(l => l.src))].sort(),
      balls: pit.balls.map(b => ({ x: +b.x.toFixed(1), y: +b.y.toFixed(1), v: +Math.hypot(b.vx, b.vy).toFixed(1), inside: b.x - b.r >= L - 1e-6 && b.x + b.r <= R + 1e-6 && b.y - b.r >= T - 1e-6 && b.y + b.r <= B + 1e-6 })) };
  }, setup);
  const wall = await run({ secs: .5, world: "j.pits[0].balls=[{id:1,note:60,bx:0,by:0,bvx:400,bvy:0}]" });
  check('a ball thrown at the right wall: it sounds once, bounces back, a little slower', wall.hits === 1 && wall.balls[0].x < 140 && wall.balls[0].v < 400 && wall.balls[0].v > 300, wall);
  const pair = await run({ secs: .3, world: "j.pits[0].balls=[{id:1,note:60,bx:-60,by:0,bvx:200,bvy:0},{id:2,note:67,bx:60,by:0,bvx:-200,bvy:0}]" });
  check('two balls meeting: both sound, and they part', pair.hits === 2 && JSON.stringify(pair.srcs) === '["ball9.1","ball9.2"]' && pair.balls[0].x < pair.balls[1].x - 15, pair);
  const quiet = await run({ secs: .5, world: "j.pits[0].balls=[{id:1,note:60,bx:-100,by:0,bvx:-10,bvy:0}]" });
  is('a ball that only drifts against the wall is silent', quiet.hits, 0);
  const arm = await run({ secs: 1, world: "j.cogs=[{id:1,x:0,y:0,n:12,motor:{rpm:60,revs:0},layer:0,th:0,track:null}];j.arms=[{id:2,cog:1,a:0,r0:0,len:80,flex:.5}];j.pits[0].balls=[{id:1,note:60,bx:0,by:60,bvx:0,bvy:0}]" });
  check('a ball at rest in a turning arm\'s sweep: the arm bats it, it flies and sounds on a wall', arm.balls[0].v > 50 && arm.hits >= 1, arm);
  const crowd = await run({ secs: 4, world: "j.cogs=[{id:1,x:0,y:0,n:12,motor:{rpm:90,revs:0},layer:0,th:0,track:null}];j.arms=[{id:2,cog:1,a:0,r0:0,len:95,flex:.5}];j.pits[0].balls=Array.from({length:24},(_,i)=>({id:i+1,note:[60,64,67][i%3],bx:-120+(i%8)*34,by:-70+Math.floor(i/8)*60,bvx:(i%5-2)*90,bvy:(i%3-1)*120}))" });
  check('24 balls and a fast arm for 4 s: all still in the box, none past the speed cap', crowd.balls.every(b => b.inside && b.v <= 1200 + 1e-6), crowd.balls.filter(b => !b.inside || b.v > 1200));
  await probe(() => setPaused(false));

  console.log('PALETTE');
  const pal = await probe(() => {
    const p = W.pits[0]; p.notes = [60, 62, 64, 65, 67]; p.bag = []; const draw = Array.from({ length: 5 }, () => pitNote(p)).sort((a, b) => a - b);
    p.balls = [makeBall(p, 62, 0, 0, 0, 0), makeBall(p, 66, 0, 0, 0, 0)]; p.notes = [60, 64, 67]; pitRetune(p);
    return { draw, kept: p.balls.map(b => [b.note, b.r === ballR(b.note)]) };
  });
  is('five drops from five notes: each note once before any repeats', pal.draw, [60, 62, 64, 65, 67]);
  is('notes taken off the palette: their balls take the nearest note left, and its size', pal.kept, [[60, true], [67, true]]);
  is('low notes are big balls, high notes small', await probe(() => [ballR(36), ballR(60), ballR(84)].map(r => +r.toFixed(1))), [22, 11, 6]);

  console.log('THE PART');
  await probe(() => { const j = __cogs.serialize(); Object.assign(j, { cogs: [], arms: [], pads: [], readers: [], belts: [], strings: [], cams: [], pits: [], view: { x: 0, y: 0 } }); j.S.zoom = 1.5; __cogs.deserialize(j); __cogs.commit(); });
  await p.keyboard.press('o'); await p.mouse.click(...await scr([0, 0])); await settle(); await p.keyboard.press('Escape');
  const made = await probe(() => ({ n: W.pits.length, sel: __cogs.sel?.type, x: W.pits[0]?.x, y: W.pits[0]?.y, voice: W.pits[0]?.voice.kind, sheet: !!document.querySelector('#inspector .pit-keys') }));
  is('key o, then a click: a box there, selected, a synth voice, its palette keyboard in the sheet', made, { n: 1, sel: 'pit', x: 0, y: 0, voice: 'wave', sheet: true });
  await p.mouse.click(...await scr([-60, 20])); await settle();
  const one = await probe(() => { const b = W.pits[0].balls[0]; return { n: W.pits[0].balls.length, note: PIT_NOTES.includes(b.note), at: ballAt(b) }; });
  check('a tap inside drops a ball where it was tapped, on a palette note', one.n === 1 && one.note && Math.hypot(one.at.x + 60, one.at.y - 20) < 30, one);
  const [ax, ay] = await scr([-40, -40]), [zx, zy] = await scr([40, -40]);
  await p.mouse.move(ax, ay); await p.mouse.down(); await p.mouse.move(zx, zy, { steps: 6 }); await p.mouse.up(); await settle();
  const thrown = await probe(() => { const b = W.pits[0].balls[1]; return { n: W.pits[0].balls.length, vx: b.vx, vy: b.vy }; });
  check('a drag inside throws a new ball the way the finger went', thrown.n === 2 && thrown.vx > 100 && Math.abs(thrown.vy) < thrown.vx / 3, thrown);
  const u0 = await probe(() => __cogs.undos);
  await probe(() => __cogs.undo()); await wait(100);
  const back = await probe(() => ({ n: W.pits[0].balls.length, moving: Math.hypot(W.pits[0].balls[0].vx, W.pits[0].balls[0].vy) > 0 }));
  is('undo takes the thrown ball out; the other keeps moving', back, { n: 1, moving: true });
  await probe(() => __cogs.redo()); await wait(100);
  is('redo puts it back', await probe(() => W.pits[0].balls.length), 2);
  // hold a ball to take it out: pause so it stays under the finger
  await probe(() => { setPaused(true); const b = W.pits[0].balls[0]; b.vx = b.vy = 0; });
  const at = await probe(() => { const q = ballAt(W.pits[0].balls[0]), s = __cogs.toScreen(q); return [s.x, s.y]; });
  await p.mouse.move(...at); await p.mouse.down(); await wait(600); await p.mouse.up(); await settle();
  is('hold a ball: it is taken out', await probe(() => W.pits[0].balls.length), 1);
  await probe(() => setPaused(false));
  // the palette keyboard: a tap on a lit key takes the note off, on a dark one puts it on
  const keyAt = m => probe(m => { const cv = document.querySelector('#inspector .pit-keys'), r = cv.getBoundingClientRect(), ww = r.width / 14, i = [0, 2, 4, 5, 7, 9, 11].indexOf(m % 12) + 7 * (Math.floor(m / 12) - 5); return [r.left + ww * (i + .5), r.top + r.height * .85]; }, m);
  await p.mouse.click(...await keyAt(62)); await settle();
  await p.mouse.click(...await keyAt(65)); await settle();
  is('the palette keyboard: D4 off, F4 on', await probe(() => W.pits[0].notes), [60, 64, 65, 67, 69, 72]);
  await p.click('#inspector .wave-part>summary'); await p.selectOption('#inspector .row[data-k="kind"] select', 'synth'); await settle();
  is('its voice can be the modal synth', await probe(() => W.pits[0].voice.kind), 'synth');
  const saved = await probe(() => { const j = __cogs.serialize(); __cogs.deserialize(JSON.parse(JSON.stringify(j))); const k = __cogs.serialize(); return [JSON.stringify(j.pits) === JSON.stringify(k.pits), k.pits[0].balls.length, k.pits[0].notes]; });
  is('save and load: the box, its palette and its balls come back as they were', saved, [true, 1, [60, 64, 65, 67, 69, 72]]);
  const ev0 = await probe(() => __cogs.stats.events);
  await probe(() => { const p = W.pits[0]; for (let i = 0; i < 6; i++) dropBall(p, { x: -80 + i * 30, y: 0 }, 300 * Math.cos(i), 300 * Math.sin(i)); });
  await wait(1500);
  check('live: balls dropped into the running machine sound on the walls and each other', await probe(() => __cogs.stats.events) - ev0 >= 5, await probe(() => __cogs.stats.events) - ev0);
  const moved = await probe(() => { const p = W.pits[0], before = p.balls.map(b => [b.x - p.x, b.y - p.y]); shiftPit(p, 500, 0); return p.balls.every((b, i) => Math.abs(b.x - p.x - before[i][0]) < 1e-9); });
  is('moving the box carries its balls', moved, true);

  check('page errors', errs.length === 0, errs.slice(0, 3));
  console.log(pass ? 'COGS PIT: PASS' : 'COGS PIT: FAIL');
  await b.close(); process.exit(pass ? 0 : 1);
})().catch(e => { console.error(e); process.exit(1); });
