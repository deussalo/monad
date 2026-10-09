const TAU=Math.PI*2,TOOTH=12,STORE='cogs.v1';
// A CSP-sandboxed page has an opaque origin, where touching localStorage throws.
const storage=(()=>{try{localStorage.getItem(STORE);return {ok:true,get:()=>localStorage.getItem(STORE),put:v=>localStorage.setItem(STORE,v)}}
  catch(e){return {ok:false,why:e.name,get:()=>null,put:()=>{}}}})();
const $=s=>document.querySelector(s);
const clamp=(x,a,b)=>Math.min(b,Math.max(a,x));
const wrap=a=>((a%TAU)+TAU)%TAU;
const hz=m=>440*Math.pow(2,(m-69)/12);
const radius=c=>c.n*TOOTH/TAU;

const S={bpm:0,beats:4,seed:1,meshTol:4,coaxTol:6,snapTol:16,vRef:500,velCurve:.72,master:.7,lookahead:.06,maxStep:.05,zoom:1,windows:0,ids:0};
const S_DEFS=[['bpm','bpm lock',0,300,1],['beats','beats/bar',1,16,1],['seed','seed',0,9999,1],['meshTol','mesh tol',0,20,.5],['coaxTol','coax tol',0,20,.5],['snapTol','snap tol',0,60,1],
  ['vRef','v ref px/s',20,3000,1],['velCurve','vel curve',.1,3,.01],['master','master',0,1.5,.01],
  ['lookahead','lookahead s',.01,.3,.005],['maxStep','step rad',.005,.3,.005],['zoom','zoom',.1,8,.01],
  ['windows','windows',0,1,1],['ids','ids',0,1,1]];
const SPACE={dry:1,wet:.35,predelay:12,size:1,decay:2.5,lowMult:1.1,highMult:.55,lowCross:200,highCross:4500,
  diffusion:.78,density:.55,modRate:.3,modDepth:.5,shimmer:0,shimmerPitch:12,freeze:0,width:1.1,duck:.3,lowCut:110,highCut:15000};
const SPACE_DEFS=[['dry','dry',0,1,.001],['wet','wet',0,1.5,.001],['predelay','predelay ms',0,500,1],
  ['size','size',.2,3,.01],['decay','T60 s',.3,60,.01],['lowMult','low decay',.25,3,.01],
  ['highMult','high decay',.05,1.5,.01],['lowCross','low x Hz',40,1200,1],['highCross','high x Hz',800,14000,1],
  ['diffusion','diffusion',0,1,.01],['density','density',0,1,.01],['modRate','mod Hz',.02,3,.01],
  ['modDepth','mod ms',0,3,.01],['shimmer','shimmer',0,1,.01],['shimmerPitch','shim st',-12,24,1],
  ['freeze','freeze',0,1,1],['width','width',0,1.5,.01],['duck','duck',0,1,.01],
  ['lowCut','wet lo Hz',20,1000,1],['highCut','wet hi Hz',1500,20000,1]];

const BODIES={harmonic:[1,2,3,4,5],bell:[1,1.183,1.506,2.0,2.514],plate:[1,1.594,2.136,2.653,3.156],
  bar:[1,2.756,5.404,8.933,13.34],bowl:[1,2.32,4.25,6.63,9.38]};
const SCALES={chromatic:[0,1,2,3,4,5,6,7,8,9,10,11],major:[0,2,4,5,7,9,11],minor:[0,2,3,5,7,8,10],
  dorian:[0,2,3,5,7,9,10],pentatonic:[0,2,4,7,9],minpent:[0,3,5,7,10],whole:[0,2,4,6,8,10]};
const VOICES={modal:{note:60,body:'bell',bend:1,bright:.6,hard:.6,decay:1.2,damp:.8},
  kick:{tune:52,sweep:3,pdecay:.045,decay:.35},snare:{tune:190,tone:.35,cutoff:1800,decay:.16},hat:{cutoff:7500,decay:.045}};
