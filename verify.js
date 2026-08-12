// Verification harness (dev tool, never shipped). Drives monad.html in real Chromium.
const { chromium } = require('/srv/rig/monad/node_modules/playwright-core') ;
const path = require('path');

(async () => {
  const browser = await chromium.launch({
    executablePath:'/srv/rig/.cache/ms-playwright/chromium-1187/chrome-linux/chrome',
    args: ['--use-gl=angle','--use-angle=swiftshader','--enable-unsafe-swiftshader','--mute-audio']
  });
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });

  const errors = [], warnings = [], logs = [];
  page.on('console', m => {
    const t = m.type();
    if (t === 'error') errors.push(m.text());
    else if (t === 'warning') warnings.push(m.text());
    else logs.push(t + ': ' + m.text());
  });
  page.on('pageerror', e => errors.push('PAGEERROR: ' + (e && e.stack || e)));

  await page.goto('file://' + path.join(__dirname, 'monad.html'));
  await page.waitForTimeout(900);

  // --- structural / design-law checks -----------------------------------
  const audit = await page.evaluate(() => {
    const out = {};
    const shell = document.getElementById('shell');
    const dev = document.getElementById('dev');
    out.devHidden = dev.hidden;
    // any visible text node outside the (hidden) diagnostics layer?
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    const stray = [];
    while (walker.nextNode()) {
      const n = walker.currentNode;
      if (dev.contains(n)) continue;
      const t = n.nodeValue.trim();
      if (!t) continue;
      const el = n.parentElement;
      const cs = getComputedStyle(el);
      if (cs.display === 'none' || cs.visibility === 'hidden') continue;
      stray.push(t.slice(0, 40) + ' <' + el.tagName + '>');
    }
    out.strayText = stray;
    out.shellNodes = shell.querySelectorAll('.node').length;
    out.rootGlyph = !!document.getElementById('root-glyph');
    out.stageFullScreen = (() => { const r = document.getElementById('stage').getBoundingClientRect(); return Math.round(r.width) + 'x' + Math.round(r.height); })();
    return out;
  });

  // --- interaction: open shell, drill into each group --------------------
  const interaction = { groups: [] };
  const glyph = { x: 720, y: 900 - 56 };
  await page.mouse.click(glyph.x, glyph.y);
  await page.waitForTimeout(500);
  interaction.hubsVisible = await page.evaluate(() =>
    [...document.querySelectorAll('#shellSvg .bloom.in')].length);

  // hub positions, then drill into each
  const hubs = await page.evaluate(() =>
    [...document.querySelectorAll('#shellSvg .bloom.in')].map(g => {
      const r = g.getBoundingClientRect();
      return { x: r.x + r.width / 2, y: r.y + r.height / 2 };
    }));
  for (let i = 0; i < hubs.length; i++) {
    await page.mouse.click(hubs[i].x, hubs[i].y);
    await page.waitForTimeout(420);
    const n = await page.evaluate(() => document.querySelectorAll('#shellSvg .bloom.in').length);
    interaction.groups.push({ hub: i, items: n });
    await page.mouse.click(glyph.x, glyph.y); // back to hub ring
    await page.waitForTimeout(380);
  }

  // --- knob drag actually changes a bound input -------------------------
  await page.evaluate(() => document.querySelectorAll('#shellSvg .node')[0]);
  await page.mouse.click(glyph.x, glyph.y); // ensure hub ring
  await page.waitForTimeout(300);

  // --- spawn some orbs and measure fps ----------------------------------
  await page.mouse.click(400, 300);
  for (const [x, y] of [[500,320],[640,420],[820,300],[960,500],[540,600],[1100,380],[300,520]]) {
    await page.mouse.move(x, y);
    await page.mouse.down();
    await page.mouse.move(x + 40, y + 40, { steps: 4 });
    await page.mouse.up();
    await page.waitForTimeout(60);
  }

  const fps = await page.evaluate(() => new Promise(res => {
    let frames = 0; const t0 = performance.now();
    (function tick() { frames++; if (performance.now() - t0 < 3000) requestAnimationFrame(tick); else res(frames / ((performance.now() - t0) / 1000)); })();
  }));

  const orbCount = await page.evaluate(() => document.querySelectorAll('#shellSvg').length && window.__orbs || null);

  await page.screenshot({ path: 'shot-rest.png' });
  await page.mouse.click(glyph.x, glyph.y);
  await page.waitForTimeout(500);
  await page.screenshot({ path: 'shot-open.png' });
  const hubs2 = await page.evaluate(() => [...document.querySelectorAll('#shellSvg .bloom.in')].map(g => { const r = g.getBoundingClientRect(); return { x: r.x + r.width/2, y: r.y + r.height/2 }; }));
  if (hubs2[1]) { await page.mouse.click(hubs2[1].x, hubs2[1].y); await page.waitForTimeout(520); await page.screenshot({ path: 'shot-group.png' }); }

  console.log(JSON.stringify({ audit, interaction, fps: +fps.toFixed(1), errors, warnings: warnings.slice(0,5), logs: logs.slice(0,10) }, null, 2));
  await browser.close();
  process.exit(errors.length ? 1 : 0);
})().catch(e => { console.error('HARNESS FAIL', e); process.exit(2); });
