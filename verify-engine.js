// Worklet engine, driven in real Chromium: it arms on a gesture, sounds, keeps
// each orb's pitch, its reverb decays for the T60 it is set to, freeze holds,
// and the legacy graph is still reachable for A/B.
const {chromium}=require('/srv/rig/monad/node_modules/playwright-core');
const path=require('path');
const FILE=process.argv[2]||('file://'+path.join(__dirname,'monad.html'));
const CHROME=process.env.MONAD_CHROME||'/srv/rig/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome';
(async()=>{
  const b=await chromium.launch({executablePath:CHROME,args:['--use-gl=angle','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
  const p=await (await b.newContext({viewport:{width:390,height:844},deviceScaleFactor:2,hasTouch:true,isMobile:true})).newPage();
  const errs=[];p.on('pageerror',e=>errs.push(e.message));p.on('console',m=>{if(m.type()==='error')errs.push(m.text())});
  let pass=true;
  const check=(label,ok,got)=>{if(!ok)pass=false;console.log(('  '+label).padEnd(58),JSON.stringify(got),ok?'✓':'✗')};
  await p.goto(FILE);await p.waitForTimeout(1500);
  await p.touchscreen.tap(30,800);await p.waitForTimeout(800);
  const st=await p.evaluate(()=>window.__monadAudioStats());
  check('engine armed by a tap',st.state==='running'&&st.engine==='worklet'&&st.engineState==='ready',{state:st.state,engine:st.engine,engineState:st.engineState});

  const level=await p.evaluate(async()=>{
    const B=window.__monadBuses,an=B.ac.createAnalyser();an.fftSize=2048;B.master.connect(an);
    const T=window.__monadTest;for(let i=0;i<6;i++)T.strike(90);
    await new Promise(r=>setTimeout(r,400));
    const buf=new Float32Array(an.fftSize);an.getFloatTimeDomainData(buf);
    let e=0;for(const v of buf)e+=v*v;return +(10*Math.log10(e/buf.length+1e-20)).toFixed(1);
  });
  check('strikes reach the master bus (dBFS RMS)',level>-50,level);

  const pitch=await p.evaluate(()=>{const T=window.__monadTest,o=T.orbs[0],got=new Set();for(let i=0;i<12;i++)got.add(T.strike(60));return{note:o.note,sounded:[...got]}});
  check('an orb sounds its own pitch',pitch.sounded.length===1&&pitch.sounded[0]===pitch.note,pitch);

  // Offline renders of the same inlined processor: decay and freeze are measured, not assumed.
  const offline=await p.evaluate(async()=>{
    const T=window.__monadTest,src=document.getElementById('monad-engine').textContent;
    async function render(space,sec){
      const ctx=new OfflineAudioContext(2,48000*sec,48000);
      const url='data:text/javascript;charset=utf-8,'+encodeURIComponent(src);
      await ctx.audioWorklet.addModule(url);
      const strike={type:'strike',at:0,k:1,f:[440,1320,2950],g:[1,1,1],t:[.02,.02,.02],amp:1,c:.0005,p:0};
      const node=new AudioWorkletNode(ctx,'monad-engine',{numberOfInputs:0,outputChannelCount:[2],
        processorOptions:{voices:8,lines:16,space:Object.assign(T.spaceMessage(),space),schedule:[strike]}});
      node.connect(ctx.destination);const t0=performance.now();
      const out=await ctx.startRendering();return{x:out.getChannelData(0),ms:performance.now()-t0};
    }
    function t60(x){const n=x.length,e=new Float64Array(n);let acc=0,start=Math.round(48000*.3);
      for(let i=n-1;i>=start;i--){acc+=x[i]*x[i];e[i]=acc}
      let i5=-1,i25=-1;for(let i=start;i<n;i++){const db=10*Math.log10(e[i]/e[start]);if(i5<0&&db<=-5)i5=i;if(db<=-25){i25=i;break}}
      return i25<0?null:+(3*(i25-i5)/48000).toFixed(2)}
    const flat={lowMult:1,highMult:1,dry:0,wet:1,predelay:.01,lowCut:20,highCut:20000,shimmer:0,freeze:0,duck:0};
    const r4=await render(Object.assign({},flat,{decay:4}),9);
    const rms=(x,a,b)=>{let e=0;for(let i=a;i<b;i++)e+=x[i]*x[i];return Math.sqrt(e/(b-a))};
    const fz=await render(Object.assign({},flat,{decay:4,freeze:1}),8);
    let finite=true;for(const v of fz.x)if(!Number.isFinite(v)){finite=false;break}
    return{t60:t60(r4.x),renderRealtimeFactor:+(9000/r4.ms).toFixed(1),
      freezeHold:+(rms(fz.x,48000*7,48000*8)/rms(fz.x,48000*1,48000*2)).toFixed(3),finite};
  });
  check('reverb T60 matches decay=4 s within 10%',offline.t60!==null&&Math.abs(offline.t60-4)<=.4,offline.t60);
  check('freeze holds the tail (7 s vs 1 s level)',offline.freezeHold>.6&&offline.freezeHold<1.5&&offline.finite,offline.freezeHold);
  console.log('  offline render speed: '+offline.renderRealtimeFactor+'× realtime (16 lines, 8 voices)');

  const ab=await p.evaluate(async()=>{const T=window.__monadTest;T.setEngine('legacy');T.strike(90);
    await new Promise(r=>setTimeout(r,200));const s=window.__monadAudioStats();T.setEngine('worklet');
    return{engine:s.engine,voices:s.voices,back:window.__monadAudioStats().engine}});
  check('legacy graph reachable for A/B and back',ab.engine==='legacy'&&ab.voices>0&&ab.back==='worklet',ab);

  check('no page errors',errs.length===0,errs);
  await b.close();
  console.log(pass?'PASS':'FAIL');process.exitCode=pass?0:1;
})().catch(e=>{console.error(e);process.exitCode=1});