const VOICE_DEFS={
  modal:[['note','note',12,108,1],['body','body',Object.keys(BODIES)],['bend','bend',0,1,.01],['bright','bright',0,1,.01],
    ['hard','hard',0,1,.01],['decay','decay s',.02,8,.01],['damp','damp',0,2,.01]],
  kick:[['tune','tune Hz',20,200,1],['sweep','sweep x',1,10,.1],['pdecay','p decay s',.005,.3,.001],['decay','decay s',.02,2,.01]],
  snare:[['tune','tune Hz',60,600,1],['tone','tone',0,1,.01],['cutoff','cutoff Hz',200,12000,10],['decay','decay s',.02,1,.005]],
  hat:[['cutoff','cutoff Hz',1000,16000,10],['decay','decay s',.005,1,.005]]};
const COMMON_DEFS=[['level','level',0,1.5,.01],['velSense','vel sense',0,1,.01]];
const makeVoice=(kind,common={level:.7,velSense:.7})=>({...common,...VOICES[kind],kind});
const S_DEFAULT={...S},SPACE_DEFAULT={...SPACE};

/* ---------- world ---------- */
const W={cogs:[],arms:[],pads:[],readers:[],belts:[],strings:[],next:1};
let simT=0,sel=null,group=[],marquee=null;
const view={x:0,y:0};
const stats={events:0,late:0,steps:0,stepsTick:0,log:[]};
const clock={off:0,shift:0,audio:false,view:0,paused:false};

const stackOf=c=>W.cogs.filter(o=>Math.hypot(o.x-c.x,o.y-c.y)<=S.coaxTol);
const renderTh=c=>c.th-c.w*(simT-clock.view);
const ringR=(c,i)=>{const n=c.track.rings,r=radius(c);return n>1?r*(.25+.65*i/(n-1)):r*.6};
const scaleNote=(base,scale,i)=>{const sc=SCALES[scale];return base+sc[i%sc.length]+12*Math.floor(i/sc.length)};
const ringNote=(t,i)=>scaleNote(t.base,t.scale,i);
const motorRpm=c=>S.bpm&&c.revs?c.revs*S.bpm/S.beats:c.motor;
function chance(...n){let h=(S.seed|0)^0x9e3779b9;for(const x of n){h=Math.imul(h^Math.round(x*4096),0x85ebca6b);h^=h>>>13;h=Math.imul(h,0xc2b2ae35);h^=h>>>16}return (h>>>0)/4294967296}

// How two cogs touch: on one axle, teeth in mesh, overlapping (a jam), or apart.
function contact(a,b){
  const d=Math.hypot(a.x-b.x,a.y-b.y);
  if(d<=S.coaxTol)return 'axle';
  if(a.layer!==b.layer)return null;
  const gap=d-radius(a)-radius(b);
  return Math.abs(gap)<=S.meshTol?'mesh':gap<0?'jam':null;
}
function solve(){
  const cs=W.cogs,adj=new Map(cs.map(c=>[c,[]]));
  for(const c of cs){c.jam=false;c.k=null;c.w=0}
  for(let i=0;i<cs.length;i++)for(let j=i+1;j<cs.length;j++){
    const a=cs[i],b=cs[j],how=contact(a,b);
    if(how==='axle'){adj.get(a).push([b,1]);adj.get(b).push([a,1])}
    else if(how==='mesh'){adj.get(a).push([b,-a.n/b.n]);adj.get(b).push([a,-b.n/a.n])}
    else if(how==='jam')a.jam=b.jam=true;
  }
  for(const b of W.belts){const s=b.cross?-1:1;adj.get(b.a).push([b.b,s*b.a.n/b.b.n]);adj.get(b.b).push([b.a,s*b.b.n/b.a.n])}
  for(const root of cs){
    if(root.k!==null)continue;
    root.k=1;const comp=[root];let jam=false;
    for(let q=0;q<comp.length;q++){const a=comp[q];for(const[b,ratio]of adj.get(a)){const k=a.k*ratio;
      if(b.k===null){b.k=k;comp.push(b)}else if(Math.abs(b.k-k)>1e-9*Math.abs(k))jam=true}}
    if(comp.some(c=>c.jam))jam=true;
    const drivers=comp.filter(c=>motorRpm(c)),driver=drivers[0]||null;
    const wRoot=driver?motorRpm(driver)*TAU/60/driver.k:0;
    for(const d of drivers)if(Math.abs(d.k*wRoot-motorRpm(d)*TAU/60)>1e-9*Math.abs(wRoot))jam=true;
    for(const c of comp){c.jam=jam;c.w=jam?0:c.k*wRoot;c.driver=driver}
  }
}

