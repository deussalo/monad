const TAU=Math.PI*2,TOOTH=12,STORE='cogs.v1';
// A CSP-sandboxed page has an opaque origin, where touching localStorage throws.
const $=s=>document.querySelector(s);
function notice(text){const n=$('#notice');n.textContent=text;n.hidden=false}
const storage=(()=>{
  try{localStorage.getItem(STORE)}catch(e){return {ok:false,why:e.name,get:()=>null,put:()=>{},keep:()=>{}}}
  const st={ok:true,why:'',get:()=>localStorage.getItem(STORE),
    put(v){try{localStorage.setItem(STORE,v);st.ok=true}catch(e){if(st.ok)notice(`not saved: ${e.name}. Use json export to keep this machine.`);st.ok=false;st.why=e.name}},
    keep(key,v){localStorage.setItem(key,v)}};
  return st;
})();
const clamp=(x,a,b)=>Math.min(b,Math.max(a,x));
const wrap=a=>((a%TAU)+TAU)%TAU;
const hz=m=>440*Math.pow(2,(m-69)/12);
const radius=c=>c.n*TOOTH/TAU;

const S={bpm:0,beats:4,seed:1,meshTol:4,coaxTol:6,snapTol:16,vRef:500,velCurve:.72,master:.7,lookahead:.2,maxStep:.05,zoom:1,windows:0,ids:0,ratios:0};
const S_DEFS=[['bpm','bpm lock',0,300,1],['beats','beats/bar',1,16,1],['seed','seed',0,9999,1],['meshTol','mesh tol',0,20,.5],['coaxTol','coax tol',0,20,.5],['snapTol','snap tol',0,60,1],
  ['vRef','v ref px/s',20,3000,1],['velCurve','vel curve',.1,3,.01],['master','master',0,1.5,.01],
  ['lookahead','lookahead s',.12,.5,.005],['maxStep','step rad',.005,.3,.005],['zoom','zoom',.1,8,.01],
  ['windows','windows',0,1,1],['ids','ids',0,1,1],['ratios','ratio snap',0,1,1]];
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
// Partials past a body's measured modes continue at harmonic spacing from its last mode.
const bodyRatio=(F,k)=>k<F.length?F[k]:F[F.length-1]+k+1-F.length;
const PARTIALS_MAX=12;
const SCALES={chromatic:[0,1,2,3,4,5,6,7,8,9,10,11],major:[0,2,4,5,7,9,11],minor:[0,2,3,5,7,8,10],
  dorian:[0,2,3,5,7,9,10],pentatonic:[0,2,4,7,9],minpent:[0,3,5,7,10],whole:[0,2,4,6,8,10]};
const VOICES={synth:{note:60,octave:0,semi:0,body:'bell',partials:8,bright:.6,even:1,bend:1,hard:.6,attack:0,decay:1.2,damp:.8},
  kick:{tune:52,sweep:3,pdecay:.045,decay:.35},snare:{tune:190,tone:.35,cutoff:1800,decay:.16},hat:{cutoff:7500,decay:.045},
  tom:{tune:110,sweep:1.6,pdecay:.08,tone:.15,decay:.4},clap:{cutoff:1300,spread:.011,decay:.25},rim:{tune:1700,decay:.06},
  cowbell:{tune:560,decay:.3},wood:{tune:2200,decay:.04},cymbal:{cutoff:5500,tone:.4,decay:1.4},
  // the wave synth's patch is built by waveVoice (wave.js); its own editor shows it, past these pitch rows
  wave:{note:60,octave:0,semi:0}};
