// Screenshot the HTML build (cogs/src, inlined as build.js does, into /tmp) on a phone viewport, for comparing with
// the Rust stage's snapshots. node html_shot.js <out.png> [world.json] [script.js to evaluate in the page first]
const fs = require('fs'), path = require('path');
const { chromium } = require(process.env.COGS_PW || '/srv/rig/rig/web/node_modules/.pnpm/playwright-core@1.64.0/node_modules/playwright-core');
const SRC = path.join(__dirname, '../../../../cogs/src'), INCLUDE = /^\s*(?:\/\/|\/\*)@include (\S+?)(?:\*\/)?\s*$/;
const expand = f => fs.readFileSync(path.join(SRC, f), 'utf8').replace(/\n$/, '').split('\n').map(l => { const m = l.match(INCLUDE); return m ? expand(m[1]) : l; }).join('\n');
const [out = '/tmp/html.png', world = '', script = ''] = process.argv.slice(2);
fs.writeFileSync('/tmp/cogs-phone.html', expand('phone.html') + '\n');
(async () => {
  const b = await chromium.launch({ executablePath: process.env.MONAD_CHROME || '/srv/rig/.cache/ms-playwright/chromium-1248/chrome-linux64/chrome', args: ['--no-sandbox'] });
  const ctx = await b.newContext({ viewport: { width: 360, height: 780 }, deviceScaleFactor: 3, hasTouch: true, isMobile: true });
  const p = await ctx.newPage(); const errs = [];
  p.on('pageerror', e => errs.push(String(e)));
  await p.goto('file:///tmp/cogs-phone.html');
  await p.waitForTimeout(300);
  if (world) await p.evaluate(j => { deserialize(j); const f = fitTarget(); view.x = f.x; view.y = f.y; S.zoom = f.z; }, JSON.parse(fs.readFileSync(world, 'utf8')));
  if (script) await p.evaluate(fs.readFileSync(script, 'utf8'));
  await p.waitForTimeout(1200);
  await p.screenshot({ path: out });
  console.log('wrote', out, errs.slice(0, 3));
  await b.close();
})().catch(e => { console.error(e); process.exit(1); });
