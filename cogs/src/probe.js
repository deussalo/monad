/* ---------- the probe: any moving point, as a parameter ---------- */
// A probe pins onto something that moves: an arm (anywhere along it), a point on a cog or disc, a cam's follower, or
// a ball in a box. It reads one axis of that point's travel (up–down, left–right, or reach from the axle) as a value
// from 0 to 1 over the whole of its travel, and drives one parameter of a voiced part with it:
// - a synth (wave) voice: one of its macros, so the patch's routes carry it anywhere; the worklet computes it on
//   the notes' clock from a table of one turn, as it does a cam's (a ball's is streamed, it has no closed form);
// - any other voice (drums, modal, its level): the parameter is set at each strike from where the probe is then.
const PROBE_N=128,PROBE_AXES=['y','x','reach'],PROBE_AXIS_LABEL={y:'up–down',x:'left–right',reach:'reach'};
const newProbe=id=>({id,host:null,on:null,ball:0,r:0,a:0,axis:'y',to:null,macro:1,param:null,lo:0,hi:1});
const probeCog=pr=>pr.host==='arm'?pr.on.cog:pr.host==='cog'?pr.on:pr.host==='cam'?pr.on.cog:null;
const probeBall=pr=>pr.host==='ball'?pr.on.balls.find(b=>b.id===pr.ball)||null:null;
function probeAlive(pr){
  const o=pr.on;if(!o)return false;
  if(pr.host==='arm')return W.arms.includes(o)&&!!o.cog;
  if(pr.host==='cog')return W.cogs.includes(o);
  if(pr.host==='cam')return W.cams.includes(o)&&!!o.cog;
  return pr.host==='ball'&&W.pits.includes(o)&&!!probeBall(pr);
}
const voicedType=o=>W.readers.includes(o)?'reader':W.pads.includes(o)?'pad':W.pits.includes(o)?'pit':null;
const voicedName=o=>{const t=voicedType(o);return t?`${t==='pit'?'sphere':t} ${o.id}`:'nothing'};
const probeHostName=pr=>!pr.on?'nothing':pr.host==='ball'?`ball ${pr.ball} in sphere ${pr.on.id}`:`${pr.host==='cog'&&pr.on.track?'disc':pr.host} ${pr.on.id}`;