const VOICE_DEFS={
  synth:[['note','note',12,108,1],['octave','octave',-3,3,1],['semi','semitones',-12,12,1],['body','body',Object.keys(BODIES)],['partials','partials',1,PARTIALS_MAX,1],
    ['bright','bright',0,1,.01],['even','even partials',0,1,.01],['bend','bend',0,1,.01],['hard','hard',0,1,.01],['attack','attack s',0,1,.005],['decay','decay s',.02,8,.01],['damp','damp',0,2,.01]],
  kick:[['tune','tune Hz',20,200,1],['sweep','sweep x',1,10,.1],['pdecay','p decay s',.005,.3,.001],['decay','decay s',.02,2,.01]],
  snare:[['tune','tune Hz',60,600,1],['tone','tone',0,1,.01],['cutoff','cutoff Hz',200,12000,10],['decay','decay s',.02,1,.005]],
  hat:[['cutoff','cutoff Hz',1000,16000,10],['decay','decay s',.005,1,.005]],
  tom:[['tune','tune Hz',40,400,1],['sweep','sweep x',1,4,.1],['pdecay','p decay s',.005,.3,.001],['tone','tone',0,1,.01],['decay','decay s',.02,2,.01]],
  clap:[['cutoff','cutoff Hz',300,6000,10],['spread','spread s',.003,.04,.001],['decay','decay s',.05,1,.005]],
  rim:[['tune','tune Hz',400,4000,10],['decay','decay s',.01,.5,.005]],
  cowbell:[['tune','tune Hz',200,1200,1],['decay','decay s',.02,2,.01]],
  wood:[['tune','tune Hz',500,5000,10],['decay','decay s',.01,.5,.005]],
  cymbal:[['cutoff','cutoff Hz',1000,12000,10],['tone','tone',0,1,.01],['decay','decay s',.1,4,.01]],
  wave:[['note','note',12,108,1],['octave','octave',-3,3,1],['semi','semitones',-12,12,1]]};
const COMMON_DEFS=[['level','level',0,1.5,.01],['velSense','vel sense',0,1,.01]];
const DRUMS=Object.keys(VOICES).filter(k=>k!=='synth'&&k!=='wave');
const makeVoice=(kind,common={level:.7,velSense:.7})=>kind==='wave'?waveVoice(null,common):({...common,...VOICES[kind],kind});
// The room's musical face; the rest of SPACE and S is engineering, kept behind the advanced drawer.
const ROOM_SPACE=['wet','size','decay','highMult','shimmer','freeze'],ROOM_S=['master','bpm','beats'];
const LOOKAHEAD_MIN=.12;
const S_DEFAULT={...S},SPACE_DEFAULT={...SPACE};

/* ---------- world ---------- */
const W={cogs:[],arms:[],pads:[],readers:[],belts:[],strings:[],next:1};
let simT=0,sel=null,group=[],marquee=null;
const view={x:0,y:0};
const stats={events:0,late:0,skipped:0,steps:0,stepsTick:0,log:[]};
const clock={off:0,shift:0,audio:false,view:0,paused:false};

const stackOf=c=>W.cogs.filter(o=>Math.hypot(o.x-c.x,o.y-c.y)<=S.coaxTol);
const renderTh=c=>c.th-c.w*(simT-clock.view);
// A disc spans whole octaves of its scale: one lane per scale degree, lowest lane innermost.
const RANGES=[1,2,3],OCTAVES=[1,2,3,4,5,6,7];
const trackRings=t=>t.range*SCALES[t.scale].length;
const trackBase=t=>12*(t.octave+1)+t.root;
const ringR=(c,i)=>{const n=trackRings(c.track),r=radius(c);return n>1?r*(.25+.65*i/(n-1)):r*.6};
const scaleNote=(base,scale,i)=>{const sc=SCALES[scale];return base+sc[i%sc.length]+12*Math.floor(i/sc.length)};
const ringNote=(t,i)=>scaleNote(trackBase(t),t.scale,i);
const trimDots=t=>{const n=trackRings(t);t.dots=t.dots.filter(d=>d.ring<n)};
const READER_MODES=['disc notes','comb'];
const readerMode=r=>r.tines?'comb':'disc notes';
function setReaderMode(r,mode){r.tines=mode==='comb'?(r.tines||12):0}
const motorRpm=c=>!c.motor?0:S.bpm&&c.motor.revs?c.motor.revs*S.bpm/S.beats:c.motor.rpm;
function chance(...n){let h=(S.seed|0)^0x9e3779b9;for(const x of n){h=Math.imul(h^Math.round(x*4096),0x85ebca6b);h^=h>>>13;h=Math.imul(h,0xc2b2ae35);h^=h>>>16}return (h>>>0)/4294967296}

