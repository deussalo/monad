/* ---------- motion parts: the cam ---------- */
// A cam rides a cog's axle and turns with it; a follower fixed in the world rides its edge. The follower's lift is a
// value from 0 to 1, one cycle per turn of the cog, in the shape of the cam's outline, so the shape on screen is the
// shape the sound takes. Linked to a synth reader, the cam turns one of its macros: the patch's own routes from that
// macro carry it to cutoff, wavetable position, FX mix or anything else. The worklet computes the motion itself
// (motionAt in wave.worklet.js) from a program posted only when the train or the cam changes.
const CAM_N=64;
const camSay=(read,held)=>{if(typeof showInteraction==='function')showInteraction(read,held)};
const CAM_SHAPES={
  sine:u=>.5-.5*Math.cos(TAU*u),'ramp up':u=>u,'ramp down':u=>1-u,triangle:u=>1-Math.abs(2*u-1),
  square:u=>u<.5?1:0,'steps 4':u=>Math.floor(u*4)/3,'steps 8':u=>Math.floor(u*8)/7,
  random:u=>{const k=Math.floor(u*8);const x=Math.sin(k*12.9898+78.233)*43758.5453;return x-Math.floor(x)}};
const CAM_STEPPED=new Set(['square','steps 4','steps 8','random']);
const camShape=name=>Array.from({length:CAM_N},(_,i)=>+CAM_SHAPES[name](i/CAM_N).toFixed(3));
const newCam=id=>({id,cog:null,ang:0,phase:0,shape:'sine',prof:camShape('sine'),to:null,macro:1,lo:0,hi:1,ver:0});
// The cam's lift where the follower rides, at the cog's angle th: u is the fraction of a turn under the follower.
const camU=(m,th)=>{const u=(m.ang-th)/TAU-m.phase;return u-Math.floor(u)};
function camAt(m,u){const t=m.prof,x=u*CAM_N,i=Math.floor(x)%CAM_N;return m.shape!=='drawn'&&CAM_STEPPED.has(m.shape)?t[i]:t[i]+(t[(i+1)%CAM_N]-t[i])*(x-Math.floor(x))}
const camLift=m=>camAt(m,camU(m,renderTh(m.cog)));
// Its picture: the plate's outline at radius rb·(0.45 + 0.55·lift), the follower a pushrod on the line at angle ang.
const camRb=m=>Math.max(8,radius(m.cog)*.78);
const camRoll=(m,y)=>camRb(m)*(.45+.55*y);
function camTip(m){const c=m.cog,r=camRoll(m,camLift(m))+14;return {x:c.x+Math.cos(m.ang)*r,y:c.y+Math.sin(m.ang)*r}}
function drawCam(m){
  const c=m.cog,th=renderTh(c),rb=camRb(m);
  cx.lineWidth=1.5*PX;cx.strokeStyle=HOT;cx.globalAlpha=.85;cx.beginPath();
  // the plate point at fraction u of a turn lies at world angle th + 2π(u + phase): the point under the follower is camU
  for(let i=0;i<=CAM_N;i++){const u=i/CAM_N,w=th+TAU*(u+m.phase),r=camRoll(m,camAt(m,u%1)),x=c.x+Math.cos(w)*r,y=c.y+Math.sin(w)*r;i?cx.lineTo(x,y):cx.moveTo(x,y)}
  cx.closePath();cx.stroke();cx.globalAlpha=1;
  // the low point's flare, so the phase reads at a glance
  let lo=0;for(let i=1;i<CAM_N;i++)if(m.prof[i]<m.prof[lo])lo=i;
  {const w=th+TAU*(lo/CAM_N+m.phase),r=camRoll(m,m.prof[lo]);cx.fillStyle=HOT;circle(c.x+Math.cos(w)*r,c.y+Math.sin(w)*r,1.8*PX,true)}
  // the follower: a roller on the edge and a rod out along its line
  const y=camLift(m),r=camRoll(m,y),ca=Math.cos(m.ang),sa=Math.sin(m.ang);
  cx.strokeStyle=FG;cx.lineWidth=2*PX;line(c.x+ca*r,c.y+sa*r,c.x+ca*(r+14),c.y+sa*(r+14));
  cx.lineWidth=PX;circle(c.x+ca*r,c.y+sa*r,2.2*PX);cx.fillStyle=FG;circle(c.x+ca*(r+14),c.y+sa*(r+14),2.6*PX,true);
  // its guide: a short fixed sleeve where the rod slides, marked with the travel's range
  const g0=camRoll(m,0)+14,g1=camRoll(m,1)+14;cx.strokeStyle=DIM;cx.lineWidth=PX;
  line(c.x+ca*g0-sa*4*PX,c.y+sa*g0+ca*4*PX,c.x+ca*g1-sa*4*PX,c.y+sa*g1+ca*4*PX);
  line(c.x+ca*g0+sa*4*PX,c.y+sa*g0-ca*4*PX,c.x+ca*g1+sa*4*PX,c.y+sa*g1-ca*4*PX);
}
// The link: a thin rod from the follower to the reader it turns, dashed while the reader is not a synth.
function drawCamLink(m){
  if(!m.to)return;const a=camTip(m),b=mid(m.to),live=m.to.voice.kind==='wave';
  cx.strokeStyle=SEL;cx.lineWidth=PX;cx.globalAlpha=live?.7:.3;cx.setLineDash([2*PX,3*PX]);line(a.x,a.y,b.x,b.y);cx.setLineDash([]);cx.globalAlpha=1;
  cx.fillStyle=SEL;circle(b.x,b.y,2*PX,true);
}