// Where it is when the cog it rides has turned to th.
function probePointAt(pr,th){
  const o=pr.on;
  if(pr.host==='cam'){const L=camRoll(o,camAt(o,camU(o,th)))+14,c=o.cog;return {x:c.x+Math.cos(o.ang)*L,y:c.y+Math.sin(o.ang)*L}}
  const c=probeCog(pr),ang=th+(pr.host==='arm'?o.a:pr.a),L=pr.host==='arm'?clamp(pr.r,o.r0,o.r0+o.len):pr.r;
  return {x:c.x+Math.cos(ang)*L,y:c.y+Math.sin(ang)*L};
}
const probeRaw=(pr,q,c)=>pr.axis==='x'?q.x:pr.axis==='y'?-q.y:Math.hypot(q.x-c.x,q.y-c.y);
// One turn of the cog, as the probe's value at each of PROBE_N angles, scaled to its own travel; cached on the
// geometry that shapes it (where it sits never matters: the travel is measured from its own lowest point).
function probeTab(pr){
  const o=pr.on,c=probeCog(pr);
  const key=[pr.host,pr.axis,pr.r,pr.a,o.a,o.r0,o.len,o.ang,o.ver,o.phase,pr.host==='cam'?camRb(o):0,o.shape].join();
  if(pr._key===key)return pr._tab;
  const raw=[],pts=[];for(let i=0;i<PROBE_N;i++){const q=probePointAt(pr,TAU*i/PROBE_N);pts.push(q);raw.push(probeRaw(pr,q,c))}
  const lo=Math.min(...raw),hi=Math.max(...raw),span=hi-lo;
  pr._tab=raw.map(x=>span>.01?+((x-lo)/span).toFixed(5):.5);pr._key=key;
  // the travel's box, relative to the axle, for the gauge
  pr._box={x0:Math.min(...pts.map(q=>q.x))-c.x,x1:Math.max(...pts.map(q=>q.x))-c.x,y0:Math.min(...pts.map(q=>q.y))-c.y,y1:Math.max(...pts.map(q=>q.y))-c.y};
  return pr._tab;
}
function probeTabAt(pr,th){const t=probeTab(pr),u=th/TAU,x=(u-Math.floor(u))*PROBE_N,i=Math.floor(x)%PROBE_N;return t[i]+(t[(i+1)%PROBE_N]-t[i])*(x-Math.floor(x))}
function ballValue(pr,q){
  const p=pr.on,{L,B}=pitRect(p);
  return clamp(pr.axis==='x'?(q.x-L)/p.w:pr.axis==='y'?(B-q.y)/p.h:Math.hypot(q.x-p.x,q.y-p.y)/pitR(p),0,1);
}
// Its value as drawn (render time), and at sim time t (a strike being scheduled).
function probeNow(pr){if(!probeAlive(pr))return 0;if(pr.host==='ball')return ballValue(pr,ballAt(probeBall(pr)));return probeTabAt(pr,renderTh(probeCog(pr)))}
function probeAt(pr,t){if(!probeAlive(pr))return 0;if(pr.host==='ball')return ballValue(pr,probeBall(pr));const c=probeCog(pr);return probeTabAt(pr,c.th+c.w*(t-simT))}
function probePoint(pr){if(pr.host==='ball')return ballAt(probeBall(pr));return probePointAt(pr,renderTh(probeCog(pr)))}

// Pin it onto what is under p: a cam's follower, a ball or an arm when close, otherwise the cog or disc it lies on.
// Over nothing, it comes loose (carried in from the shelf) or stays where it was (keep: dragged).
function probeAttach(pr,p,keep=false){
  const Z=S.zoom,d=q=>Math.hypot(q.x-p.x,q.y-p.y)*Z;let best=null;
  const take=(dist,reach,set)=>{if(dist<=reach&&(!best||dist<best.dist))best={dist,set}};
  for(const m of W.cams)if(m.cog)take(d(camTip(m)),22,()=>({host:'cam',on:m}));
  for(const q of W.pits)for(const b of q.balls)take(d(ballAt(b))-b.r*Z,14,()=>({host:'ball',on:q,ball:b.id}));
  for(const a of W.arms)if(a.cog){const c=a.cog,ang=renderTh(c)+a.a,ux=Math.cos(ang),uy=Math.sin(ang),r=clamp((p.x-c.x)*ux+(p.y-c.y)*uy,a.r0,a.r0+a.len);
    take(d({x:c.x+ux*r,y:c.y+uy*r}),14,()=>({host:'arm',on:a,r}))}
  let at=best?best.set():null;
  if(!at){const c=pickCog(p);if(c)at={host:'cog',on:c,r:Math.hypot(p.x-c.x,p.y-c.y),a:Math.atan2(p.y-c.y,p.x-c.x)-renderTh(c)}}
  if(!at){if(!keep){pr.on=null;pr.host=null}return false}
  const was=pr.host;Object.assign(pr,{ball:0,r:0,a:0},at);pr._key=null;
  if(pr.host!==was){if(pr.host==='cam')pr.axis='reach';else if(pr.axis==='reach'&&pr.host!=='cog')pr.axis='y'}
  return true;
}