// How two cogs touch: on one axle, teeth in mesh, overlapping (a jam), or apart.
function contact(a,b){
  const d=Math.hypot(a.x-b.x,a.y-b.y);
  if(d<=S.coaxTol)return 'axle';
  if(a.layer!==b.layer)return null;
  const gap=d-radius(a)-radius(b);
  return Math.abs(gap)<=S.meshTol?'mesh':gap<0?'jam':null;
}
// A reader seated on a disc lies along one of its radii: `seat` holds that radius's world angle and the reader's
// ends as fractions of the disc's radius, so it follows the disc when it moves and scales when it is resized.
function seatOf(r,c){
  const a=Math.atan2((r.y1+r.y2)/2-c.y,(r.x1+r.x2)/2-c.x),R=radius(c),d1=Math.hypot(r.x1-c.x,r.y1-c.y),d2=Math.hypot(r.x2-c.x,r.y2-c.y);
  return {ang:a,f0:d1/R,f1:d2/R};
}
function seatReader(r,c){
  r.on=c;r.seat=seatOf(r,c);
}
function placeSeated(r){
  const c=r.on,q=r.seat,R=radius(c),ux=Math.cos(q.ang),uy=Math.sin(q.ang);
  const x1=c.x+ux*q.f0*R,y1=c.y+uy*q.f0*R,x2=c.x+ux*q.f1*R,y2=c.y+uy*q.f1*R;
  if(Math.abs(x1-r.x1)+Math.abs(y1-r.y1)+Math.abs(x2-r.x2)+Math.abs(y2-r.y2)>1e-7)Object.assign(r,{x1,y1,x2,y2});
}
function solve(){
  for(const r of W.readers)if(r.on){if(!W.cogs.includes(r.on)||!r.on.track)r.on=null;else placeSeated(r)}
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
    const motors=comp.filter(c=>c.motor),driver=motors[0]||null;
    if(motors.length>1)jam=true;
    const wRoot=driver?motorRpm(driver)*TAU/60/driver.k:0;
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
  fire(v,t,vn,pad,'arm'+arm.id+'>pad'+pad.id,1,(when,amp,pan,vx,dark)=>playVoice(v,v.note,when,amp,pan,key,vx,0,dark));
}

function circleCross(c,R,a0,dt,seg){
  const a1=a0+c.w*dt,x0=c.x+Math.cos(a0)*R,y0=c.y+Math.sin(a0)*R,x1=c.x+Math.cos(a1)*R,y1=c.y+Math.sin(a1)*R;
  const ex=seg.x2-seg.x1,ey=seg.y2-seg.y1,s0=ex*(y0-seg.y1)-ey*(x0-seg.x1),s1=ex*(y1-seg.y1)-ey*(x1-seg.x1);
  if(!(s0<0&&s1>=0||s0>0&&s1<=0))return null;
  const f=s0/(s0-s1),qx=x0+(x1-x0)*f,qy=y0+(y1-y0)*f,L2=ex*ex+ey*ey,u=((qx-seg.x1)*ex+(qy-seg.y1)*ey)/L2;
  if(u<0||u>1)return null;
  const am=a0+c.w*dt*f;
  return {f,u,x:qx,y:qy,am,vn:Math.abs(c.w*R*(Math.sin(am)*ey+Math.cos(am)*ex))/Math.sqrt(L2)};
}
function dotCross(c,dot,rd,dt){
  const hit=circleCross(c,ringR(c,dot.ring),c.th+dot.a,dt,rd);if(!hit)return;
  const t=simT+dt*hit.f,turn=c.turns+Math.floor((c.th+c.w*dt*hit.f)/TAU);
  if(dot.p<1&&chance(c.id,dot.ring,dot.a,turn)>=dot.p)return;
  const midi=rd.tines?scaleNote(rd.combBase,rd.combScale,Math.min(rd.tines-1,Math.floor(hit.u*rd.tines))):ringNote(c.track,dot.ring);
  rd.hits++;rd.lastV=hit.vn;rd.lastMidi=voiceMidi(rd.voice,midi);dot.flash=rd.flash=t;
  const v=rd.voice,key=rd.id*128+midi,hold=dot.len>1?dot.len*TAU/c.track.div/Math.abs(c.w):0;
  fire(v,t,hit.vn,hit,'cog'+c.id+'.ring'+dot.ring+'>reader'+rd.id,dot.v,
    (when,amp,pan,vx,dark)=>playVoice(v,midi,when,amp,pan,key,vx,hold,dark));
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
  fire(s,t,hit.vn,hit,'arm'+arm.id+'>string'+s.id,1,(when,amp,pan,vx,dark)=>pluckSound(s,midi,hit.u,when,amp,pan,vx,dark));
}
function pluckSound(s,midi,u,when,amp,pan,vx,dark=0){
  const f0=hz(midi),ratios=s.kind==='bar'?BODIES.bar:BODIES.harmonic,f=[],g=[],tt=[];
  for(let k=0;k<ratios.length;k++){f.push(f0*ratios[k]);g.push(Math.abs(Math.sin((k+1)*Math.PI*u))/(k+1)*Math.exp(-dark*k));tt.push(s.decay*Math.pow(ratios[k],-s.damp))}
  strike(f,g,tt,when,amp,pan,s.id*128+Math.round(midi),contactTime(s.hard,vx),0);
}
function flex(arm,t,vn){arm.bendT=t;arm.bendA=Math.min(.6,vn/S.vRef*.5)*arm.flex*(arm.cog.w>0?-1:1)}