function snap(c){
  let best=null,bestGap=Infinity;
  for(const o of W.cogs){
    if(o===c||o.layer!==c.layer)continue;
    const d=Math.hypot(c.x-o.x,c.y-o.y);if(d<=S.coaxTol)continue;
    const g=Math.abs(d-radius(o)-radius(c));if(g<bestGap){bestGap=g;best=o}
  }
  if(!best||bestGap>S.snapTol)return;
  const d=Math.hypot(c.x-best.x,c.y-best.y)||1,R=radius(best)+radius(c);
  c.x=best.x+(c.x-best.x)/d*R;c.y=best.y+(c.y-best.y)/d*R;
  interlock(c,best);
}
// Change a cog's tooth count keeping every mesh: it grows away from the neighbour toward its driver,
// and whatever it drives slides along its line of centres, carrying its axle-mates and the readers on it.
function resizer(c){
  const cs=W.cogs,links=new Map(cs.map(o=>[o,[]]));
  for(let i=0;i<cs.length;i++)for(let j=i+1;j<cs.length;j++){const how=contact(cs[i],cs[j]);
    if(how==='axle'||how==='mesh'){links.get(cs[i]).push([cs[j],how]);links.get(cs[j]).push([cs[i],how])}}
  const stack=new Set(stackOf(c)),meshed=[...stack].flatMap(o=>links.get(o).filter(([,how])=>how==='mesh').map(([m])=>m)).filter(m=>!stack.has(m));
  let anchor=meshed[0]||null;
  if(c.driver&&!stack.has(c.driver)){
    const seen=new Set([c.driver]),q=[c.driver];
    for(let i=0;i<q.length&&!q.some(o=>meshed.includes(o));i++)for(const[o]of links.get(q[i]))if(!seen.has(o)&&!stack.has(o)){seen.add(o);q.push(o)}
    anchor=q.find(o=>meshed.includes(o))||anchor;
  }
  const at=new Map([...cs,...W.readers].map(o=>[o,{x:o.x,y:o.y,x1:o.x1,y1:o.y1,x2:o.x2,y2:o.y2}])),n0=c.n;
  const carried=o=>W.readers.filter(r=>Math.hypot(at.get(r).x1-at.get(o).x,at.get(r).y1-at.get(o).y)<radius({n:o===c?n0:o.n}));
  const shift=(o,dx,dy)=>{const p=at.get(o);o.x=p.x+dx;o.y=p.y+dy;
    for(const r of carried(o)){const q=at.get(r);Object.assign(r,{x1:q.x1+dx,y1:q.y1+dy,x2:q.x2+dx,y2:q.y2+dy})}};
  return {anchor,
    // The radius whose rim passes through world point q while the contact with the anchor stays put.
    radiusThrough(q){
      if(!anchor)return Math.hypot(q.x-c.x,q.y-c.y);
      const a=at.get(anchor),p=at.get(c),d=Math.hypot(p.x-a.x,p.y-a.y),ux=(p.x-a.x)/d,uy=(p.y-a.y)/d,ra=radius(anchor);
      const tx=a.x+ux*ra,ty=a.y+uy*ra,qx=q.x-tx,qy=q.y-ty,along=qx*ux+qy*uy;
      return along>8?(qx*qx+qy*qy)/(2*along):Math.hypot(q.x-p.x,q.y-p.y);
    },
    set(n){
      c.n=n;
      for(const[o,p]of at)Object.assign(o,o.x1===undefined?{x:p.x,y:p.y}:{x1:p.x1,y1:p.y1,x2:p.x2,y2:p.y2});
      if(anchor){
        const done=new Set([anchor]),q=[],move=(o,dx,dy)=>{for(const m of stackOf({x:at.get(o).x,y:at.get(o).y}).filter(m=>at.has(m)&&!done.has(m))){done.add(m);shift(m,dx,dy);q.push(m)}};
        const a=at.get(anchor),p=at.get(c),d=Math.hypot(p.x-a.x,p.y-a.y),R=radius(anchor)+radius(c);
        move(c,a.x+(p.x-a.x)/d*R-p.x,a.y+(p.y-a.y)/d*R-p.y);interlock(c,anchor);
        for(let i=0;i<q.length;i++){const m=q[i],pm=at.get(m);
          for(const[o,how]of links.get(m)){if(how!=='mesh'||done.has(o))continue;
            const po=at.get(o),e=Math.hypot(po.x-pm.x,po.y-pm.y),Rm=radius(m)+radius(o);
            move(o,m.x+(po.x-pm.x)/e*Rm-po.x,m.y+(po.y-pm.y)/e*Rm-po.y);interlock(o,m)}}
      }
      solve();
    }};
}
function interlock(c,partner){
  const phi=Math.atan2(c.y-partner.y,c.x-partner.x),toothOnPartner=(phi-partner.th)*partner.n/TAU;
  c.th=phi+Math.PI-(.5-toothOnPartner)*TAU/c.n;
}

