// Guards standing rule (1): NOTHING allocates AudioNodes per strike.
// Counts every AudioNode constructor call, split into init vs steady-state.
// A regression here is silent by nature — a feature that allocates per strike
// reintroduces the crackle without any visible symptom in other tests.
const { chromium } = require('/srv/rig/monad/node_modules/playwright-core');
const path = require('path');
const FILE = process.argv[2] || ('file://' + path.join(__dirname, 'monad.html'));
(async () => {
  const b = await chromium.launch({ executablePath:'/srv/rig/.cache/ms-playwright/chromium-1187/chrome-linux/chrome',
    args:['--use-gl=angle','--use-angle=swiftshader','--enable-unsafe-swiftshader'] });
  const ctx = await b.newContext({viewport:{width:390,height:844},deviceScaleFactor:3,hasTouch:true,isMobile:true});
  const p = await ctx.newPage();
  await p.addInitScript(() => {
    window.__nodes = {};
    const AC = window.AudioContext || window.webkitAudioContext;
    const names = ['createOscillator','createGain','createBiquadFilter','createDelay',
                   'createStereoPanner','createChannelMerger','createDynamicsCompressor','createConstantSource'];
    for (const n of names) {
      const orig = AC.prototype[n];
      if (!orig) continue;
      AC.prototype[n] = function (...a) { window.__nodes[n] = (window.__nodes[n]||0) + 1; return orig.apply(this, a); };
    }
  });
  const errs=[]; p.on('pageerror',e=>errs.push(e.message));
  await p.goto(FILE); await p.waitForTimeout(1200);
  await p.touchscreen.tap(195,800); await p.waitForTimeout(1200);   // arm audio -> builds the pool
  if (await p.evaluate(()=>window.__monadAudio()) !== 'running') { console.log('AUDIO NOT RUNNING'); await b.close(); process.exit(1); }
  await p.touchscreen.tap(195,800); await p.waitForTimeout(400);
  const atInit = await p.evaluate(()=>JSON.parse(JSON.stringify(window.__nodes)));
  const sum = o => Object.values(o).reduce((a,c)=>a+c,0);
  console.log('AudioNodes created at init :', sum(atInit), JSON.stringify(atInit));
  // storm
  await p.evaluate(()=>{const s=(i,v)=>{const e=document.querySelector(i);e.value=v;e.dispatchEvent(new Event('input',{bubbles:true}))};
    s('#wind','1'); s('#drag','0.02');});
  for (let i=0;i<12;i++){ await p.touchscreen.tap(50+((i*61)%290), 150+((i*83)%460)); await p.waitForTimeout(70); }
  const before = await p.evaluate(()=>JSON.parse(JSON.stringify(window.__nodes)));
  await p.waitForTimeout(12000);
  const after  = await p.evaluate(()=>JSON.parse(JSON.stringify(window.__nodes)));
  const st = await p.evaluate(()=>window.__monadAudioStats());
  const grew = {};
  for (const k of new Set([...Object.keys(before),...Object.keys(after)])) {
    const d = (after[k]||0)-(before[k]||0); if (d) grew[k]=d;
  }
  const total = sum(after)-sum(before);
  console.log('strikes during storm       :', st.strikeCount);
  console.log('AudioNodes created DURING  :', total, Object.keys(grew).length?JSON.stringify(grew):'(none)');
  console.log('voiceDrops / modalDrops    :', st.voiceDrops, '/', st.modalDrops);
  console.log('AudioContext state         :', st.state);
  console.log('errors                     :', errs.length?errs.slice(0,2):'NONE');
  const ok = total === 0 && st.state === 'running' && st.voiceDrops === 0 && errs.length === 0;
  console.log(ok ? '\nWARM POOL: PASS (zero allocation per strike)' : '\nWARM POOL: FAIL');
  await b.close(); process.exit(ok?0:1);
})();
