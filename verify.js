const {chromium}=require('playwright-core');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const executablePath=process.env.CHROMIUM_PATH||'/srv/rig/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome';
const url=process.argv[2]||`file://${path.join(__dirname,'monad.html')}`;
const report={};
(async()=>{
  const browser=await chromium.launch({executablePath,args:['--no-sandbox']});
  try{
    const page=await browser.newPage({viewport:{width:1440,height:960}});
    const errors=[];page.on('pageerror',e=>errors.push(e.message));
    await page.goto(url);await page.waitForTimeout(500);
    assert.equal(await page.evaluate(()=>synth),null,'Audio stays gated before a gesture');
    await page.screenshot({path:'shot-viii-world.png'});
    await page.click('#invitation');
    assert.equal(await page.evaluate(()=>synth.ctx.state),'running');
    await page.click('#pause');
    const point=await page.evaluate(()=>view(bodies[2]));
    await page.mouse.move(point.x,point.y);await page.mouse.down();await page.waitForTimeout(130);
    assert.equal(await page.evaluate(()=>synth.held),1,'A held sphere sustains its envelope');
    await page.mouse.up();
    assert.equal(await page.evaluate(()=>synth.held),0,'Releasing the sphere releases its envelope');
    assert.equal(await page.evaluate(()=>selected.id),2);
    await page.locator('[data-tab="envelope"]').click();
    await page.locator('#knob-attack').focus();await page.keyboard.press('End');
    assert.equal(await page.evaluate(()=>selected.source.attack),2);
    assert.notEqual(await page.evaluate(()=>bodies[0].source.attack),2,'ADSR belongs to one source');
    await page.keyboard.press('Home');
    await page.locator('[data-tab="equalizer"]').click();
    await page.locator('#knob-high').focus();await page.keyboard.press('End');
    assert.equal(await page.evaluate(()=>selected.source.high),12);
    assert.equal(await page.evaluate(()=>bodies[0].source.high),1.5,'EQ belongs to one source');
    await page.locator('[data-tab="layers"]').click();await page.locator('[data-layer="1"]').click();
    await page.locator('#knob-octave').focus();await page.keyboard.press('End');
    await page.selectOption('#waveform','glass');
    assert.deepEqual(await page.evaluate(()=>[selected.source.layers[1].octave,selected.source.layers[0].octave,selected.source.layers[1].wave]),[2,0,'glass']);
    await page.screenshot({path:'shot-viii-layers.png'});
    await page.locator('[data-tab="envelope"]').click();await page.screenshot({path:'shot-viii-envelope.png'});
    await page.click('#audition');await page.waitForTimeout(350);
    const signal=await page.evaluate(()=>{const samples=new Float32Array(1024);synth.analyser.getFloatTimeDomainData(samples);return Math.max(...samples.map(Math.abs));});
    assert.ok(signal>.00001,'Actual output signal follows interaction');
    await page.click('#record');await page.click('#audition');await page.waitForTimeout(1100);
    const [download]=await Promise.all([page.waitForEvent('download'),page.click('#record')]);
    const downloadPath=await download.path();assert.ok(fs.statSync(downloadPath).size>1000,'Recording contains bytes');
    report.recording={name:download.suggestedFilename(),bytes:fs.statSync(downloadPath).size};
    const snapshot=await page.evaluate(()=>JSON.stringify(saveUniverse()));
    await page.evaluate(()=>{const snapshot=saveUniverse();const valid=readUniverse(JSON.stringify(snapshot));openUniverse(valid);});
    assert.deepEqual(await page.evaluate(()=>saveUniverse()),JSON.parse(snapshot),'Scene export/import preserves every source');
    assert.equal(await page.evaluate(()=>{const before=JSON.stringify(saveUniverse());const broken=saveUniverse();broken.bodies[1].source.release='bad';try{openUniverse(readUniverse(JSON.stringify(broken)));return false;}catch{return JSON.stringify(saveUniverse())===before;}}),true,'Invalid imports cannot partly mutate the instrument');
    await page.click('#world-mode');
    const knob=await page.locator('#knob-gravity').boundingBox();
    const gravityBefore=await page.evaluate(()=>settings.gravity);
    await page.mouse.move(knob.x+knob.width/2,knob.y+knob.height/2);await page.mouse.down();await page.mouse.move(knob.x+knob.width/2,knob.y+knob.height/2-40);await page.mouse.up();
    assert.ok(await page.evaluate(()=>settings.gravity)>gravityBefore,'Sphere dragging changes its value');
    await page.evaluate(()=>{for(let i=0;i<70;i++)addBody(geometry.x+(i%7)*12,geometry.y+Math.floor(i/7)*10);});
    assert.equal(await page.evaluate(()=>bodies.length),22);
    await page.click('#pause');
    await page.waitForTimeout(500);
    report.physics=await page.evaluate(()=>{
      for(let i=0;i<2400;i++)step(1/120);
      return {finite:bodies.every(b=>[b.x,b.y,b.z,b.vx,b.vy,b.vz].every(Number.isFinite)),bodies:bodies.length,ripples:ripples.length,voices:synth.active};
    });
    assert.equal(await page.evaluate(()=>{const a=bodies[0],b=bodies[1];Object.assign(b,{x:a.x,y:a.y,z:a.z,vx:a.vx,vy:a.vy,vz:a.vz});step(1/120);return Math.hypot(a.x-b.x,a.y-b.y,a.z-b.z)>1;}),true,'Coincident spheres separate without a singularity');
    assert.equal(await page.evaluate(()=>{const b=bodies[0];b.z=geometry.radius*.8;b.x=width;b.y=height;contain(b);const p=view(b);return p.x+p.r<=geometry.right+.001&&p.y+p.r<=geometry.bottom+.001;}),true,'Depth projection keeps spheres inside the playable field');
    assert.ok(report.physics.finite);assert.ok(report.physics.voices<=24);assert.ok(report.physics.ripples<=32);
    report.frames=await page.evaluate(()=>new Promise(resolve=>{
      const gaps=[],costs=[];let previous=performance.now();
      function tick(now){gaps.push(now-previous);previous=now;const start=performance.now();draw();costs.push(performance.now()-start);if(gaps.length<180)requestAnimationFrame(tick);else{gaps.sort((a,b)=>a-b);costs.sort((a,b)=>a-b);resolve({medianMs:gaps[90],p95Ms:gaps[171],drawP95Ms:costs[171]});}}
      requestAnimationFrame(tick);
    }));
    assert.ok(report.frames.drawP95Ms<16.7,'Crowded-scene rendering fits a 60 Hz frame budget on this host');
    report.audio=await page.evaluate(async()=>{
      const ctx=new OfflineAudioContext(2,48000*14,48000),instrument=createInstrument(ctx),source=makeSource(1);
      instrument.set({matter:.6,bloom:1});
      for(let i=0;i<120;i++){
        source.layers[0].wave=['sine','triangle','warm','glass'][i%4];
        source.low=source.mid=source.high=i%2?12:-12;source.gain=1;source.attack=.003;source.decay=.04;source.sustain=1;source.release=.6;
        source.layers[1].octave=i%5-2;
        instrument.strike(48+i%36,1,(i%7)/6,source,i%22,.1+i*.025);
      }
      const buffer=await ctx.startRendering();
      let peak=0,jump=0,squares=0,tail=0,stereo=0,finite=true;
      const left=buffer.getChannelData(0),right=buffer.getChannelData(1);
      for(let i=0;i<left.length;i++){
        const a=left[i],b=right[i];finite&&=Number.isFinite(a)&&Number.isFinite(b);peak=Math.max(peak,Math.abs(a),Math.abs(b));
        if(i)jump=Math.max(jump,Math.abs(a-left[i-1]),Math.abs(b-right[i-1]));
        if(i<48000*4)squares+=a*a+b*b;if(i>48000*13)tail+=a*a+b*b;stereo+=(a-b)**2;
      }
      return {finite,peak,maxSampleStep:jump,rms:Math.sqrt(squares/(48000*8)),tailRms:Math.sqrt(tail/96000),stereoDifference:Math.sqrt(stereo/left.length)};
    });
    assert.ok(report.audio.finite);assert.ok(report.audio.peak<.89);assert.ok(report.audio.rms>.003);assert.ok(report.audio.maxSampleStep<.3);assert.ok(report.audio.tailRms<.001);assert.ok(report.audio.stereoDifference>.001);
    const mobile=await browser.newContext({viewport:{width:390,height:844},deviceScaleFactor:2,isMobile:true,hasTouch:true});
    const phone=await mobile.newPage();phone.on('pageerror',e=>errors.push(e.message));await phone.goto(url);
    await phone.touchscreen.tap(195,375);await phone.waitForTimeout(250);
    assert.equal(await phone.evaluate(()=>synth.ctx.state),'running','Real touch unlocks audio under autoplay policy');
    await phone.locator('[data-tab="layers"]').click();
    await phone.screenshot({path:'shot-viii-phone.png'});
    report.mobile=await phone.evaluate(()=>({width:innerWidth,scroll:document.documentElement.scrollWidth,dock:$('dock').getBoundingClientRect().toJSON(),knobs:[...document.querySelectorAll('.orb-knob')].map(n=>n.getBoundingClientRect().toJSON())}));
    assert.equal(report.mobile.width,report.mobile.scroll);assert.ok(report.mobile.dock.x>=0);assert.ok(report.mobile.dock.bottom<=844);
    for(const knob of report.mobile.knobs)assert.ok(knob.x>=0&&knob.right<=390);
    await phone.setViewportSize({width:844,height:390});await phone.waitForTimeout(100);await phone.screenshot({path:'shot-viii-landscape.png'});
    assert.equal(await phone.evaluate(()=>document.documentElement.scrollWidth),844);
    assert.ok(await phone.evaluate(()=>geometry.right<$('dock').getBoundingClientRect().left),'Landscape reserves a playable field beside the controls');
    await phone.evaluate(()=>{const b=bodies[0];selectBody(b);b.lastHit=-10;sing(b,.7,true,true);gestures.set(77,{body:b});});
    await phone.evaluate(()=>window.dispatchEvent(new Event('blur')));
    assert.equal(await phone.evaluate(()=>synth.held),0,'Focus loss cannot leave a held voice stuck');
    const reduced=await browser.newPage({viewport:{width:390,height:844},reducedMotion:'reduce'});await reduced.goto(url);assert.equal(await reduced.evaluate(()=>paused),true);
    assert.deepEqual(errors,[]);report.errors=errors;console.log(JSON.stringify(report,null,2));
    if(process.env.REPORT_PATH)fs.writeFileSync(process.env.REPORT_PATH,JSON.stringify(report,null,2)+'\n');
  }finally{await browser.close();}
})().catch(error=>{console.error(error);process.exitCode=1;});