// The worklet's programs: one per linked cam, keyed by patch id and macro slot; posted again only when it changed.
const motionPosted=new Map();
function camProgram(m){
  const c=m.cog,sr=ac.sampleRate,table=m.prof,step=m.shape!=='drawn'&&CAM_STEPPED.has(m.shape);
  if(clock.paused||!c.w)return {table,step,f0:0,ph0:camU(m,renderTh(c)),rate:0,lo:m.lo,hi:m.hi};
  // anchored at the sim's own time, mapped to audio frames as fire() maps strikes, so it lines up with the notes
  return {table,step,f0:(simT+clock.off-clock.shift)*sr,ph0:camU(m,c.th),rate:-c.w/TAU,lo:m.lo,hi:m.hi};
}
function postMotions(){
  if(!waveNode||!ac||!W.cams.length&&!motionPosted.size)return;
  const seen=new Set();
  for(const m of W.cams){
    if(!m.to||!m.cog||m.to.voice.kind!=='wave'||!W.readers.includes(m.to))continue;
    const v=m.to.voice,id=waveIdOf.get(v)||wavePost(v);if(!id)continue;
    const key=id*8+m.macro-1;seen.add(key);
    const p=camProgram(m),old=motionPosted.get(key);
    if(old&&old.cam===m&&old.ver===m.ver&&old.rate===p.rate&&old.lo===p.lo&&old.hi===p.hi&&old.step===p.step){
      const at=old.ph0+old.rate*(p.f0-old.f0)/ac.sampleRate,d=at-p.ph0;if(Math.abs(d-Math.round(d))<1e-4)continue}
    motionPosted.set(key,{...p,cam:m,ver:m.ver});
    waveNode.port.postMessage({type:'motion',id,slot:m.macro-1,prog:{...p,table:Float32Array.from(p.table)}});
  }
  for(const key of motionPosted.keys())if(!seen.has(key)){motionPosted.delete(key);waveNode.port.postMessage({type:'motion',id:Math.floor(key/8),slot:key%8,prog:null})}
}
// Which macros a cam drives on a voice: the synth editor marks them.
const camsOn=v=>W.cams.filter(m=>m.to&&m.to.voice===v);

// Link a cam to a synth reader: it takes the reader's first free macro (no routes, knob at 0), and the reader's
// patch sheet opens with that macro armed, so one drag on a knob finishes the connection.
function linkCam(m,rd){
  if(rd.voice.kind!=='wave'){camSay(()=>'link: that reader is not a synth');return false}
  const v=rd.voice,used=new Set(camsOn(v).filter(o=>o!==m).map(o=>o.macro));
  let slot=v.macro.findIndex((x,i)=>!used.has(i+1)&&!x&&!v.mods.some(r=>r.src==='m'+(i+1)||r.aux==='m'+(i+1)));
  if(slot<0)slot=[...Array(WAVE_MACROS).keys()].find(i=>!used.has(i+1))??0;
  m.to=rd;m.macro=slot+1;buzz(12);changed(true);
  waveUi.arm='m'+m.macro;waveUi.pin=false;if(['MATRIX','PATCH'].includes(waveUi.tab))waveUi.tab='MAIN';
  select('reader',rd);if(typeof openPartEditor==='function')openPartEditor();
  camSay(()=>`cam ${m.id} turns M${m.macro}: drag a knob to set how far`);
  return true;
}
// Linking: the next tap on a reader takes the link; anywhere else cancels it.
function startLink(m){ui.linking=m;buzz(6);camSay(()=>`cam ${m.id}: tap a synth reader to link it`,true)}
function linkGesture(s){
  const m=ui.linking;ui.linking=null;const p=screenToWorld(s);
  const rd=W.readers.map(r=>[r,segDist(p,r.x1,r.y1,r.x2,r.y2)*S.zoom]).filter(([,d])=>d<24).sort((a,b)=>a[1]-b[1])[0]?.[0];
  if(rd)linkCam(m,rd);else camSay(()=>'link: cancelled');
  return {o:null,move(){},up(){},tap(){},cancel(){}};
}