// The parameters it can drive on a voice that is not the synth: its numbers, not its pitch.
const probeDefs=v=>[...(VOICE_DEFS[v.kind]||[]),...COMMON_DEFS].filter(d=>!Array.isArray(d[2])&&!PITCH_KEYS.has(d[0]));
const PROBE_PREFER=['cutoff','tune','bright','decay'];
function probeDef(pr){
  const defs=probeDefs(pr.to.voice);
  return defs.find(d=>d[0]===pr.param)||defs.find(d=>d[0]===PROBE_PREFER.find(k=>defs.some(e=>e[0]===k)))||defs[0]||null;
}
const defAt=(d,y)=>{const[,,a,b]=d;return a>0&&b/a>=20?a*Math.pow(b/a,y):a+(b-a)*y};
// A voice as a strike plays it: every probe on it sets its parameter from where the probe is at that instant.
function probed(v,t){
  if(!W.probes.length||v.kind==='wave')return v;
  let out=v;
  for(const pr of W.probes){if(!pr.to||pr.to.voice!==v||!probeAlive(pr))continue;
    const d=probeDef(pr);if(!d)continue;if(out===v)out={...v};out[d[0]]=defAt(d,pr.lo+(pr.hi-pr.lo)*probeAt(pr,t))}
  return out;
}
// What a probe on a synth sends the worklet: one turn's table, or for a ball its value now (a short glide).
function probeProgram(pr){
  const o=pr.to;if(!o||!voicedType(o)||o.voice.kind!=='wave'||!probeAlive(pr))return null;
  const v=o.voice;
  if(pr.host==='ball'){const y=+probeNow(pr).toFixed(3);return {v,sig:'b'+y,p:{table:[y],step:true,f0:0,ph0:0,rate:0,lo:pr.lo,hi:pr.hi,glide:.03}}}
  const c=probeCog(pr),table=probeTab(pr),sig=pr._key;
  if(clock.paused||!c.w)return {v,sig,p:{table,step:false,f0:0,ph0:renderTh(c)/TAU,rate:0,lo:pr.lo,hi:pr.hi}};
  return {v,sig,p:{table,step:false,f0:(simT+clock.off-clock.shift)*ac.sampleRate,ph0:c.th/TAU,rate:c.w/TAU,lo:pr.lo,hi:pr.hi}};
}
// Every part turning a synth's macro, cam or probe; the synth editor shows which.
const macroUsers=v=>[...camsOn(v),...W.probes.filter(p=>p.to&&p.to.voice===v)];
function macroDriver(v,slot){
  if(!W.cams.length&&!W.probes.length)return null;
  const m=camsOn(v).find(c=>c.macro===slot&&c.cog);if(m)return {label:'cam '+m.id,now:()=>m.lo+(m.hi-m.lo)*camLift(m)};
  const pr=W.probes.find(p=>p.to&&p.to.voice===v&&p.macro===slot&&probeAlive(p));
  return pr?{label:'probe '+pr.id,now:()=>pr.lo+(pr.hi-pr.lo)*probeNow(pr)}:null;
}
// Drop probes whose host has gone; let go of a part that has gone.
function pruneProbes(){W.probes=W.probes.filter(probeAlive);for(const pr of W.probes)if(pr.to&&!voicedType(pr.to))pr.to=null}