function hitWindow(c,arm,pad){
  const dx=pad.x-c.x,dy=pad.y-c.y,D=Math.hypot(dx,dy),rho=pad.r;
  if(D<=rho)return null;
  const s=clamp(Math.sqrt(D*D-rho*rho),Math.max(arm.r0,1e-6),arm.r0+arm.len),cosd=(s*s+D*D-rho*rho)/(2*s*D);
  if(cosd>=1||cosd<=-1)return null;
  return {beta:Math.atan2(dy,dx),half:Math.acos(cosd),s};
}

function armHit(p,t){
  const {c,arm,pad,edge,s}=p,px=c.x+Math.cos(edge)*s,py=c.y+Math.sin(edge)*s;
  const nl=Math.hypot(px-pad.x,py-pad.y)||1,nx=(px-pad.x)/nl,ny=(py-pad.y)/nl;
  const vn=Math.abs(c.w*s*(-Math.sin(edge)*nx+Math.cos(edge)*ny));
  pad.hits++;pad.lastV=vn;pad.flash=arm.flash=t;flex(arm,t,vn);
  const v=pad.voice,key=pad.id*128+(v.note|0);
  fire(v,t,vn,pad.x,'arm'+arm.id+'>pad'+pad.id,1,(when,amp,pan,vx)=>v.kind==='modal'?modal(v,v.note,when,amp,pan,key,vx):drum(v,when,amp,pan));
}

