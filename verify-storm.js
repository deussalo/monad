// Collision-storm audio test. Deliberately does NOT pass
// --autoplay-policy=no-user-gesture-required: a real touchscreen tap grants
// activation at pointerup, which is the path a phone actually takes. Asserts
// CONFIRMED state and measures pool exhaustion (dropped strikes), which is the
// mechanism behind render-quantum crackle.
const { chromium } = require('/srv/rig/monad/node_modules/playwright-core');
const path = require('path');
const FILE = process.argv[2] || ('file://' + path.join(__dirname, 'monad.html'));
(async () => {
  const b = await chromium.launch({ executablePath:'/srv/rig/.cache/ms-playwright/chromium-1187/chrome-linux/chrome',
    args:['--use-gl=angle','--use-angle=swiftshader','--enable-unsafe-swiftshader'] });
  const ctx = await b.newContext({viewport:{width:390,height:844},deviceScaleFactor:3,hasTouch:true,isMobile:true});
  const p = await ctx.newPage();
  const errs=[]; p.on('pageerror',e=>errs.push(e.message)); p.on('console',m=>{if(m.type()==='error')errs.push(m.text())});
  await p.goto(FILE); await p.waitForTimeout(1200);

  // real tap -> activation at pointerup
  await p.touchscreen.tap(195, 800); await p.waitForTimeout(900);
  const armed = await p.evaluate(()=>window.__monadAudio());
  if (armed !== 'running') { console.log('AUDIO NOT RUNNING:', armed); await b.close(); process.exit(1); }
  await p.touchscreen.tap(195, 800); await p.waitForTimeout(400);   // collapse shell

  const st0 = await p.evaluate(()=>window.__monadAudioStats());
  console.log('armed:', st0.state, '| sampleRate', st0.sampleRate, '| baseLatency', st0.baseLatency+'ms');

  // storm: many bodies, max wind, wheels open
  await p.evaluate(()=>{const w=document.querySelector('#wind');w.value='1';w.dispatchEvent(new Event('input',{bubbles:true}));
                        const d=document.querySelector('#drag');d.value='0.02';d.dispatchEvent(new Event('input',{bubbles:true}));});
  for (let i=0;i<12;i++){ await p.touchscreen.tap(50+((i*61)%290), 150+((i*83)%460)); await p.waitForTimeout(70); }
  await p.touchscreen.tap(195,800); await p.waitForTimeout(500);           // open shell
  const hub = await p.evaluate(()=>{const n=[...document.querySelectorAll('#shellSvg .bloom.in')]
      .find(e=>e.querySelector('.hitpad').getAttribute('aria-label')==='space');
    if(!n)return null; const r=n.querySelector('.hitpad').getBoundingClientRect();
    return {x:r.x+r.width/2,y:r.y+r.height/2};});
  if (hub) { await p.touchscreen.tap(hub.x,hub.y); await p.waitForTimeout(600); }

  // 10s storm, sampling
  const samples=[];
  const t0=Date.now();
  while (Date.now()-t0 < 10000) {
    samples.push(await p.evaluate(()=>window.__monadAudioStats()));
    await p.waitForTimeout(250);
  }
  const peak = k => samples.reduce((m,s)=>Math.max(m,s[k]),0);
  const states = [...new Set(samples.map(s=>s.state))];
  const last = samples[samples.length-1];
  console.log('peak voices        ', peak('voices'), '/ 64 pool');
  console.log('peak modal partials', peak('modals'));
  console.log('dropped strikes    ', last.voiceDrops, '(voice pool exhausted)');
  // modalDrops > 0 under storm is BY DESIGN as of 9ee36a7: the modal layer
  // drops rather than steals, because re-pitching a live sine is audible.
  // Stacking modal partials is what produced the dial-up tone. Do NOT
  // 'fix' a non-zero number here.
  console.log('dropped modal events', last.modalDrops, '(>0 is by design - see 9ee36a7)');
  console.log('AudioContext states seen:', states.join(','), states.length===1&&states[0]==='running'?'✓':'✗ CHANGED');
  console.log('errors:', errs.length?errs.slice(0,2):'NONE');
  const ok = states.length===1 && states[0]==='running' && peak('voices')<=64;
  console.log(ok?'\nSTORM: PASS':'\nSTORM: FAIL');
  await b.close();
  process.exit(ok?0:1);
})();