// Link it: a synth gives it a free macro and opens the patch sheet with that macro armed (drag a knob to finish);
// anything else takes a parameter at once, which its dial changes.
function linkProbe(pr,o){
  const v=o.voice;pr.to=o;
  if(v.kind==='wave'){
    const used=new Set(macroUsers(v).filter(x=>x!==pr).map(x=>x.macro));
    let slot=v.macro.findIndex((x,i)=>!used.has(i+1)&&!x&&!v.mods.some(r=>r.src==='m'+(i+1)||r.aux==='m'+(i+1)));
    if(slot<0)slot=[...Array(WAVE_MACROS).keys()].find(i=>!used.has(i+1))??0;
    pr.macro=slot+1;buzz(12);changed(true);
    waveUi.arm='m'+pr.macro;waveUi.pin=false;if(['MATRIX','PATCH'].includes(waveUi.tab))waveUi.tab='MAIN';
    select(voicedType(o),o);if(typeof openPartEditor==='function')openPartEditor();
    camSay(()=>`probe ${pr.id} turns M${pr.macro}: drag a knob to set how far`);
  }else{
    pr.param=probeDef(pr)?.[0]||null;buzz(12);changed(true);select('probe',pr);
    camSay(()=>`probe ${pr.id} drives ${voicedName(o)}'s ${probeDef(pr)?.[1]||'nothing'}`);
  }
}
// The voiced part nearest p: a reader or comb, a pad, or a box.
function voicedAt(p){
  const Z=S.zoom;let best=null;const take=(o,d,reach)=>{if(d<=reach&&(!best||d<best.d))best={o,d}};
  for(const r of W.readers)take(r,segDist(p,r.x1,r.y1,r.x2,r.y2)*Z,24);
  for(const o of W.pads)take(o,Math.max(0,Math.hypot(p.x-o.x,p.y-o.y)-o.r)*Z,16);
  for(const o of W.pits)if(Math.hypot(p.x-o.x,p.y-o.y)<=pitR(o))take(o,20,24)
  return best?best.o:null;
}

// Its picture: a pin head where it rides; beside its travel, a rail for the axis it reads with a tick where it
// is now, joined to it by a dashed line; and a thin rod to the part it drives.
function drawProbe(pr){
  if(!probeAlive(pr))return;
  const q=probePoint(pr),y=probeNow(pr),g=8*PX;
  cx.strokeStyle=SEL;cx.fillStyle=SEL;cx.lineWidth=PX;
  if(pr.axis==='reach'){
    const c=pr.host==='ball'?pr.on:probeCog(pr);cx.globalAlpha=.5;cx.setLineDash([2*PX,3*PX]);line(c.x,c.y,q.x,q.y);cx.setLineDash([]);cx.globalAlpha=1;
  }else{
    let x0,x1,y0,y1;
    if(pr.host==='ball'){const{L,R,T,B}=pitRect(pr.on);x0=L;x1=R;y0=T;y1=B}
    else{probeTab(pr);const c=probeCog(pr),b=pr._box;x0=c.x+b.x0;x1=c.x+b.x1;y0=c.y+b.y0;y1=c.y+b.y1}
    cx.globalAlpha=.55;
    if(pr.axis==='y'){const rx=x1+g,ty=y1+(y0-y1)*y;line(rx,y0,rx,y1);line(rx-3*PX,ty,rx+3*PX,ty);cx.setLineDash([2*PX,3*PX]);line(q.x,q.y,rx,ty)}
    else{const ry=y1+g,tx=x0+(x1-x0)*y;line(x0,ry,x1,ry);line(tx,ry-3*PX,tx,ry+3*PX);cx.setLineDash([2*PX,3*PX]);line(q.x,q.y,tx,ry)}
    cx.setLineDash([]);cx.globalAlpha=1;
  }
  circle(q.x,q.y,3.2*PX);line(q.x-5*PX,q.y,q.x+5*PX,q.y);line(q.x,q.y-5*PX,q.x,q.y+5*PX);
  if(pr.to&&voicedType(pr.to)){const b='x1'in pr.to?mid(pr.to):pr.to;
    cx.globalAlpha=.6;cx.setLineDash([2*PX,3*PX]);line(q.x,q.y,b.x,b.y);cx.setLineDash([]);cx.globalAlpha=1;circle(b.x,b.y,2*PX,true)}
}
function drawProbeIcon(){
  cx.strokeStyle=DIM;cx.lineWidth=PX;circle(-8,6,12);
  const q={x:-8+Math.cos(-1)*10,y:6+Math.sin(-1)*10};
  cx.strokeStyle=SEL;circle(q.x,q.y,3);line(q.x-5,q.y,q.x+5,q.y);line(q.x,q.y-5,q.x,q.y+5);
  line(16,-12,16,18);line(13,q.y,19,q.y);cx.setLineDash([2,3]);line(q.x,q.y,16,q.y);cx.setLineDash([]);
}