function circleCross(c,R,a0,dt,seg){
  const a1=a0+c.w*dt,x0=c.x+Math.cos(a0)*R,y0=c.y+Math.sin(a0)*R,x1=c.x+Math.cos(a1)*R,y1=c.y+Math.sin(a1)*R;
  const ex=seg.x2-seg.x1,ey=seg.y2-seg.y1,s0=ex*(y0-seg.y1)-ey*(x0-seg.x1),s1=ex*(y1-seg.y1)-ey*(x1-seg.x1);
  if(!(s0<0&&s1>=0||s0>0&&s1<=0))return null;
  const f=s0/(s0-s1),qx=x0+(x1-x0)*f,qy=y0+(y1-y0)*f,L2=ex*ex+ey*ey,u=((qx-seg.x1)*ex+(qy-seg.y1)*ey)/L2;
  if(u<0||u>1)return null;
  const am=a0+c.w*dt*f;
  return {f,u,x:qx,am,vn:Math.abs(c.w*R*(Math.sin(am)*ey+Math.cos(am)*ex))/Math.sqrt(L2)};
}
function dotCross(c,dot,rd,dt){
  const hit=circleCross(c,ringR(c,dot.ring),c.th+dot.a,dt,rd);if(!hit)return;
  const t=simT+dt*hit.f,turn=c.turns+Math.floor((c.th+c.w*dt*hit.f)/TAU);
  if(dot.p<1&&chance(c.id,dot.ring,dot.a,turn)>=dot.p)return;
  const midi=rd.tines?scaleNote(rd.combBase,rd.combScale,Math.min(rd.tines-1,Math.floor(hit.u*rd.tines))):ringNote(c.track,dot.ring);
  rd.hits++;rd.lastV=hit.vn;rd.lastMidi=midi;dot.flash=rd.flash=t;
  const v=rd.voice,key=rd.id*128+midi;
  fire(v,t,hit.vn,hit.x,'cog'+c.id+'.ring'+dot.ring+'>reader'+rd.id,dot.v,
    (when,amp,pan,vx)=>v.kind==='modal'?modal(v,midi,when,amp,pan,key,vx):drum(v,when,amp,pan));
}
const stringLen=s=>Math.hypot(s.x2-s.x1,s.y2-s.y1);
function stringMidi(s){
  const L=Math.max(1,stringLen(s)),f0=s.kind==='bar'?s.f200*(200/L)**2:s.f200*200/L,m=69+12*Math.log2(f0/440);
  return s.quant?Math.round(m):m;
}
function pluck(arm,s,dt){
  const c=arm.cog,hit=circleCross(c,arm.r0+arm.len,c.th+arm.a,dt,s);if(!hit)return;
  const t=simT+dt*hit.f,midi=stringMidi(s);
  s.hits++;s.lastV=hit.vn;s.lastU=hit.u;s.flash=t;flex(arm,t,hit.vn*.5);
  fire(s,t,hit.vn,hit.x,'arm'+arm.id+'>string'+s.id,1,(when,amp,pan,vx)=>pluckSound(s,midi,hit.u,when,amp,pan,vx));
}
function pluckSound(s,midi,u,when,amp,pan,vx){
  const f0=hz(midi),ratios=s.kind==='bar'?BODIES.bar:BODIES.harmonic,f=[],g=[],tt=[];
  for(let k=0;k<ratios.length;k++){f.push(f0*ratios[k]);g.push(Math.abs(Math.sin((k+1)*Math.PI*u))/(k+1));tt.push(s.decay*Math.pow(ratios[k],-s.damp))}
  strike(f,g,tt,when,amp,pan,s.id*128+Math.round(midi),contactTime(s.hard,vx));
}
function flex(arm,t,vn){arm.bendT=t;arm.bendA=Math.min(.6,vn/S.vRef*.5)*arm.flex*(arm.cog.w>0?-1:1)}

function turn(c,dt){const th=c.th+c.w*dt,k=Math.floor(th/TAU);c.th=th-k*TAU;c.turns+=k}
function advance(tEnd){
  if(tEnd-simT>.5){const gap=tEnd-S.lookahead-simT;for(const c of W.cogs)turn(c,gap);simT+=gap}
  const pairs=[];
  for(const arm of W.arms){const c=arm.cog;if(!c.w)continue;
    for(const pad of W.pads){const h=hitWindow(c,arm,pad);if(h)pairs.push({c,arm,pad,s:h.s,edge:c.w>0?h.beta-h.half:h.beta+h.half})}}
  const plucks=[];
  for(const arm of W.arms)if(arm.cog.w)for(const s of W.strings)plucks.push([arm,s]);
  const tracks=W.readers.length?W.cogs.filter(c=>c.track&&c.w&&c.track.dots.length):[];
  let maxW=0;for(const c of W.cogs)maxW=Math.max(maxW,Math.abs(c.w));
  stats.stepsTick=0;
  while(simT<tEnd){
    const dt=Math.min(tEnd-simT,maxW?S.maxStep/maxW:Infinity);
    for(const p of pairs){
      const psi=p.c.th+p.arm.a,span=Math.abs(p.c.w)*dt,d0=p.c.w>0?wrap(p.edge-psi):wrap(psi-p.edge);
      if(d0>0&&d0<=span)armHit(p,simT+dt*d0/span);
    }
    for(const c of tracks)for(const dot of c.track.dots)for(const rd of W.readers)dotCross(c,dot,rd,dt);
    for(const[arm,s]of plucks)pluck(arm,s,dt);
    for(const c of W.cogs)turn(c,dt);
    simT+=dt;stats.steps++;stats.stepsTick++;
  }
}