function turn(c,dt){const th=c.th+c.w*dt,k=Math.floor(th/TAU);c.th=th-k*TAU;c.turns+=k}
function segDist(p,x1,y1,x2,y2){const ex=x2-x1,ey=y2-y1,u=clamp(((p.x-x1)*ex+(p.y-y1)*ey)/(ex*ex+ey*ey||1),0,1);return Math.hypot(p.x-x1-ex*u,p.y-y1-ey*u)}
// A circle of radius R about c can only cross a segment that comes within R of c.
const within=(c,R,seg)=>segDist(c,seg.x1,seg.y1,seg.x2,seg.y2)<=R+1e-9;
function advance(tEnd){
  if(tEnd-simT>.5){const gap=tEnd-S.lookahead-simT;for(const c of W.cogs)turn(c,gap);simT+=gap;stats.skipped+=gap}
  const pairs=[];
  for(const arm of W.arms){const c=arm.cog;if(!c.w)continue;
    for(const pad of W.pads){const h=hitWindow(c,arm,pad);if(h)pairs.push({c,arm,pad,s:h.s,edge:c.w>0?h.beta-h.half:h.beta+h.half})}}
  const plucks=[],reads=[];
  for(const arm of W.arms)if(arm.cog.w)for(const s of W.strings)if(within(arm.cog,arm.r0+arm.len,s))plucks.push([arm,s]);
  for(const c of W.cogs)if(c.track&&c.w)for(const dot of c.track.dots)for(const rd of W.readers)if(within(c,ringR(c,dot.ring),rd))reads.push([c,dot,rd]);
  let maxW=0;for(const c of W.cogs)maxW=Math.max(maxW,Math.abs(c.w));
  stats.stepsTick=0;
  while(simT<tEnd){
    const dt=Math.min(tEnd-simT,maxW?S.maxStep/maxW:Infinity);
    for(const p of pairs){
      const psi=p.c.th+p.arm.a,span=Math.abs(p.c.w)*dt,d0=p.c.w>0?wrap(p.edge-psi):wrap(psi-p.edge);
      if(d0>0&&d0<=span)armHit(p,simT+dt*d0/span);
    }
    for(const[c,dot,rd]of reads)dotCross(c,dot,rd,dt);
    for(const[arm,s]of plucks)pluck(arm,s,dt);
    for(const c of W.cogs)turn(c,dt);
    simT+=dt;stats.steps++;stats.stepsTick++;
  }
}