// The probe's settings: what it rides, the axis it reads, what it drives, and the window of its travel.
function probeTargets(){return [...W.readers.map(o=>['reader',o]),...W.pads.map(o=>['pad',o]),...W.pits.map(o=>['pit',o])]}
const targetKey=o=>o?voicedType(o)+':'+o.id:'none';
function setProbeTarget(pr,k){if(k==='none'){pr.to=null;changed(true);return}const t=probeTargets().find(([,o])=>targetKey(o)===k);if(t)linkProbe(pr,t[1])}
function probeRows(f,pr){
  roRow(f,'rides',()=>probeHostName(pr));
  selRow(f,'axis',PROBE_AXES,()=>pr.axis,x=>{pr.axis=x;pr._key=null;changed()},x=>PROBE_AXIS_LABEL[x]);
  selRow(f,'drives',['none',...probeTargets().map(([,o])=>targetKey(o))],()=>targetKey(pr.to&&voicedType(pr.to)?pr.to:null),x=>{setProbeTarget(pr,x);buildInspector()},
    x=>x==='none'?'nothing':voicedName(probeTargets().find(([,o])=>targetKey(o)===x)[1])+' · '+kindLabel(probeTargets().find(([,o])=>targetKey(o)===x)[1].voice.kind));
  if(pr.to&&pr.to.voice.kind==='wave')selRow(f,'macro',['1','2','3','4','5','6','7','8'],()=>String(pr.macro),x=>{pr.macro=+x;changed(true)},x=>'M'+x);
  else if(pr.to){const defs=probeDefs(pr.to.voice);selRow(f,'param',defs.map(d=>d[0]),()=>probeDef(pr)?.[0],x=>{pr.param=x;changed()},k=>defs.find(d=>d[0]===k)[1])}
  numRow(f,'low',0,1,.01,()=>pr.lo,x=>{pr.lo=x;changed()},0);
  numRow(f,'high',0,1,.01,()=>pr.hi,x=>{pr.hi=x;changed()},1);
  roRow(f,'value',()=>{if(!probeAlive(pr))return 'rides nothing';const y=pr.lo+(pr.hi-pr.lo)*probeNow(pr);
    if(pr.to&&pr.to.voice.kind!=='wave'){const d=probeDef(pr);if(d)return `${settingTextOf(defAt(d,y))} ${d[1]}`}return Math.round(y*100)+'%'});
}
const settingTextOf=x=>String(+(+x).toPrecision(3));
// Serialized: its host by kind and id (a ball by its box and ball id), its target by kind and id.
const probeOut=pr=>({id:pr.id,host:pr.host,on:pr.on.id,ball:pr.ball,r:pr.r,a:pr.a,axis:pr.axis,to:pr.to?targetKey(pr.to):null,macro:pr.macro,param:pr.param,lo:pr.lo,hi:pr.hi});
function probesIn(j,next){
  const lists={arm:next.arms,cog:next.cogs,cam:next.cams,ball:next.pits},targets={reader:next.readers,pad:next.pads,pit:next.pits};
  return (j.probes||[]).map(p=>{
    const on=(lists[p.host]||[]).find(o=>o.id===p.on);if(!on)return null;
    if(p.host==='ball'&&!on.balls.some(b=>b.id===p.ball))return null;
    const [tk,ti]=String(p.to||'').split(':'),to=(targets[tk]||[]).find(o=>o.id===+ti)||null;
    return {...newProbe(p.id),host:p.host,on,ball:p.ball|0,r:+p.r||0,a:+p.a||0,axis:PROBE_AXES.includes(p.axis)?p.axis:'y',to,
      macro:clamp(p.macro|0||1,1,WAVE_MACROS),param:typeof p.param==='string'?p.param:null,lo:clamp(+p.lo||0,0,1),hi:clamp(p.hi==null?1:+p.hi,0,1)};
  }).filter(Boolean);
}
