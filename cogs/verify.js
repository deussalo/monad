// Builds a contraption through the UI and checks the mechanism and the audio it drives.
const { chromium } = require('/srv/rig/monad/node_modules/playwright-core');
const path = require('path');
const FILE = process.argv[2] || ('file://' + path.join(__dirname, 'cogs.html'));
const SHOT = process.env.COGS_SHOT || '';
(async () => {
  const b = await chromium.launch({ executablePath: (process.env.MONAD_CHROME || '/srv/rig/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome'),
    args: ['--use-gl=angle', '--use-angle=swiftshader', '--enable-unsafe-swiftshader'] });
  const p = await (await b.newContext({ viewport: { width: 1200, height: 800 } })).newPage();
  const errs = []; p.on('pageerror', e => errs.push(e.message));
  let pass = true;
  const check = (label, ok, got) => { if (!ok) pass = false; console.log(('  ' + label).padEnd(58), JSON.stringify(got), ok ? '✓' : '✗'); };
  const probe = fn => p.evaluate(fn);
  const near = (a, b, tol) => Math.abs(a - b) <= tol;

  await p.goto(FILE); await p.waitForTimeout(400);
  await p.click('button[data-act=audio]');
  await p.waitForFunction(() => __cogs.engine().state === 'ready' && __cogs.engine().ac === 'running', null, { timeout: 5000 });
  await p.click('button[data-act=clear]');
  const rect = await probe(() => { const r = document.querySelector('#c').getBoundingClientRect(); return { x: r.left + r.width / 2, y: r.top + r.height / 2 }; });
  const at = (x, y) => [rect.x + x, rect.y + y];
  const tool = t => p.click(`button[data-tool=${t}]`);
  const drag = async (a, z) => { await p.mouse.move(...at(...a)); await p.mouse.down(); await p.mouse.move(...at(...z), { steps: 6 }); await p.mouse.up(); };
  const tap = async a => { await p.mouse.move(...at(...a)); await p.mouse.down(); await p.mouse.up(); };
  const field = (k, v) => p.fill(`#panel fieldset:nth-of-type(4) [data-k="${k}"] input[type=number]`, String(v));

  console.log('GEARS');
  await tool('cog'); await drag([0, 0], [46, 0]); await field('motor rpm', 60);
  await tool('cog'); await drag([70, 0], [93, 0]);
  const gears = await probe(() => __cogs.W.cogs.map(c => ({ n: c.n, x: c.x, rpm: c.w * 60 / (2 * Math.PI), jam: c.jam })));
  check('drawn teeth 24 and 12', gears[0].n === 24 && gears[1].n === 12, gears.map(g => g.n));
  check('second cog snapped to pitch distance 68.75', near(gears[1].x, 36 * 12 / (2 * Math.PI), 1e-9), gears[1].x);
  check('60 rpm drives -120 rpm', near(gears[0].rpm, 60, 1e-9) && near(gears[1].rpm, -120, 1e-9), gears.map(g => g.rpm));

  console.log('ARMS AND PADS');
  await tool('arm'); await drag([0, 0], [0, -80]);
  await tool('arm'); await drag([68.75, 0], [68.75, 40]);
  await tool('pad'); await tap([-60, -30]);
  await tool('pad'); await tap([98.75, 20]);
  await p.click('button[data-act="reset hits"]'); await p.waitForTimeout(4000);
  const hits = await probe(() => {
    const [a, b] = __cogs.W.pads, log = __cogs.stats.log;
    const times = id => log.filter(l => l.src.endsWith('>pad' + id)).map(l => l.t);
    const gaps = ts => ts.slice(1).map((t, i) => t - ts[i]);
    return { a: a.hits, b: b.hits, gapsA: gaps(times(a.id)), gapsB: gaps(times(b.id)), vA: a.lastV, late: __cogs.stats.late };
  });
  check('60 rpm arm: 4 hits in 4 s', hits.a >= 3 && hits.a <= 5, hits.a);
  check('-120 rpm arm: 8 hits in 4 s', hits.b >= 7 && hits.b <= 9, hits.b);
  check('hit period 1 s to 1e-6', hits.gapsA.length > 1 && hits.gapsA.every(g => near(g, 1, 1e-6)), hits.gapsA);
  check('hit period 0.5 s to 1e-6', hits.gapsB.length > 1 && hits.gapsB.every(g => near(g, .5, 1e-6)), hits.gapsB);
  const vTangent = 2 * Math.PI * Math.sqrt(60 * 60 + 30 * 30 - 144);
  check('impact speed = w * tangent radius', near(hits.vA, vTangent, 1e-6), [hits.vA, vTangent]);
  check('no event scheduled late', hits.late === 0, hits.late);

  console.log('JAM');
  const selectCog = async pts => { for (const pt of pts) { await tap(pt); if (await probe(() => document.querySelector('#panel fieldset:nth-of-type(4) legend').textContent.startsWith('cog'))) return; } throw new Error('no cog selected'); };
  await tool('select'); await selectCog([[68.75, 15], [55, -10], [80, -12], [62, 18]]); await field('motor rpm', 100);
  const jam = await probe(() => __cogs.W.cogs.map(c => [c.jam, c.w]));
  check('two disagreeing motors jam the train', jam.every(([j, w]) => j && w === 0), jam);
  await field('motor rpm', -120);
  const ok = await probe(() => __cogs.W.cogs.map(c => [c.jam, c.w * 60 / (2 * Math.PI)]));
  check('agreeing motors turn', ok.every(([j]) => !j) && near(ok[1][1], -120, 1e-9), ok);
  await field('motor rpm', 0);

  console.log('DISC AND READER');
  await tool('disc'); await tap([0, 170]);
  await tool('dot'); for (const pt of [[40, 170], [0, 240], [-80, 170]]) await tap(pt);
  await tool('reader'); await drag([0, 165], [0, 80]);
  await tool('select'); await selectCog([[80, 175], [-70, 190]]); await field('motor rpm', 30);
  const disc = await probe(() => { const c = __cogs.W.cogs[2]; return { n: c.n, dots: c.track.dots.length, rpm: c.w * 60 / (2 * Math.PI), strikes: __cogs.engine().strikes }; });
  check('disc: 48 teeth, 3 dots, 30 rpm', disc.n === 48 && disc.dots === 3 && near(disc.rpm, 30, 1e-9), disc);
  await p.click('button[data-act="reset hits"]'); await p.waitForTimeout(4200);
  const rd = await probe(() => ({ hits: __cogs.W.readers[0].hits, rings: [...new Set(__cogs.stats.log.filter(l => l.src.includes('>reader')).map(l => l.src.split('>')[0]))], eng: __cogs.engine() }));
  check('reader: 3 dots x 2 revolutions', rd.hits >= 5 && rd.hits <= 7, rd.hits);
  check('three distinct rings sounded', rd.rings.length === 3, rd.rings);
  check('engine consumed the modal strikes', rd.eng.strikes - disc.strikes >= rd.hits - 1 && rd.eng.drops === 0, { strikes: rd.eng.strikes - disc.strikes, drops: rd.eng.drops });
  const peak = await probe(() => document.querySelector('#panel [data-k="peak dB"] b').textContent);
  check('output reaches the destination', parseFloat(peak) > -60, peak);

  console.log('PAUSE AND PERSIST');
  await p.click('button[data-act=pause]'); const before = await probe(() => __cogs.stats.events);
  await p.waitForTimeout(1000); const after = await probe(() => __cogs.stats.events);
  check('paused: no events', after - before <= 1, after - before);
  await p.click('button[data-act=pause]');
  await p.waitForTimeout(600); await p.reload(); await p.waitForTimeout(400);
  const world = await probe(() => __cogs.W.cogs.length + ' ' + __cogs.W.arms.length + ' ' + __cogs.W.pads.length + ' ' + __cogs.W.readers.length);
  check('world restored after reload', world === '3 2 2 1', world);
  if (SHOT) await p.screenshot({ path: SHOT });
  check('page errors', errs.length === 0, errs.slice(0, 3));
  console.log(pass ? '\nCOGS: PASS' : '\nCOGS: FAIL');
  await b.close(); process.exit(pass ? 0 : 1);
})();