/* ---------- audio ---------- */
let ac=null,engineNode=null,engineState='off',engineStats={},master=null,meter=null,drumBus=null,noise=null,peak=0;
const strikes=[];
function initAudio(){
  if(ac){if(ac.state!=='running')ac.resume();return}
  ac=new (window.AudioContext||window.webkitAudioContext)({latencyHint:'interactive'});
  master=ac.createGain();master.gain.value=S.master;
  const lim=ac.createDynamicsCompressor();
  lim.threshold.value=-10;lim.knee.value=8;lim.ratio.value=3.5;lim.attack.value=.015;lim.release.value=.3;
  meter=ac.createAnalyser();meter.fftSize=2048;
  master.connect(lim);lim.connect(ac.destination);lim.connect(meter);
  noise=ac.createBuffer(1,ac.sampleRate,ac.sampleRate);
  const nd=noise.getChannelData(0);for(let i=0;i<nd.length;i++)nd[i]=Math.random()*2-1;
  engineState='loading';
  // Chrome refuses blob: worklet modules on file:// pages; a data: URL loads there and over https.
  const url='data:text/javascript;charset=utf-8,'+encodeURIComponent($('#engine').textContent);
  ac.audioWorklet.addModule(url).then(()=>{
    engineNode=new AudioWorkletNode(ac,'cogs-engine',{numberOfInputs:1,numberOfOutputs:1,outputChannelCount:[2],
      channelCount:2,channelCountMode:'explicit',processorOptions:{voices:64,lines:16,space:{...SPACE}}});
    engineNode.port.onmessage=e=>{if(e.data.type==='stats')engineStats=e.data};
    engineNode.onprocessorerror=()=>{engineState='processor error'};
    drumBus=ac.createGain();drumBus.connect(engineNode);engineNode.connect(master);
    engineState='ready';
  }).catch(err=>{engineState='failed: '+String(err.message).slice(0,160)});
  ac.resume();
}
function flushStrikes(){const d=new Float64Array(strikes);strikes.length=0;engineNode.port.postMessage({type:'strikes',d},[d.buffer])}
const contactTime=(hard,x)=>clamp((.0016-.0014*hard)*Math.pow(Math.max(.02,x),-.35),.00012,.004);
function strike(f,g,t,when,amp,pan,key,contact){
  if(!strikes.length)queueMicrotask(flushStrikes);
  strikes.push(when*ac.sampleRate,key,amp*.3,contact,0,pan,f.length,...f,...g,...t);
}
function modal(v,midi,when,amp,pan,key,x){
  const H=BODIES.harmonic,F=BODIES[v.body],f0=hz(midi),f=[],g=[],t=[];
  for(let k=0;k<H.length;k++){const r=H[k]+(F[k]-H[k])*v.bend;f.push(f0*r);g.push(k?Math.pow(v.bright,k):1);t.push(v.decay*Math.pow(r,-v.damp))}
  strike(f,g,t,when,amp,pan,key,contactTime(v.hard,x));
}
function drum(v,when,amp,pan){
  const g=ac.createGain(),p=ac.createStereoPanner(),end=when+v.decay;
  p.pan.value=pan;g.connect(p);p.connect(drumBus);
  g.gain.setValueAtTime(amp,when);g.gain.exponentialRampToValueAtTime(1e-4,end);
  const src=[];
  if(v.kind==='kick'){
    const o=ac.createOscillator();o.frequency.setValueAtTime(v.tune*v.sweep,when);
    o.frequency.exponentialRampToValueAtTime(v.tune,when+v.pdecay);o.connect(g);src.push(o);
  }else{
    const n=ac.createBufferSource(),f=ac.createBiquadFilter(),ng=ac.createGain();
    n.buffer=noise;n.loop=true;f.type=v.kind==='hat'?'highpass':'bandpass';f.frequency.value=v.cutoff;f.Q.value=.8;
    ng.gain.value=v.kind==='snare'?1-v.tone:1;n.connect(f);f.connect(ng);ng.connect(g);src.push(n);
    if(v.kind==='snare'){const o=ac.createOscillator(),tg=ac.createGain();o.type='triangle';o.frequency.value=v.tune;
      tg.gain.value=v.tone;o.connect(tg);tg.connect(g);src.push(o)}
  }
  for(const s of src){s.start(when);s.stop(end+.02)}
}
function fire(v,t,vn,x,src,gain,play){
  stats.events++;
  stats.log.push({t,src,vn});if(stats.log.length>256)stats.log.shift();
  if(!clock.audio||engineState!=='ready')return;
  const vx=clamp(vn/S.vRef,0,1),amp=gain*v.level*(1-v.velSense+v.velSense*Math.pow(vx,S.velCurve));
  if(amp<=0)return;
  let when=t+clock.off-clock.shift;
  if(when<ac.currentTime){stats.late++;when=ac.currentTime}
  play(when,amp,clamp((x-view.x)/300,-.9,.9),vx);
}