/* ---------- audio ---------- */
let ac=null,engineNode=null,engineState='off',engineStats={},master=null,distant=null,meter=null,drumBus=null,noise=null,peak=0;
const strikes=[];
function initAudio(){
  if(ac){if(ac.state!=='running')ac.resume();return}
  // A touch device gets the larger 'playback' buffer: the smallest one underran on phones and the beat hiccuped.
  // Sound is scheduled S.lookahead ahead and drawing reads the output latency, so nothing shown lags.
  ac=new (window.AudioContext||window.webkitAudioContext)({latencyHint:matchMedia('(pointer:coarse)').matches?'playback':'interactive'});
  master=ac.createGain();master.gain.value=S.master;
  distant=ac.createBiquadFilter();distant.type='lowpass';distant.frequency.value=20000;distant.Q.value=.5;
  const lim=ac.createDynamicsCompressor();
  lim.threshold.value=-10;lim.knee.value=8;lim.ratio.value=3.5;lim.attack.value=.015;lim.release.value=.3;
  meter=ac.createAnalyser();meter.fftSize=2048;
  master.connect(distant);distant.connect(lim);lim.connect(ac.destination);lim.connect(meter);
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
    drumBus=ac.createGain();drumBus.connect(engineNode);engineNode.connect(master);startWave();
    engineState='ready';
  }).catch(err=>{engineState='failed: '+String(err.message).slice(0,160)});
  ac.resume();
}
function flushStrikes(){const d=new Float64Array(strikes);strikes.length=0;engineNode.port.postMessage({type:'strikes',d},[d.buffer])}
const contactTime=(hard,x)=>clamp((.0016-.0014*hard)*Math.pow(Math.max(.02,x),-.35),.00012,.004);
function strike(f,g,t,when,amp,pan,key,contact,attack){
  if(!strikes.length)queueMicrotask(flushStrikes);
  strikes.push(when*ac.sampleRate,key,amp*.3,contact,attack,pan,f.length,...f,...g,...t);
}
// A held note rings for its length, then a damper takes it off over 60 ms.
function damp(key,when){engineNode.port.postMessage({type:'damp',at:when*ac.sampleRate,key});waveNode?.port.postMessage({type:'off',at:when*ac.sampleRate,key})}
// Every kind's way to sound: the modal synth, the wave synth, or a drum graph.
function playVoice(v,midi,when,amp,pan,key,x,hold=0,dark=0){
  if(v.kind==='synth')synth(v,midi,when,amp,pan,key,x,hold,dark);else if(v.kind==='wave')waveNote(v,midi,when,amp,pan,key,x,hold,dark);else drum(v,when,amp,pan,dark);
}
const voiceMidi=(v,midi)=>clamp(midi+(v.kind==='synth'||v.kind==='wave'?12*v.octave+v.semi:0),0,127);
// Additive: `partials` ringing modes at the body's ratios, bent toward harmonic, each decaying by its ratio^-damp.
function synth(v,midi,when,amp,pan,key,x,hold=0,dark=0){
  const F=BODIES[v.body],f0=hz(voiceMidi(v,midi)),f=[],g=[],t=[];let e=0;
  for(let k=0;k<v.partials;k++){const r=k+1+(bodyRatio(F,k)-k-1)*v.bend,gk=Math.pow(v.bright,k)*(k%2?v.even:1)*Math.exp(-dark*k);f.push(f0*r);g.push(gk);t.push(Math.max(hold*1.5,v.decay*Math.pow(r,-v.damp)));e+=gk*gk}
  strike(f,g,t,when,amp*Math.min(1,1.3/Math.sqrt(e)),pan,key,contactTime(v.hard,x),v.attack);
  if(hold)damp(key,when+hold);
}
// Drums are WebAudio graphs on the drum bus: tuned oscillators and filtered noise under one decaying gain.
function drum(v,when,amp,pan,dark=0){
  const out=ac.createGain(),p=ac.createStereoPanner(),end=when+v.decay,src=[];
  p.pan.value=pan;p.connect(drumBus);
  if(dark>.02){const lp=ac.createBiquadFilter();lp.type='lowpass';lp.frequency.value=16000*Math.exp(-dark*3.2);out.connect(lp);lp.connect(p)}else out.connect(p);
  out.gain.setValueAtTime(amp,when);out.gain.exponentialRampToValueAtTime(1e-4,end);
  const osc=(type,f0,gain,f1=f0,sweepT=0,at=when,to=end)=>{const o=ac.createOscillator(),og=ac.createGain();o.type=type;o.frequency.setValueAtTime(f0,at);
    if(f1!==f0)o.frequency.exponentialRampToValueAtTime(f1,at+sweepT);og.gain.value=gain;o.connect(og);og.connect(out);src.push([o,at,to])};
  const hiss=(type,cutoff,gain,at=when,to=end)=>{const n=ac.createBufferSource(),f=ac.createBiquadFilter(),ng=ac.createGain();n.buffer=noise;n.loop=true;n.loopStart=Math.random();
    f.type=type;f.frequency.value=cutoff;f.Q.value=.8;ng.gain.setValueAtTime(gain,at);if(to<end)ng.gain.exponentialRampToValueAtTime(1e-4,to);n.connect(f);f.connect(ng);ng.connect(out);src.push([n,at,to])};
  const k=v.kind;
  if(k==='kick')osc('sine',v.tune*v.sweep,1,v.tune,v.pdecay);
  else if(k==='snare'){hiss('bandpass',v.cutoff,1-v.tone);osc('triangle',v.tune,v.tone)}
  else if(k==='hat')hiss('highpass',v.cutoff,1);
  else if(k==='tom'){osc('sine',v.tune*v.sweep,1-v.tone,v.tune,v.pdecay);hiss('bandpass',v.tune*8,v.tone,when,when+.03)}
  else if(k==='clap'){for(let i=0;i<3;i++)hiss('bandpass',v.cutoff,.8,when+i*v.spread,when+i*v.spread+v.spread*.9);hiss('bandpass',v.cutoff,.6,when+3*v.spread)}
  else if(k==='rim'){osc('triangle',v.tune,.6);osc('square',v.tune*1.78,.3);hiss('highpass',v.tune*2,.5,when,when+.008)}
  else if(k==='cowbell'){osc('square',v.tune,.5);osc('square',v.tune*1.48,.5)}
  else if(k==='wood')osc('sine',v.tune,1,v.tune*.9,v.decay);
  else if(k==='cymbal'){hiss('highpass',v.cutoff,1-v.tone);for(const r of [1,1.34,1.71,2.26])osc('square',v.cutoff*.11*r,v.tone*.18)}
  for(const[s,at,to]of src){s.start(at);s.stop(to+.02)}
}
function fire(v,t,vn,at,src,gain,play){
  stats.events++;
  stats.log.push({t,src,vn});if(stats.log.length>256)stats.log.shift();
  if(!clock.audio||engineState!=='ready')return;
  const vx=clamp(vn/S.vRef,0,1),amp=gain*v.level*(1-v.velSense+v.velSense*Math.pow(vx,S.velCurve));
  if(amp<=0)return;
  let when=t+clock.off-clock.shift;
  if(when<ac.currentTime){stats.late++;when=ac.currentTime}
  const p=heard(at);
  play(when,amp*p.gain,p.pan,vx,p.dark);
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
// Each running train turns until its motor shows angle 0; its cogs move by their ratios, so meshes hold.
function toStart(){
  const by=new Map(W.cogs.filter(m=>m.motor&&m.w).map(m=>[m,-wrap(renderTh(m))/m.w]));
  for(const c of W.cogs){const dt=by.get(c.driver);if(dt!==undefined)c.th=wrap(c.th+c.w*dt);c.turns=0}
  changed();
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
const UNDONE_ELSEWHERE=new Set(['view','zoom','th']);
const worldKey=()=>JSON.stringify(serialize(),(k,v)=>UNDONE_ELSEWHERE.has(k)?undefined:v);
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
  const kept=sel&&{type:sel.type,id:sel.o.id};
  const j=JSON.parse(s.key);Object.assign(j.S,{zoom:S.zoom});j.view={...view};
  for(const c of j.cogs){const p=s.phase.get(c.id);c.th=wrap(p.th+p.w*(simT-s.t))}
  reload(j);
  const back=kept&&W[kept.type+'s']?.find(o=>o.id===kept.id);if(back)select(kept.type,back);
  clearTimeout(edits.timer);edits.pending=false;
}
addEventListener('pointerdown',()=>{edits.held=true},true);
for(const type of ['pointerup','pointercancel'])addEventListener(type,()=>{edits.held=false;if(edits.pending)edits.commit()});
function serialize(){
  return {S,SPACE,view,
    cogs:W.cogs.map(c=>({id:c.id,x:c.x,y:c.y,n:c.n,motor:c.motor&&{...c.motor},layer:c.layer,th:wrap(c.th),
      track:c.track&&{...c.track,dots:c.track.dots.map(d=>({ring:d.ring,a:d.a,p:d.p,v:d.v,len:d.len}))}})),
    arms:W.arms.map(a=>({id:a.id,cog:a.cog.id,a:a.a,r0:a.r0,len:a.len,flex:a.flex})),
    pads:W.pads.map(p=>({id:p.id,x:p.x,y:p.y,r:p.r,voice:p.voice})),
    readers:W.readers.map(r=>({id:r.id,on:r.on?r.on.id:null,x1:r.x1,y1:r.y1,x2:r.x2,y2:r.y2,voice:r.voice,tines:r.tines,combBase:r.combBase,combScale:r.combScale})),
    belts:W.belts.map(b=>({id:b.id,a:b.a.id,b:b.b.id,cross:b.cross})),
    strings:W.strings.map(({id,x1,y1,x2,y2,kind,f200,quant,decay,damp,hard,level,velSense})=>({id,x1,y1,x2,y2,kind,f200,quant,decay,damp,hard,level,velSense}))};
}
// Every part is built and checked before any of the live world is replaced.
function deserialize(j){
  const cogs=j.cogs.map(c=>({...c,motor:c.motor?{rpm:+c.motor.rpm||0,revs:+c.motor.revs||0}:null,w:0,turns:0}));
  const byId=new Map(cogs.map(c=>[c.id,c]));
  const cog=(id,who)=>{const c=byId.get(id);if(!c)throw new Error(`${who}: no cog ${id}`);return c};
  const importedVoice=o=>{
    const v=o.voice;if(!VOICES[v.kind])throw new Error(`${o.id}: voice kind ${v.kind}`);
    if(v.kind==='wave'){const keep={};for(const k of WAVE_VOICE_KEYS)if(v[k]!==undefined)keep[k]=v[k];return waveVoice(v,keep)}
    return {...makeVoice(v.kind),...v};
  };
  for(const c of cogs)if(c.track){const t=c.track;
    if(!SCALES[t.scale]||!RANGES.includes(t.range)||!OCTAVES.includes(t.octave)||!(t.root>=0&&t.root<12))throw new Error(`cog ${c.id}: track scale ${t.scale}, range ${t.range}, octave ${t.octave}, root ${t.root}`);
    c.track={...t,dots:t.dots.map(d=>({p:1,v:1,len:1,...d}))}}
  const next={cogs,
    arms:j.arms.map(a=>({flex:.5,...a,cog:cog(a.cog,'arm '+a.id)})),
    belts:j.belts.map(b=>({...b,a:cog(b.a,'belt '+b.id),b:cog(b.b,'belt '+b.id)})),
    strings:j.strings.map(s=>({...s,hits:0,lastV:0})),
    pads:j.pads.map(p=>({...p,voice:importedVoice(p),hits:0,lastV:0})),
    readers:j.readers.map(r=>({tines:0,combBase:60,combScale:'major',...r,on:null,voice:importedVoice(r),hits:0,lastV:0}))};
  // A save from before seats: a reader lying wholly on a disc is seated on it.
  for(const[i,r]of next.readers.entries()){
    const on=j.readers[i].on,c=on!==undefined?(on===null?null:cog(on,'reader '+r.id)):
      cogs.filter(c=>c.track&&Math.hypot(r.x1-c.x,r.y1-c.y)<=radius(c)&&Math.hypot(r.x2-c.x,r.y2-c.y)<=radius(c)).sort((a,b)=>b.layer-a.layer)[0];
    if(c&&c.track)seatReader(r,c);
  }
  Object.assign(W,next);
  W.next=1+Math.max(0,...Object.values(next).flat().map(o=>o.id));
  Object.assign(S,j.S);Object.assign(SPACE,j.SPACE);Object.assign(view,j.view);
  // Older saves schedule 60 ms ahead, too close for a phone's main thread: notes landed late and the groove dragged.
  S.lookahead=Math.max(S.lookahead,LOOKAHEAD_MIN);
  select(null);solve();applyAudioSettings();syncRows();changed();
}
// Rebuild the world from j with every surviving cog keeping its revolution count.
function reload(j){const turns=new Map(W.cogs.map(c=>[c.id,c.turns]));deserialize(j);for(const c of W.cogs)c.turns=turns.get(c.id)??0}
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
