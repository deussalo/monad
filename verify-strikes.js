// "Every impact should make a sound." Measures sounded/attempted directly, and
// simultaneously watches the crackle proxies so the fix cannot silently trade
// audibility for underruns. Real tap for activation; no autoplay override.
const { chromium } = require('/srv/rig/monad/node_modules/playwright-core');
const path = require('path');
const FILE = process.argv[2] || ('file://' + path.join(__dirname, 'monad.html'));
(async () => {
  const b = await chromium.launch({ executablePath:(process.env.MONAD_CHROME||'/srv/rig/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome'),
    args:['--use-gl=angle','--use-angle=swiftshader','--enable-unsafe-swiftshader'] });
  const ctx = await b.newContext({viewport:{width:390,height:844},deviceScaleFactor:3,hasTouch:true,isMobile:true});
  const p = await ctx.newPage();
  const errs=[]; p.on('pageerror',e=>errs.push(e.message)); p.on('console',m=>{if(m.type()==='error')errs.push(m.text())});
  await p.goto(FILE); await p.waitForTimeout(1200);
  await p.touchscreen.tap(195,800); await p.waitForTimeout(900);
  if (await p.evaluate(()=>window.__monadAudio()) !== 'running') { console.log('AUDIO NOT RUNNING'); await b.close(); process.exit(1); }
  await p.touchscreen.tap(195,800); await p.waitForTimeout(400);
  // Wind off and six pairs launched head-on from close range: software GL runs the
  // simulation at about a tenth of real time, so a long course never closes.
  await p.evaluate(()=>{const e=document.querySelector('#wind');e.value='0';e.dispatchEvent(new Event('input',{bubbles:true}))});
  const PAIRS = 6;
  const base = await p.evaluate(n=>{const orbs=[];
    for(let i=0;i<n;i++){const y=.15+i*.14;orbs.push({x:.38,y,radius:.05,vx:.5,vy:0},{x:.62,y,radius:.05,vx:-.5,vy:0})}
    const s=window.__monadAudioStats();window.__monadTest.applyPresetObject({scene:{orbs}});return s},PAIRS);
  await p.waitForTimeout(6000);
  const end = await p.evaluate(()=>window.__monadAudioStats());
  const dc = (end.contactEvents||0)-(base.contactEvents||0);
  const ds = (end.strikeCount||0)-(base.strikeCount||0);
  console.log('physical contacts    :', dc);
  console.log('strikes sounded      :', ds);
  console.log('ratio                :', dc? (ds/dc).toFixed(3) : 'n/a');
  console.log('voiceDrops           :', end.voiceDrops, '(must be 0)');
  console.log('modalDrops           :', end.modalDrops, '(>0 by design: modal layer drops, never steals)');
  console.log('AudioContext state   :', end.state);
  console.log('errors               :', errs.length?errs.slice(0,2):'NONE');
  console.log('designed contacts    :', 2*PAIRS, '(at least)');
  const ok = end.state==='running' && end.voiceDrops===0 && dc >= 2*PAIRS && ds/dc > 0.9;
  console.log(ok?'\nSTRIKES: PASS':'\nSTRIKES: FAIL');
  await b.close(); process.exit(ok?0:1);
})();