/* ---------- clock ---------- */
const audioLive=()=>!!(ac&&ac.state==='running'&&engineState==='ready');
const clockBase=()=>clock.audio?ac.currentTime:performance.now()/1000;
const nowSec=()=>clockBase()+clock.shift;
function syncClock(){
  const live=audioLive();if(live===clock.audio)return;
  const before=nowSec();clock.audio=live;clock.shift=before-clockBase();
}
function setPaused(p){
  if(p===clock.paused)return;
  clock.paused=p;if(!p)clock.off=nowSec()-clock.view;
}


/* ---------- persistence ---------- */
let saveTimer=0;
function roomEdited(){applyAudioSettings();changed()}
function changed(structural){
  if(structural)solve();
  clearTimeout(saveTimer);saveTimer=setTimeout(()=>storage.put(JSON.stringify(serialize())),400);
  edits.mark();
  refreshInspector();
}
// Undo: one step per finished action. A step settles when the pointer lifts, or after a quiet
// spell for wheel and keyboard edits. Camera, pause, selection and cog phase are never undone.
const worldKey=()=>{const j=JSON.parse(JSON.stringify(serialize()));delete j.view;delete j.S.zoom;for(const c of j.cogs)delete c.th;return JSON.stringify(j)};
const EDITS_MAX=500,EDITS_QUIET=350;
const edits={past:[],future:[],cur:null,pending:false,timer:0,held:false,
  snap(){return {key:worldKey(),t:simT,phase:new Map(W.cogs.map(c=>[c.id,{th:c.th,w:c.w}]))}},
  mark(){this.pending=true;clearTimeout(this.timer);this.timer=setTimeout(()=>{if(!this.held)this.commit()},EDITS_QUIET)},
  commit(){
    clearTimeout(this.timer);this.pending=false;const s=this.snap();
    if(this.cur&&s.key===this.cur.key)return;
    if(this.cur){this.past.push(this.cur);if(this.past.length>EDITS_MAX)this.past.shift();this.future.length=0}
    this.cur=s;
  },
  reset(){clearTimeout(this.timer);this.pending=false;this.past.length=0;this.future.length=0;this.cur=this.snap()},
  step(from,to){if(this.pending)this.commit();if(!from.length)return false;to.push(this.cur);this.cur=from.pop();restore(this.cur);return true},
  undo(){return this.step(this.past,this.future)},
  redo(){return this.step(this.future,this.past)},
};
// Put the parts of a snapshot back without touching the clock: each cog takes the phase it
// would have now had it kept the speed it had then, and keeps its revolution count.
function restore(s){
  const turns=new Map(W.cogs.map(c=>[c.id,c.turns])),kept=sel&&{type:sel.type,id:sel.o.id};
  const j=JSON.parse(s.key);Object.assign(j.S,{zoom:S.zoom});j.view={...view};
  for(const c of j.cogs){const p=s.phase.get(c.id);c.th=wrap(p.th+p.w*(simT-s.t))}
  deserialize(j);
  for(const c of W.cogs)c.turns=turns.get(c.id)??0;
  const back=kept&&W[kept.type+'s']?.find(o=>o.id===kept.id);if(back)select(kept.type,back);
  clearTimeout(edits.timer);edits.pending=false;
}
addEventListener('pointerdown',()=>{edits.held=true},true);
for(const type of ['pointerup','pointercancel'])addEventListener(type,()=>{edits.held=false;if(edits.pending)edits.commit()});
function serialize(){
  return {S,SPACE,view,
    cogs:W.cogs.map(c=>({id:c.id,x:c.x,y:c.y,n:c.n,motor:c.motor,revs:c.revs,layer:c.layer,th:wrap(c.th),
      track:c.track&&{...c.track,dots:c.track.dots.map(d=>({ring:d.ring,a:d.a,p:d.p,v:d.v}))}})),
    arms:W.arms.map(a=>({id:a.id,cog:a.cog.id,a:a.a,r0:a.r0,len:a.len,flex:a.flex})),
    pads:W.pads.map(p=>({id:p.id,x:p.x,y:p.y,r:p.r,voice:p.voice})),
    readers:W.readers.map(r=>({id:r.id,x1:r.x1,y1:r.y1,x2:r.x2,y2:r.y2,voice:r.voice,tines:r.tines,combBase:r.combBase,combScale:r.combScale})),
    belts:W.belts.map(b=>({id:b.id,a:b.a.id,b:b.b.id,cross:b.cross})),
    strings:W.strings.map(({id,x1,y1,x2,y2,kind,f200,quant,decay,damp,hard,level,velSense})=>({id,x1,y1,x2,y2,kind,f200,quant,decay,damp,hard,level,velSense}))};
}
function deserialize(j){
  const cogs=j.cogs.map(c=>({revs:0,...c,w:0,turns:0}));
  const byId=new Map(cogs.map(c=>[c.id,c]));
  const cog=(id,who)=>{const c=byId.get(id);if(!c)throw new Error(`${who}: no cog ${id}`);return c};
  const arms=j.arms.map(a=>({flex:.5,...a,cog:cog(a.cog,'arm '+a.id)}));
  W.belts=(j.belts||[]).map(b=>({...b,a:cog(b.a,'belt '+b.id),b:cog(b.b,'belt '+b.id)}));
  W.strings=(j.strings||[]).map(s=>({...s,hits:0,lastV:0}));
  for(const c of cogs)if(c.track)for(const d of c.track.dots){d.p??=1;d.v??=1}
  for(const o of [...j.pads,...j.readers])if(!VOICES[o.voice.kind])throw new Error(`${o.id}: voice kind ${o.voice.kind}`);
  W.cogs=cogs;W.arms=arms;
  W.pads=j.pads.map(p=>({...p,hits:0,lastV:0}));W.readers=j.readers.map(r=>({tines:0,combBase:60,combScale:'major',...r,hits:0,lastV:0}));
  W.next=1+Math.max(0,...[...W.cogs,...W.arms,...W.pads,...W.readers,...W.belts,...W.strings].map(o=>o.id));
  Object.assign(S,j.S);Object.assign(SPACE,j.SPACE);Object.assign(view,j.view);
  select(null);solve();applyAudioSettings();syncRows();changed();
}
const DEFAULT_WORLD=
//@include default-world.json
;
function demo(){deserialize(structuredClone(DEFAULT_WORLD));const f=fitTarget();Object.assign(view,{x:f.x,y:f.y});S.zoom=f.z}

const b64url={enc:u=>{let s='';for(const b of u)s+=String.fromCharCode(b);return btoa(s).replace(/\+/g,'-').replace(/\//g,'_').replace(/=+$/,'')},
  dec:s=>Uint8Array.from(atob(s.replace(/-/g,'+').replace(/_/g,'/')),c=>c.charCodeAt(0))};
const pipe=(bytes,stream)=>new Response(new Blob([bytes]).stream().pipeThrough(stream)).arrayBuffer();
async function shareURL(){
  const packed=new Uint8Array(await pipe(new TextEncoder().encode(JSON.stringify(serialize())),new CompressionStream('deflate-raw')));
  return location.href.split('#')[0]+'#w='+b64url.enc(packed);
}
async function loadShared(code){deserialize(JSON.parse(new TextDecoder().decode(await pipe(b64url.dec(code),new DecompressionStream('deflate-raw')))))}