// The profile strip: one turn unrolled left to right; drag to draw the lift. It turns the cam's shape to 'drawn'.
function camStrip(m,f){
  const cv=el('canvas',{className:'cam-strip',width:280*dpr,height:72*dpr}),H=72;
  const draw=()=>{const W_=cv.clientWidth||280;if(cv.width!==Math.round(W_*dpr)){cv.width=Math.round(W_*dpr);cv.height=H*dpr}
    const x=cv.getContext('2d');x.setTransform(dpr,0,0,dpr,0,0);x.clearRect(0,0,W_,H);
    x.strokeStyle=css('--line')||'#333';x.lineWidth=1;for(let k=1;k<4;k++){x.beginPath();x.moveTo(W_*k/4,0);x.lineTo(W_*k/4,H);x.stroke()}
    x.strokeStyle=css('--hot');x.lineWidth=2;x.beginPath();
    for(let i=0;i<=CAM_N;i++){const u=i/CAM_N,y=H-4-camAt(m,u%1)*(H-8);i?x.lineTo(u*W_,y):x.moveTo(0,y)}x.stroke();
    if(m.cog){const u=camU(m,renderTh(m.cog));x.fillStyle=css('--fg');x.fillRect(u*W_-1,0,2,H)}};
  let last=null;
  const paint=e=>{const r=cv.getBoundingClientRect(),i=clamp(Math.floor((e.clientX-r.left)/r.width*CAM_N),0,CAM_N-1),y=+clamp(1-(e.clientY-r.top-4)/(H-8),0,1).toFixed(3);
    const a=last??i;for(let k=Math.min(a,i);k<=Math.max(a,i);k++)m.prof[k]=y;last=i;m.shape='drawn';m.ver++;draw()};
  cv.onpointerdown=e=>{cv.setPointerCapture(e.pointerId);last=null;paint(e)};
  cv.onpointermove=e=>{if(e.buttons||e.pointerType==='touch')if(last!==null)paint(e)};
  cv.onpointerup=cv.onpointercancel=()=>{if(last!==null){last=null;changed()}};
  f.append(cv);requestAnimationFrame(draw);camStrips.push(draw);
}
let camStrips=[];
function camRows(f,m){
  camStrips=[];camStrip(m,f);
  selRow(f,'shape',[...Object.keys(CAM_SHAPES),'drawn'],()=>m.shape,x=>{if(x!=='drawn'){m.prof=camShape(x)}m.shape=x;m.ver++;changed()});
  numRow(f,'phase',0,1,.01,()=>m.phase,x=>{m.phase=x;changed()},0);
  numRow(f,'low',0,1,.01,()=>m.lo,x=>{m.lo=x;changed()},0);
  numRow(f,'high',0,1,.01,()=>m.hi,x=>{m.hi=x;changed()},1);
  const synths=()=>W.readers.filter(r=>r.voice.kind==='wave');
  selRow(f,'turns',['none',...synths().map(r=>String(r.id))],()=>m.to?String(m.to.id):'none',x=>{if(x==='none'){m.to=null;changed(true)}else{const r=W.readers.find(r=>String(r.id)===x);if(r)linkCam(m,r)}},x=>x==='none'?'nothing':'reader '+x);
  selRow(f,'macro',['1','2','3','4','5','6','7','8'],()=>String(m.macro),x=>{m.macro=+x;changed(true)},x=>'M'+x);
  roRow(f,'travel',()=>m.cog?Math.round(camLift(m)*100)+'%':'no cog');
}
const camDrawStrips=()=>{for(const d of camStrips)d()};
