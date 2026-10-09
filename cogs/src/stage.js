/* ---------- look ---------- */
const css=k=>getComputedStyle(document.documentElement).getPropertyValue(k).trim();
let BG,FG,DIM,FAINT,SEL,BAD,HOT,READ,PAD;
function readPalette(){[BG,FG,DIM,FAINT,SEL,BAD,HOT,READ,PAD]=['bg','fg','dim','faint','sel','bad','hot','read','pad'].map(k=>css('--'+k))}
readPalette();
const glow=t=>t==null||clock.view<t?0:Math.exp(-(clock.view-t)/.12);
function hot(t,base,g=cx){const e=glow(t);if(e<=.03)return base;g.globalAlpha=.35+.65*e;return HOT}

/* ---------- render ---------- */
const cv=$('#c');let cx=cv.getContext('2d');
let dpr=1,CW=0,CH=0,PX=1;
const ZMIN=.1,ZMAX=8,DUR=380;
function resize(){
  const r=cv.getBoundingClientRect();
  dpr=devicePixelRatio||1;CW=r.width;CH=r.height;cv.width=Math.round(CW*dpr);cv.height=Math.round(CH*dpr);
  onResize();
}
const toScreen=p=>({x:(p.x-view.x)*S.zoom+CW/2,y:(p.y-view.y)*S.zoom+CH/2});
const screenToWorld=s=>({x:(s.x-CW/2)/S.zoom+view.x,y:(s.y-CH/2)/S.zoom+view.y});
const isSel=o=>sel&&sel.o===o||group.some(g=>g.o===o);
const ringGap=c=>c.track.rings>1?radius(c)*.65/(c.track.rings-1):radius(c)*.3;
const noteMode=(c,z=S.zoom)=>!!c.track&&ringGap(c)*z>=12;
const mid=o=>({x:(o.x1+o.x2)/2,y:(o.y1+o.y2)/2});
function line(x1,y1,x2,y2){cx.beginPath();cx.moveTo(x1,y1);cx.lineTo(x2,y2);cx.stroke()}
function circle(x,y,r,fill){cx.beginPath();cx.arc(x,y,Math.max(0,r),0,TAU);fill?cx.fill():cx.stroke()}
function beltLines(b){
  const A=b.a,B=b.b,ra=radius(A)+3,rb=radius(B)+3,d=Math.hypot(B.x-A.x,B.y-A.y),base=Math.atan2(B.y-A.y,B.x-A.x);
  const cosA=(b.cross?ra+rb:ra-rb)/d;if(!(Math.abs(cosA)<1))return [];
  const al=Math.acos(cosA);
  return [1,-1].map(s=>{const a=base+s*al,aB=b.cross?a+Math.PI:a;return [A.x+ra*Math.cos(a),A.y+ra*Math.sin(a),B.x+rb*Math.cos(aB),B.y+rb*Math.sin(aB)]});
}

function strokes(x1,y1,x2,y2,layers){for(const[w,col]of layers){cx.lineWidth=w;cx.strokeStyle=col;line(x1,y1,x2,y2)}}
function drawCog(c){
  const th=renderTh(c),r=radius(c),driven=!!motorRpm(c);let x=c.x,y=c.y;
  if(c.jam){const k=clock.view;x+=Math.sin(k*71)*1.3*PX;y+=Math.cos(k*53)*1.3*PX}
  cx.lineWidth=PX;cx.strokeStyle=c.jam?BAD:DIM;cx.setLineDash(c.layer?[4*PX,3*PX]:[]);circle(x,y,r);cx.setLineDash([]);
  cx.beginPath();
  for(let k=0;k<c.n;k++){const a=th+k*TAU/c.n,ca=Math.cos(a),sa=Math.sin(a);cx.moveTo(x+ca*(r-3),y+sa*(r-3));cx.lineTo(x+ca*(r+3),y+sa*(r+3))}
  cx.stroke();
  if(c.track)drawDiscFace(c,th,x,y);
  cx.strokeStyle=FG;line(x,y,x+Math.cos(th)*(r-5),y+Math.sin(th)*(r-5));
  cx.fillStyle=cx.strokeStyle=c.jam?BAD:FG;circle(x,y,3*PX,driven);
}
function drawDiscFace(c,th,x,y){
  const t=c.track,note=noteMode(c,1/PX),gap=ringGap(c),rIn=ringR(c,0),rOut=ringR(c,t.rings-1);
  if(note)for(let i=0;i<t.rings;i++)if(ringNote(t,i)%12===t.base%12){
    const R=ringR(c,i);cx.fillStyle=PAD;cx.beginPath();cx.arc(x,y,R+gap/2,0,TAU);cx.arc(x,y,Math.max(0,R-gap/2),0,TAU,true);cx.fill()}
  cx.strokeStyle=FAINT;cx.lineWidth=PX;
  for(let i=0;i<t.rings;i++)circle(x,y,ringR(c,i));
  for(let k=0;k<t.div;k++){const a=th+k*TAU/t.div;cx.strokeStyle=note&&k%4===0?DIM:FAINT;line(x+Math.cos(a)*rIn,y+Math.sin(a)*rIn,x+Math.cos(a)*rOut,y+Math.sin(a)*rOut)}
  const base=note?clamp(gap*.32,1.5,4.5):1.5*PX;
  for(const d of t.dots){
    const R=ringR(c,d.ring),a=th+d.a,px=x+Math.cos(a)*R,py=y+Math.sin(a)*R,pr=base*(note?.6+.4*d.v:1+d.v)+2*glow(d.flash)*PX;
    cx.fillStyle=hot(d.flash,FG);cx.globalAlpha*=.3+.7*d.p;circle(px,py,pr,true);cx.globalAlpha=1;
    if(isSel(d)){cx.strokeStyle=SEL;circle(px,py,pr+3*PX)}
  }
}
const armBend=a=>{const age=clock.view-(a.bendT??-9);return age>=0&&age<2?a.bendA*Math.exp(-age/.25)*Math.cos(age*TAU*9):0};
function drawArm(a){
  const c=a.cog,ang=renderTh(c)+a.a,b=armBend(a),bx=c.x+Math.cos(ang)*a.r0,by=c.y+Math.sin(ang)*a.r0;
  const mx=bx+Math.cos(ang)*a.len/2,my=by+Math.sin(ang)*a.len/2,tx=bx+Math.cos(ang+b)*a.len,ty=by+Math.sin(ang+b)*a.len;
  cx.lineWidth=3*PX;cx.strokeStyle=hot(a.flash,FG);cx.beginPath();cx.moveTo(bx,by);cx.quadraticCurveTo(mx,my,tx,ty);cx.stroke();cx.globalAlpha=1;cx.lineWidth=PX;
}
const armTip=a=>{const c=a.cog,ang=renderTh(c)+a.a,L=a.r0+a.len;return {x:c.x+Math.cos(ang)*L,y:c.y+Math.sin(ang)*L}};
function drawPad(p){
  if(p.flash!=null&&clock.view>=p.flash){const dt=clock.view-p.flash;if(dt<.6){const k=dt/.6,amp=clamp(p.lastV/S.vRef,.25,1);
    cx.globalAlpha=(1-k)*.6*amp;cx.strokeStyle=HOT;cx.lineWidth=PX;circle(p.x,p.y,p.r+3*PX+k*(14+26*amp)*PX);cx.globalAlpha=1}}
  cx.fillStyle=hot(p.flash,PAD);circle(p.x,p.y,p.r,true);cx.globalAlpha=1;
  cx.lineWidth=PX;cx.strokeStyle=FG;circle(p.x,p.y,p.r);
  if(p.r/PX>10){cx.strokeStyle=DIM;emblem(p.voice.kind,p.x,p.y,p.r)}
}
function emblem(kind,x,y,r){
  if(kind==='kick')circle(x,y,r*.45);
  else if(kind==='snare'){line(x-r*.5,y-r*.15,x+r*.5,y-r*.15);line(x-r*.5,y+r*.15,x+r*.5,y+r*.15)}
  else if(kind==='hat'){line(x-r*.55,y,x+r*.55,y);circle(x,y,r*.12)}
  else{cx.beginPath();cx.moveTo(x-r*.35,y+r*.3);cx.quadraticCurveTo(x-r*.35,y-r*.5,x,y-r*.5);cx.quadraticCurveTo(x+r*.35,y-r*.5,x+r*.35,y+r*.3);cx.closePath();cx.stroke()}
}
function drawReader(rd){
  const ex=rd.x2-rd.x1,ey=rd.y2-rd.y1,L=Math.hypot(ex,ey)||1;
  cx.lineWidth=2*PX;cx.strokeStyle=hot(rd.flash,READ);line(rd.x1,rd.y1,rd.x2,rd.y2);cx.globalAlpha=1;
  cx.lineWidth=PX;cx.strokeStyle=READ;
  for(let i=1;i<rd.tines;i++){const u=i/rd.tines;line(rd.x1+ex*u-ey/L*4*PX,rd.y1+ey*u+ex/L*4*PX,rd.x1+ex*u+ey/L*4*PX,rd.y1+ey*u-ex/L*4*PX)}
  cx.fillStyle=READ;circle(rd.x1,rd.y1,2.5*PX,true);circle(rd.x2,rd.y2,2.5*PX,true);
}
function drawString(s){
  const ex=s.x2-s.x1,ey=s.y2-s.y1,L=Math.hypot(ex,ey)||1,age=clock.view-(s.flash??-9);
  const amp=age>=0&&age<2?Math.min(8,(s.lastV||0)/60)*Math.exp(-age/.35)*Math.sin(age*TAU*11):0;
  const u=s.lastU??.5,mx=s.x1+ex*u-ey/L*amp,my=s.y1+ey*u+ex/L*amp;
  cx.strokeStyle=hot(s.flash,FG);cx.lineWidth=(s.kind==='bar'?4:1.5)*PX;
  cx.beginPath();cx.moveTo(s.x1,s.y1);cx.lineTo(mx,my);cx.lineTo(s.x2,s.y2);cx.stroke();cx.globalAlpha=1;
  cx.fillStyle=DIM;circle(s.x1,s.y1,2.5*PX,true);circle(s.x2,s.y2,2.5*PX,true);cx.lineWidth=PX;
}
function drawBelt(b){
  cx.strokeStyle=DIM;cx.lineWidth=2*PX;cx.setLineDash([6*PX,5*PX]);cx.lineDashOffset=-renderTh(b.a)*(radius(b.a)+3);
  for(const[x1,y1,x2,y2]of beltLines(b))line(x1,y1,x2,y2);
  cx.setLineDash([]);cx.lineWidth=PX;
}
function halo(x,y,r){cx.lineWidth=1.5*PX;cx.strokeStyle=SEL;circle(x,y,r+3*PX);cx.lineWidth=PX}
function drawWindows(){
  const show=(arm,pad)=>S.windows||isSel(arm)||isSel(pad);
  cx.strokeStyle=FAINT;cx.lineWidth=PX;
  for(const arm of W.arms)for(const pad of W.pads){if(!show(arm,pad))continue;const c=arm.cog,h=hitWindow(c,arm,pad);if(!h)continue;
    cx.beginPath();cx.arc(c.x,c.y,h.s,h.beta-h.half,h.beta+h.half);cx.stroke();
    if(c.w){const e=c.w>0?h.beta-h.half:h.beta+h.half;line(c.x,c.y,c.x+Math.cos(e)*h.s,c.y+Math.sin(e)*h.s)}}
}
function drawGround(){
  cx.setTransform(dpr,0,0,dpr,0,0);cx.fillStyle=BG;cx.fillRect(0,0,CW,CH);
  if(!S.grid)return;
  const step=50*S.zoom,ox=((CW/2-view.x*S.zoom)%step+step)%step,oy=((CH/2-view.y*S.zoom)%step+step)%step;
  cx.strokeStyle=FAINT;cx.lineWidth=1;
  for(let x=ox;x<CW;x+=step)line(x,0,x,CH);for(let y=oy;y<CH;y+=step)line(0,y,CW,y);
}
function draw(){
  cx.globalAlpha=1;drawGround();
  const z=S.zoom*dpr;PX=1/S.zoom;
  cx.setTransform(z,0,0,z,dpr*CW/2-view.x*z,dpr*CH/2-view.y*z);
  for(const c of [...W.cogs].sort((a,b)=>a.layer-b.layer))drawCog(c);
  for(const b of W.belts)drawBelt(b);
  drawWindows();
  for(const s of W.strings)drawString(s);
  for(const r of W.readers)drawReader(r);
  for(const p of W.pads)drawPad(p);
  for(const a of W.arms)drawArm(a);
  if(ui.carry&&ui.carry.band){const{a,p}=ui.carry.band;cx.setLineDash([6*PX,5*PX]);strokes(a.x,a.y,p.x,p.y,[[2*PX,DIM]]);cx.setLineDash([])}
  if(sel){const o=sel.o;
    if(sel.type==='cog')halo(o.x,o.y,radius(o)+3.2);else if(sel.type==='pad')halo(o.x,o.y,o.r+2.4);
    else if(sel.type==='arm'){const t=armTip(o);halo(t.x,t.y,3.3)}else if(sel.type==='reader'||sel.type==='string'){halo(o.x1,o.y1,3.8);halo(o.x2,o.y2,3.8)}}
  if(S.ids){cx.fillStyle=DIM;cx.font=`${10*PX}px monospace`;
    for(const c of W.cogs)cx.fillText('c'+c.id,c.x+4*PX,c.y-4*PX);
    for(const p of W.pads)cx.fillText('p'+p.id,p.x+p.r+2*PX,p.y);
    for(const r of [...W.readers,...W.strings])cx.fillText((r.f200?'s':'r')+r.id,r.x2+2*PX,r.y2);
    for(const a of W.arms){const t=armTip(a);cx.fillText('a'+a.id,t.x,t.y)}}
  cx.setTransform(dpr,0,0,dpr,0,0);
  if(ui.crank)drawCrankGauge(ui.crank);
  drawChrome();
}
function drawGhost(s){
  cx.globalAlpha=.55;cx.save();cx.setTransform(dpr*S.zoom,0,0,dpr*S.zoom,dpr*s.x,dpr*s.y);PX=1/S.zoom;
  drawArm({cog:{x:-24,y:0,th:0,w:0},a:0,r0:0,len:24});cx.restore();cx.globalAlpha=1;
}
const PART_KEYS={c:'cog',d:'disc',a:'arm',b:'belt',p:'pad',t:'string',r:'reader'},KEY_OF=Object.fromEntries(Object.entries(PART_KEYS).map(([k,v])=>[v,k]));
const PARTS=Object.values(PART_KEYS);
const MODEL={
  cog:{n:12,x:0,y:0,th:.13,w:0,layer:0,motor:0,track:null},
  disc:{n:48,x:0,y:0,th:.3,w:0,layer:0,motor:0,track:{rings:5,base:57,scale:'minpent',div:8,dots:[[0,0],[2,1.57],[4,3.93],[3,5.5],[1,2.36]].map(([ring,a])=>({ring,a,p:1,v:1}))}},
};
// A part as its picture, about 50 px across at scale 1, centred on the origin of the current transform.
function drawPartIcon(kind){
  if(kind==='cog'||kind==='disc')drawCog(MODEL[kind]);
  else if(kind==='arm')drawArm({cog:{x:-14,y:14,th:0,w:0},a:-Math.PI/4,r0:0,len:40});
  else if(kind==='pad')drawPad({x:0,y:0,r:11,voice:{kind:'kick'}});
  else if(kind==='belt'){cx.strokeStyle=DIM;cx.lineWidth=PX;circle(-12,0,9);circle(14,0,6);cx.setLineDash([3,3]);strokes(-12,-9,14,-6,[[1.5,DIM]]);strokes(-12,9,14,6,[[1.5,DIM]]);cx.setLineDash([])}
  else if(kind==='string')drawString({x1:-20,y1:14,x2:20,y2:-14,kind:'string'});
  else drawReader({x1:-17,y1:17,x2:17,y2:-17,tines:0});
}
const ICON_SCALE={disc:.235,cog:.82,pad:1.3};
function bounds(){
  const xs=[],ys=[],add=(x,y,r)=>{xs.push(x-r,x+r);ys.push(y-r,y+r)};
  for(const c of W.cogs)add(c.x,c.y,radius(c)+4);for(const p of W.pads)add(p.x,p.y,p.r+3);
  for(const r of [...W.readers,...W.strings]){add(r.x1,r.y1,4);add(r.x2,r.y2,4)}for(const a of W.arms){const t=armTip(a);add(t.x,t.y,4)}
  if(!xs.length)return null;
  const x0=Math.min(...xs),x1=Math.max(...xs),y0=Math.min(...ys),y1=Math.max(...ys);return {cx:(x0+x1)/2,cy:(y0+y1)/2,w:x1-x0,h:y1-y0};
}
// The view centre that puts world point p at the middle of the open stage.
const centred=(p,z)=>{const s=stage();return {x:p.x-((s.l+s.r)/2-CW/2)/z,y:p.y-((s.t+s.b)/2-CH/2)/z}};
function fitTarget(){
  const b=bounds(),s=stage();if(!b)return {x:0,y:0,z:1,fit:1};
  const fit=Math.min((s.r-s.l-40)/b.w,(s.b-s.t-70)/b.h),z=clamp(fit,ZMIN,2);
  return {...centred({x:b.cx,y:b.cy},z),z,fit};
}

const RPM_DETENTS=[5,7.5,10,12,15,20,24,30,40,45,60,72,80,90,120,144,160,180,240,300];
const BAR_DETENTS=[1/8,1/6,1/4,1/3,1/2,2/3,3/4,1,4/3,3/2,2,3,4,6,8];
const detents=()=>S.bpm?BAR_DETENTS.map(r=>r*S.bpm/S.beats):RPM_DETENTS;
const gaugeAngle=rpm=>Math.sign(rpm)*clamp(Math.log(Math.max(1,Math.abs(rpm))/2.5)/Math.log(120),0,1)*1.7*Math.PI;
function drawCrankGauge(k){
  const c=k.c,s=toScreen(c),R=radius(c)*S.zoom+22,a0=-Math.PI/2;
  cx.lineWidth=1;cx.strokeStyle=DIM;
  for(const d of detents())for(const sg of [-1,1]){const a=a0+gaugeAngle(sg*d);line(s.x+Math.cos(a)*(R-4),s.y+Math.sin(a)*(R-4),s.x+Math.cos(a)*(R+4),s.y+Math.sin(a)*(R+4))}
  const a1=a0+gaugeAngle(motorRpm(c));
  cx.strokeStyle=HOT;cx.lineWidth=2;cx.lineCap='round';cx.beginPath();cx.arc(s.x,s.y,R,Math.min(a0,a1),Math.max(a0,a1));cx.stroke();cx.lineCap='butt';
  cx.fillStyle=HOT;circle(s.x+Math.cos(a1)*R,s.y+Math.sin(a1)*R,3.5,true);
}

/* ---------- touch ---------- */
const ui={armed:null,carry:null,scrap:false,crank:null,cam:null,sector:null,bezelOwner:null,bezelAt:0,lastTap:null,auditionAt:0,snapped:false,t:0,dotOf:null,dotAt:null,sealAt:0,sealOk:false,shared:''};
const wrapPi=a=>wrap(a+Math.PI)-Math.PI;
function buzz(ms){try{const u=navigator.userActivation;if(navigator.vibrate&&(!u||u.hasBeenActive))navigator.vibrate(ms)}catch(e){}}
// Play a part now, at a middling velocity, the way the sim would strike it.
function sound(o,midi,key){
  if(!ac||engineState!=='ready')return;
  const when=ac.currentTime+.005,v=o.voice||o,amp=.85*v.level,pan=clamp(((o.x??o.x1)-view.x)/300,-.9,.9);
  if(o.f200)pluckSound(o,midi,o.lastU??.3,when,amp,pan,.7);
  else if(v.kind==='modal')modal(v,midi,when,amp,pan,key,.7);else drum(v,when,amp,pan);
}
const discReader=c=>W.readers.find(r=>segDist(c,r.x1,r.y1,r.x2,r.y2)<radius(c))||{id:0,x:c.x,voice:makeVoice('modal')};
function audition(o,force){
  const t=performance.now();if(!force&&t-ui.auditionAt<140)return;ui.auditionAt=t;
  if(o.f200)sound(o,stringMidi(o));
  else if(o.voice)sound(o,o.tines?o.combBase:o.voice.note??60,o.id*128);
  else if(o.track)sound(discReader(o),o.track.base,o.id*128);
  else if(sel&&sel.type==='dot')sound(discReader(ui.dotOf),ringNote(ui.dotOf.track,o.ring),ui.dotOf.id*128);
}
function fly(x,y,z){ui.cam={x0:view.x,y0:view.y,z0:S.zoom,x1:x,y1:y,z1:clamp(z,ZMIN,ZMAX),t0:performance.now()}}
function stepCamera(){
  const m=ui.cam;if(!m)return;
  const k=clamp((performance.now()-m.t0)/DUR,0,1),e=1-Math.pow(1-k,3);
  S.zoom=m.z0*Math.pow(m.z1/m.z0,e);view.x=m.x0+(m.x1-m.x0)*e;view.y=m.y0+(m.y1-m.y0)*e;
  if(k>=1){ui.cam=null;changed()}
}
function flyFit(){const f=fitTarget();fly(f.x,f.y,f.z)}
// Double tap: fly to the part's working zoom, or back out when already there.
function focus(type,o,s){
  let z,at;
  if(type==='pad'){z=30/o.r;at=o}
  else if(type==='reader'||type==='string'){z=170/Math.max(20,stringLen(o));at=mid(o)}
  else if(type==='arm'){z=150/(o.r0+o.len);at=o.cog}
  else if(o.track){z=14/ringGap(o);const w=screenToWorld(s);if(S.zoom<z*.9){fly(w.x-(s.x-CW/2)/z,w.y-(s.y-CH/2)/z,z);return}}
  else{z=120/radius(o);at=o}
  if(S.zoom>=z*.9){flyFit();return}
  const c=centred(at,z);fly(c.x,c.y,z);
}
function choose(type,o,play){select(type,o);if(play)audition(o,true);reveal()}
function removeObj(type,o){
  if(type==='cog'){W.cogs=W.cogs.filter(c=>c!==o);W.arms=W.arms.filter(a=>a.cog!==o);W.belts=W.belts.filter(b=>b.a!==o&&b.b!==o)}
  else if(type==='dot')setDot(ui.dotOf.track,o,false);
  else W[type+'s']=W[type+'s'].filter(x=>x!==o);
  if(sel&&sel.o===o)select(null);
  buzz(18);changed(true);
}
function removeSel(){
  const items=group.length?group:sel?[sel]:[];group=[];
  for(const{type,o}of items)if(type!=='belt'||W.belts.includes(o))removeObj(type,o);
}
const PART_LISTS=['cog','arm','pad','reader','string','belt'];
const typeOf=o=>PART_LISTS.find(t=>W[t+'s'].includes(o))||null;
function boxSelect(a,b){
  const x0=Math.min(a.x,b.x),x1=Math.max(a.x,b.x),y0=Math.min(a.y,b.y),y1=Math.max(a.y,b.y),inside=q=>q.x>=x0&&q.x<=x1&&q.y>=y0&&q.y<=y1,g=[];
  for(const o of W.cogs)if(inside(o))g.push({type:'cog',o});
  for(const o of W.pads)if(inside(o))g.push({type:'pad',o});
  for(const type of ['reader','string'])for(const o of W[type+'s'])if(inside(mid(o)))g.push({type,o});
  for(const o of W.arms)if(inside(armTip(o)))g.push({type:'arm',o});
  for(const o of W.belts)if(g.some(x=>x.o===o.a)&&g.some(x=>x.o===o.b))g.push({type:'belt',o});
  return g;
}
function drawGroup(){
  if(!group.length&&!marquee)return;
  cx.setTransform(dpr,0,0,dpr,0,0);cx.strokeStyle=SEL;cx.lineWidth=1;cx.setLineDash([5,4]);
  if(marquee){const a=toScreen(marquee.a),b=toScreen(marquee.b);cx.strokeRect(a.x,a.y,b.x-a.x,b.y-a.y)}
  cx.setLineDash([]);
  for(const{type,o}of group){
    if(type==='cog'||type==='pad'){const s=toScreen(o);circle(s.x,s.y,(type==='cog'?radius(o):o.r)*S.zoom+5)}
    else if(type==='arm'){const s=toScreen(armTip(o));circle(s.x,s.y,7)}
    else if(type!=='belt'){const a=toScreen({x:o.x1,y:o.y1}),b=toScreen({x:o.x2,y:o.y2});circle(a.x,a.y,6);circle(b.x,b.y,6);line(a.x,a.y,b.x,b.y)}
  }
}
function pickCog(p,filter=()=>true,tol=0){
  return W.cogs.filter(c=>filter(c)&&Math.hypot(c.x-p.x,c.y-p.y)<=radius(c)+tol).sort((a,b)=>b.layer-a.layer||a.n-b.n)[0]||null;
}
function segDist(p,x1,y1,x2,y2){const ex=x2-x1,ey=y2-y1,u=clamp(((p.x-x1)*ex+(p.y-y1)*ey)/(ex*ex+ey*ey||1),0,1);return Math.hypot(p.x-x1-ex*u,p.y-y1-ey*u)}
const discAngle=(c,p)=>Math.atan2(p.y-c.y,p.x-c.x)-renderTh(c);
function cellAt(c,p){
  const t=c.track,d=Math.hypot(p.x-c.x,p.y-c.y),r=radius(c),ring=t.rings>1?Math.round((d/r-.25)/.65*(t.rings-1)):0;
  if(ring<0||ring>=t.rings)return null;
  const step=TAU/t.div;return {ring,a:wrap(Math.round(wrap(discAngle(c,p))/step)*step)};
}
function placeCog(c,p){
  const axle=W.cogs.find(o=>o!==c&&Math.hypot(o.x-p.x,o.y-p.y)<=Math.max(S.coaxTol,10/S.zoom));
  c.x=axle?axle.x:p.x;c.y=axle?axle.y:p.y;
  c.layer=axle?Math.max(...stackOf(axle).filter(o=>o!==c).map(o=>o.layer))+1:0;
  if(!axle)snap(c);solve();
}
// The cog you push becomes the train's only motor; under tempo lock it is set in revolutions per bar.
function drive(c,rpm){
  if(c.driver)for(const o of W.cogs)if(o.driver===c.driver){o.motor=0;o.revs=0}
  if(S.bpm)c.revs=rpm*S.beats/S.bpm;else c.motor=rpm;solve();
}
function detent(rpm){
  const a=Math.abs(rpm);if(a<3)return 0;
  return Math.sign(rpm)*detents().reduce((b,d)=>Math.abs(Math.log(d/a))<Math.abs(Math.log(b/a))?d:b);
}
const trackScrap=s=>{ui.scrap=offStage(s)};
function endScrap(type,o){const gone=ui.scrap;ui.scrap=false;if(gone)removeObj(type,o);else changed(true)}
const TRACK_DEFAULT={rings:12,base:48,scale:'pentatonic',div:16},newTrack=()=>({...TRACK_DEFAULT,dots:[]});
const STRING_DEFAULT={kind:'string',f200:220,quant:1,decay:2.5,damp:.5,hard:.5,level:.7,velSense:.7};
function newPart(kind,p){
  const id=W.next++;
  if(kind==='cog'||kind==='disc')return {type:'cog',o:{id,x:p.x,y:p.y,n:kind==='disc'?48:12,motor:0,revs:0,turns:0,layer:0,th:0,w:0,
    track:kind==='disc'?newTrack():null}};
  if(kind==='pad')return {type:'pad',o:{id,x:p.x,y:p.y,r:12,voice:makeVoice('kick'),hits:0,lastV:0}};
  if(kind==='reader')return {type:'reader',o:{id,x1:0,y1:0,x2:0,y2:0,voice:makeVoice('modal'),tines:0,combBase:60,combScale:'major',hits:0,lastV:0}};
  if(kind==='string')return {type:'string',o:{id,x1:0,y1:0,x2:0,y2:0,...STRING_DEFAULT,hits:0,lastV:0}};
  return {type:'arm',o:{id,cog:null,a:0,r0:0,len:4,flex:.5}};
}
function placePart(type,o,p){
  if(type==='cog')placeCog(o,p);
  else if(type==='pad'){o.x=p.x;o.y=p.y}
  else if(type==='string')Object.assign(o,{x1:p.x-60,y1:p.y,x2:p.x+60,y2:p.y});
  else if(type==='reader'){const d=pickCog(p,c=>c.track);
    if(d){const a=Math.atan2(p.y-d.y,p.x-d.x),r0=ringR(d,0)-6,r1=ringR(d,d.track.rings-1)+6;
      Object.assign(o,{x1:d.x+Math.cos(a)*r0,y1:d.y+Math.sin(a)*r0,x2:d.x+Math.cos(a)*r1,y2:d.y+Math.sin(a)*r1})}
    else Object.assign(o,{x1:p.x-30,y1:p.y,x2:p.x+30,y2:p.y})}
  else{const c=pickCog(p);o.cog=c;if(c){o.a=wrap(Math.atan2(p.y-c.y,p.x-c.x)-renderTh(c));o.len=Math.max(4,Math.hypot(p.x-c.x,p.y-c.y))}}
}

const G={
  marquee(s0){
    const a=screenToWorld(s0);marquee={a,b:a};
    return {move(s){marquee.b=screenToWorld(s)},cancel(){marquee=null},
      up(s){const b=screenToWorld(s);marquee=null;sel=null;group=boxSelect(a,b)},
      tap(s){marquee=null;if(chromeHit(s))return;
        const g=begin(s),o=g.o,type=o&&typeOf(o);g.cancel?.();if(!type)return;
        const current=group.length?group:sel?[sel]:[],rest=current.filter(x=>x.o!==o);
        sel=null;group=rest.length<current.length?rest:[...current,{type,o}]}};
  },
  group(s0){
    const p0=screenToWorld(s0),start=group.map(({o})=>({o,s:{x:o.x,y:o.y,x1:o.x1,y1:o.y1,x2:o.x2,y2:o.y2}}));
    return {o:null,move(s){trackScrap(s);const q=screenToWorld(s),dx=q.x-p0.x,dy=q.y-p0.y;
        for(const{o,s:k}of start){if('x'in o){o.x=k.x+dx;o.y=k.y+dy}if('x1'in o){o.x1=k.x1+dx;o.y1=k.y1+dy;o.x2=k.x2+dx;o.y2=k.y2+dy}}solve()},
      up(){if(ui.scrap){ui.scrap=false;removeSel()}else changed(true)}};
  },
  // An armed part: press or drag on the canvas to place it, as if carried in.
  armed(kind,s){const g=G.carry(kind);g.move(s);return {...g,tap(t){g.up(t)}}},
  // Carry a new part in: it lives in the machine while the pointer is on the stage.
  carry(kind){
    let made=null;const carry=ui.carry={ghost:null,band:null};
    const list=()=>W[made.type+'s'],present=()=>made&&list().includes(made.o);
    const out=()=>{if(present()){list().splice(list().indexOf(made.o),1);if(made.type==='cog')solve()}};
    const done=()=>{ui.carry=null};
    if(kind==='belt'){let a=null;
      return {move(s){const p=screenToWorld(s),c=!offStage(s)&&pickCog(p);if(!a&&c){a=c;buzz(6)}carry.band=a?{a,p}:null},
        up(s){done();const b=a&&!offStage(s)&&pickCog(screenToWorld(s));
          if(b&&b!==a&&!stackOf(a).includes(b)){W.belts.push({id:W.next++,a,b,cross:0});buzz(10);changed(true)}},
        cancel:done,tap:done};
    }
    return {
      move(s){
        const p=screenToWorld(s);
        if(offStage(s)){out();carry.ghost=null;return}
        made=made||newPart(kind,p);placePart(made.type,made.o,p);
        const live=made.type!=='arm'||made.o.cog;
        if(live&&!present()){list().push(made.o);if(made.type==='cog')solve();buzz(6)}else if(!live)out();
        carry.ghost=live?null:s;
      },
      up(s){done();if(!present())return;if(offStage(s)){out();return}select(made.type,made.o);changed(true);buzz(10);reveal()},
      cancel(){done();out()},tap:done,
    };
  },
  arm(a){
    return {o:a,move(s){trackScrap(s);const q=screenToWorld(s),c=a.cog;a.a=wrap(Math.atan2(q.y-c.y,q.x-c.x)-renderTh(c));a.len=Math.max(4,Math.hypot(q.x-c.x,q.y-c.y)-a.r0)},
      up(){endScrap('arm',a)},tap(){choose('arm',a)},dbl(s){focus('arm',a,s)}};
  },
  pad(o,s0,rim){
    const p0=screenToWorld(s0),dx=o.x-p0.x,dy=o.y-p0.y;
    return {o,move(s){const q=screenToWorld(s);if(rim)o.r=clamp(Math.hypot(q.x-o.x,q.y-o.y),3,200);else{trackScrap(s);o.x=q.x+dx;o.y=q.y+dy}},
      up(){if(rim)changed();else endScrap('pad',o)},tap(){choose('pad',o,true)},dbl(s){focus('pad',o,s)}};
  },
  // Readers and strings: an end stretches, the body moves. A string sings its new pitch as it stretches.
  span(type,o,s0,end){
    const p0=screenToWorld(s0),k={...o};let note=o.f200?Math.round(stringMidi(o)):0;
    return {o,move(s){const q=screenToWorld(s),dx=q.x-p0.x,dy=q.y-p0.y;if(!end)trackScrap(s);
        if(end!==2){o.x1=k.x1+dx;o.y1=k.y1+dy}if(end!==1){o.x2=k.x2+dx;o.y2=k.y2+dy}
        if(o.f200&&end){const m=Math.round(stringMidi(o));if(m!==note){note=m;buzz(3);audition(o,true)}}},
      up(){if(end)changed();else endScrap(type,o)},tap(){choose(type,o,true)},dbl(s){focus(type,o,s)}};
  },
  belt(b){return {o:b,move:trackScrap,up(){endScrap('belt',b)},tap(){b.cross=b.cross?0:1;buzz(8);changed(true)}}},
  cog(c,s0){
    const p0=screenToWorld(s0),stack=stackOf(c),start=stack.map(o=>[o,o.x,o.y]);
    return {o:c,
      move(s){trackScrap(s);const q=screenToWorld(s);for(const[o,x,y]of start){o.x=x+q.x-p0.x;o.y=y+q.y-p0.y}
        const bx=c.x,by=c.y;snap(c);const snapped=c.x!==bx||c.y!==by;if(snapped&&!ui.snapped)buzz(8);ui.snapped=snapped;
        for(const o of stack)if(o!==c){o.x+=c.x-bx;o.y+=c.y-by}solve()},
      up(){ui.snapped=false;if(ui.scrap){ui.scrap=false;for(const o of stack)removeObj('cog',o)}else changed(true)},
      tap(){choose('cog',c)},dbl(s){focus('cog',c,s)}};
  },
  // On the rim: push the teeth along to crank, pull outward or inward to change the tooth count.
  rim(c,s0){
    let mode=null,samples=[],grow=null;const min=c.track?12:6,ang=s=>{const q=screenToWorld(s);return Math.atan2(q.y-c.y,q.x-c.x)};
    return {o:c,
      move(s){
        if(!mode){const sc=toScreen(c),rx=s0.x-sc.x,ry=s0.y-sc.y,mx=s.x-s0.x,my=s.y-s0.y;
          mode=Math.abs(mx*rx+my*ry)>Math.abs(mx*ry-my*rx)?'teeth':'crank';
          if(mode==='crank'){ui.crank={c};samples=[{t:ui.t,a:ang(s0)}]}}
        if(mode==='teeth'){grow??=resizer(c);const n=clamp(Math.round(grow.radiusThrough(screenToWorld(s))*TAU/TOOTH),min,240);if(n!==c.n){grow.set(n);buzz(3)}return}
        const t=ui.t,last=samples[samples.length-1],a=last.a+wrapPi(ang(s)-last.a);samples.push({t,a});
        while(samples.length>2&&t-samples[1].t>90)samples.shift();
        const f=samples[0],dt=(t-f.t)/1000;if(dt>.01)drive(c,clamp((a-f.a)/dt*60/TAU,-600,600));
      },
      up(){
        if(mode==='teeth'){if(!grow?.anchor&&stackOf(c).length===1)snap(c);changed(true);return}
        if(mode==='crank'){const still=ui.t-samples[samples.length-1].t>110;drive(c,still?0:detent(motorRpm(c)));ui.crank=null;buzz(12);changed(true)}
      },
      cancel(){ui.crank=null},
      tap(){choose('cog',c)},dbl(s){focus('cog',c,s)}};
  },
  // The disc face at note zoom. Tap or drag to set pins. Hold a pin to tune its odds and accent.
  // Hold an empty socket, then sweep around the ring to spread k pins evenly (Euclid).
  notes(c,s0){
    const t=c.track,at=s=>cellAt(c,screenToWorld(s)),cell0=at(s0);let paint=null,mode='paint',sweep=null;
    const put=cell=>{if(!cell)return;const on=dotAt(t,cell.ring,cell.a)>=0;if(paint===null)paint=!on;
      if(on!==paint){setDot(t,{...cell,...brush},paint);if(paint){buzz(3);sound(discReader(c),ringNote(t,cell.ring),c.id*128+cell.ring)}}};
    const hold=setTimeout(()=>{
      if(!cell0||paint!==null)return;const i=dotAt(t,cell0.ring,cell0.a);
      if(i>=0){mode='odds';ui.dotOf=c;ui.dotAt=screenToWorld(s0);select('dot',t.dots[i]);buzz(12)}
      else{mode='euclid';sweep={a:discAngle(c,screenToWorld(s0)),acc:0,k:-1};buzz(12)}
    },450);
    const euclid=s=>{const a=discAngle(c,screenToWorld(s));sweep.acc+=wrapPi(a-sweep.a);sweep.a=a;
      const n=t.div,k=clamp(Math.round(Math.abs(sweep.acc)/(TAU/n)),0,n),dir=sweep.acc<0?-1:1;if(k===sweep.k)return;sweep.k=k;
      t.dots=t.dots.filter(d=>d.ring!==cell0.ring);
      for(let j=0;j<n;j++)if(k&&(j*k)%n<k)t.dots.push({ring:cell0.ring,a:wrap(cell0.a+dir*j*TAU/n),...brush});buzz(3)};
    return {o:c,
      move(s){if(mode==='euclid')return euclid(s);if(mode!=='paint')return;clearTimeout(hold);if(paint===null)put(cell0);put(at(s))},
      up(){clearTimeout(hold);changed()},cancel(){clearTimeout(hold)},
      tap(){clearTimeout(hold);put(cell0);if(!isSel(c))select('cog',c);changed()},
      dbl(s){if(cell0)setDot(t,{...cell0,...brush},dotAt(t,cell0.ring,cell0.a)<0);focus('cog',c,s)}};
  },
  pan(s0){
    const vx=view.x,vy=view.y;
    return {o:null,move(s){view.x=vx-(s.x-s0.x)/S.zoom;view.y=vy-(s.y-s0.y)/S.zoom},up(){changed()},tap(){select(null)},dbl(){flyFit()}};
  },
};
function begin(s){
  const ch=chromeHit(s);if(ch)return ch();
  const p=screenToWorld(s),Z=S.zoom,dist=q=>Math.hypot(q.x-p.x,q.y-p.y)*Z;
  // The nearest small part under the finger wins: a hammer sweeping past a pad must not steal the tap.
  let best=null;const near=(d,reach,make)=>{if(d<=reach&&(!best||d<best.d))best={d,make}};
  for(const a of W.arms)near(dist(armTip(a))-3.3*Z,20,()=>G.arm(a));
  for(const o of W.pads){const d=dist(o),r=o.r*Z;near(d-r,Math.max(8,22-r),()=>G.pad(o,s,r>=18&&d>=r-8))}
  for(const[type,list]of [['reader',W.readers],['string',W.strings]])for(const o of list){
    near(dist({x:o.x1,y:o.y1}),22,()=>G.span(type,o,s,1));near(dist({x:o.x2,y:o.y2}),22,()=>G.span(type,o,s,2));
    near(segDist(p,o.x1,o.y1,o.x2,o.y2)*Z+8,22,()=>G.span(type,o,s,0))}
  for(const o of W.belts)for(const l of beltLines(o))near(segDist(p,...l)*Z+10,22,()=>G.belt(o));
  if(best)return best.make();
  const c=pickCog(p,()=>true,14/Z);
  if(c){const d=dist(c),Rs=radius(c)*Z;
    if(Rs>=30&&Math.abs(d-Rs)<=14)return G.rim(c,s);
    if(noteMode(c)&&d>Math.max(22,Rs*.2)&&cellAt(c,p))return G.notes(c,s);
    return G.cog(c,s)}
  return G.pan(s);
}

function pointerGesture(e,s){
  if(e.ctrlKey||e.metaKey)return G.marquee(s);
  if(ui.armed&&!offStage(s))return G.armed(ui.armed,s);
  const g=begin(s);
  if(group.length>1&&g.o&&group.some(x=>x.o===g.o)){g.cancel?.();return G.group(s)}
  if(group.length&&!g.o?.track)group=[];
  return g;
}
cv.addEventListener('contextmenu',e=>e.preventDefault());
const local=e=>{const r=cv.getBoundingClientRect();return {x:e.clientX-r.left,y:e.clientY-r.top}};
function zoomAt(s,zoom,anchor){S.zoom=clamp(zoom,ZMIN,ZMAX);view.x=anchor.x-(s.x-CW/2)/S.zoom;view.y=anchor.y-(s.y-CH/2)/S.zoom}
const fingers=new Map();let pinch=null,gesture=null;
function pinchNow(){const[a,b]=[...fingers.values()];return {mid:{x:(a.x+b.x)/2,y:(a.y+b.y)/2},d:Math.hypot(a.x-b.x,a.y-b.y)||1}}
function dropGesture(){
  if(gesture){const g=gesture.g;if(g.cancel)g.cancel();else if(gesture.moved)g.up?.(gesture.last);gesture=null}
  ui.scrap=false;ui.crank=null;ui.carry=null;
}
cv.addEventListener('pointerdown',e=>{
  cv.setPointerCapture(e.pointerId);const s=local(e);fingers.set(e.pointerId,s);ui.cam=null;
  if(fingers.size===2){dropGesture();const p=pinchNow();pinch={...p,zoom:S.zoom,anchor:screenToWorld(p.mid)};return}
  if(fingers.size===1&&!pinch)gesture={id:e.pointerId,g:pointerGesture(e,s),s0:s,last:s,t0:e.timeStamp,moved:false};
});
cv.addEventListener('pointermove',e=>{
  if(!fingers.has(e.pointerId))return;const s=local(e);fingers.set(e.pointerId,s);
  if(pinch){if(fingers.size>=2){const p=pinchNow();zoomAt(p.mid,pinch.zoom*p.d/pinch.d,pinch.anchor)}return}
  if(!gesture||gesture.id!==e.pointerId)return;
  gesture.last=s;ui.t=e.timeStamp;
  if(!gesture.moved&&Math.hypot(s.x-gesture.s0.x,s.y-gesture.s0.y)<8)return;
  gesture.moved=true;gesture.g.move?.(s);
});
const endPointer=e=>{
  if(!fingers.delete(e.pointerId))return;
  if(pinch){if(!fingers.size){pinch=null;changed()}return}
  if(!gesture||gesture.id!==e.pointerId)return;
  const g=gesture;gesture=null;const s=local(e);
  ui.t=e.timeStamp;if(g.moved||e.timeStamp-g.t0>450){g.g.up?.(s);return}
  const t=e.timeStamp,l=ui.lastTap;
  if(l&&t-l.t<400&&Math.hypot(s.x-l.s.x,s.y-l.s.y)<30&&l.o===g.g.o&&g.g.dbl){ui.lastTap=null;g.g.dbl(s)}
  else{ui.lastTap={t,s,o:g.g.o};g.g.tap?.(s)}
};
cv.addEventListener('pointerup',endPointer);cv.addEventListener('pointercancel',endPointer);
// Trackpad pinch arrives as wheel with ctrlKey and small deltas; a mouse notch is ~100 px, or 3 lines.
cv.addEventListener('wheel',e=>{
  e.preventDefault();ui.cam=null;const s=local(e),dy=e.deltaY*(e.deltaMode===1?33:e.deltaMode===2?CH:1);
  zoomAt(s,S.zoom*Math.exp(-dy*(e.ctrlKey?.01:.002)),screenToWorld(s));
},{passive:false});
document.addEventListener('pointerup',initAudio);
document.addEventListener('keydown',e=>{
  initAudio();
  if(/INPUT|TEXTAREA|SELECT/.test(e.target.tagName)||rollKey(e))return;
  const k=e.key.toLowerCase();
  if((e.ctrlKey||e.metaKey)&&!e.altKey&&(k==='z'||k==='y')){e.preventDefault();if(k==='z'&&!e.shiftKey)edits.undo();else edits.redo();return}
  if(e.ctrlKey||e.metaKey||e.altKey)return;
  if(PART_KEYS[k])armPart(PART_KEYS[k]);else if(k==='s'||k==='escape')armPart(null);
  else if(e.key==='?')togglePanel();else if(e.key==='0')flyFit();
  else if(e.key==='Delete'||e.key==='Backspace')removeSel();else if(e.key===' '){e.preventDefault();setPaused(!clock.paused)}
});
document.addEventListener('visibilitychange',()=>{if(!document.hidden&&ac&&ac.state!=='running')ac.resume()});
addEventListener('resize',resize);
function armPart(kind){ui.armed=kind;cv.style.cursor=kind?'crosshair':''}
const BRUSH_DEFAULT={p:1,v:1},EU_DEFAULT={ring:0,k:3,rot:0},brush={...BRUSH_DEFAULT},eu={...EU_DEFAULT};

const dotAt=(t,ring,a)=>t.dots.findIndex(o=>o.ring===ring&&Math.abs(wrap(o.a-a+Math.PI)-Math.PI)<1e-6);
function setDot(t,cell,on){const i=dotAt(t,cell.ring,cell.a);if(on&&i<0)t.dots.push(cell);else if(!on&&i>=0)t.dots.splice(i,1)}

/* ---------- roll: the disc unwrapped into a strip, time left to right ---------- */
const NOTE_NAMES=['C','C#','D','D#','E','F','F#','G','G#','A','A#','B'];
const noteName=m=>NOTE_NAMES[m%12]+(Math.floor(m/12)-1);
let roll=null;
const rollDir=c=>c.w<0?-1:1;
const rollU=(c,a)=>wrap(-rollDir(c)*a)/TAU;
function readerPlayheads(c){
  const R=ringR(c,Math.floor((c.track.rings-1)/2)),th=renderTh(c),out=[];
  for(const rd of W.readers){
    const dx=rd.x2-rd.x1,dy=rd.y2-rd.y1,fx=rd.x1-c.x,fy=rd.y1-c.y,A=dx*dx+dy*dy,B=2*(fx*dx+fy*dy),disc=B*B-4*A*(fx*fx+fy*fy-R*R);
    if(!A||disc<0)continue;
    for(const s of [(-B-Math.sqrt(disc))/(2*A),(-B+Math.sqrt(disc))/(2*A)])
      if(s>=0&&s<=1)out.push({rd,u:rollU(c,Math.atan2(fy+dy*s,fx+dx*s)-th)});
  }
  return out;
}
function rollGeom(){const t=roll.c.track,L=roll.label,cw=Math.max(6,Math.floor((roll.box.clientWidth-L)/t.div));return {cw,W:L+cw*t.div,H:roll.row*t.rings}}
const colAngle=(c,col)=>wrap(-rollDir(c)*col*TAU/c.track.div);
const rollCol=(c,d)=>Math.round(rollU(c,d.a)*c.track.div)%c.track.div;
// The pointer in grid units: columns from the first step, rows from the top ring.
function rollAt(e){const g=rollGeom(),r=roll.cv.getBoundingClientRect();return {x:(e.clientX-r.left-roll.label)/g.cw,y:(e.clientY-r.top)/roll.row}}
function rollCell(e){
  const t=roll.c.track,at=rollAt(e),col=Math.floor(at.x),row=Math.floor(at.y);
  if(col<0||col>=t.div||row<0||row>=t.rings)return null;
  return {col,ring:t.rings-1-row,a:colAngle(roll.c,col)};
}
const pin=cell=>({ring:cell.ring,a:cell.a,p:brush.p,v:brush.v});
const sameCell=(a,b)=>a.ring===b.ring&&Math.abs(wrap(a.a-b.a+Math.PI)-Math.PI)<1e-6;

/* roll selection: a set of pins on the rolled disc, edited as one */
const rollSel=new Set();let rollSelOf=null,rollAnchor=null,rollClip=[];
const selPins=t=>t.dots.filter(d=>rollSel.has(d));
function rollEdit(){changed();edits.commit()}
// Shift a set of pins rigidly from where they started: columns wrap round the disc, rings stop at its edge.
function moveSel(c,from,dc,dr){
  const t=c.track,lo=Math.min(...from.map(f=>f[2])),hi=Math.max(...from.map(f=>f[2]));dr=clamp(dr,-lo,t.rings-1-hi);
  for(const[d,a,ring]of from){d.a=wrap(a-rollDir(c)*dc*TAU/t.div);d.ring=ring+dr}
}
// A selected pin landing on another pin replaces it.
function settleSel(t){t.dots=t.dots.filter(d=>rollSel.has(d)||![...rollSel].some(s=>sameCell(s,d)))}
function nudgeSel(c,dc,dr){const t=c.track,from=selPins(t).map(d=>[d,d.a,d.ring]);if(!from.length)return;moveSel(c,from,dc,dr);settleSel(t);rollEdit()}
function deleteSel(t){if(!rollSel.size)return;t.dots=t.dots.filter(d=>!rollSel.has(d));rollSel.clear();rollEdit()}
function copySel(c){
  const sel=selPins(c.track);if(!sel.length)return false;
  const cols=sel.map(d=>rollCol(c,d)),min=Math.min(...cols);rollClip=sel.map((d,i)=>({dc:cols[i]-min,ring:d.ring,p:d.p,v:d.v}));return true;
}
function pasteClip(c){
  const t=c.track;if(!rollClip.length)return;
  const head=readerPlayheads(c)[0],at=rollAnchor??(head?Math.floor(head.u*t.div):0);
  rollSel.clear();
  for(const q of rollClip){const d={ring:Math.min(q.ring,t.rings-1),a:colAngle(c,(at+q.dc)%t.div),p:q.p,v:q.v};t.dots.push(d);rollSel.add(d)}
  settleSel(t);rollEdit();
}
function brushSel(t,k){const sel=selPins(t);if(!sel.length)return;for(const d of sel)d[k]=brush[k];changed()}
// Keys go to the roll while it has focus or a selection; part keys stay off the canvas then.
function rollKey(e){
  if(!roll||!roll.cv.offsetParent)return false;
  const c=roll.c,t=c.track;if(document.activeElement!==roll.cv&&!selPins(t).length)return false;
  const k=e.key.toLowerCase();
  if(e.ctrlKey||e.metaKey){
    const op={c:()=>copySel(c),x:()=>{if(copySel(c))deleteSel(t)},v:()=>pasteClip(c),a:()=>{for(const d of t.dots)rollSel.add(d)}}[k];
    if(!op||e.altKey)return false;e.preventDefault();op();return true;
  }
  const arrow={arrowleft:[-1,0],arrowright:[1,0],arrowup:[0,1],arrowdown:[0,-1]}[k];
  if(arrow){e.preventDefault();nudgeSel(c,arrow[0],arrow[1]*(e.shiftKey?SCALES[t.scale].length:1));return true}
  if(k==='delete'||k==='backspace'){e.preventDefault();deleteSel(t);return true}
  if(k==='escape'&&rollSel.size){rollSel.clear();return true}
  return !!PART_KEYS[k]||k==='s';
}
function drawRoll(){
  const c=roll.c,t=c.track,g=rollGeom(),cv2=roll.cv,x=cv2.getContext('2d'),gw=g.cw*t.div,L=roll.label,RH=roll.row;
  if(cv2.width!==g.W*dpr||cv2.height!==g.H*dpr){cv2.width=g.W*dpr;cv2.height=g.H*dpr;cv2.style.width=g.W+'px';cv2.style.height=g.H+'px'}
  x.setTransform(dpr,0,0,dpr,0,0);x.fillStyle=BG;x.fillRect(0,0,g.W,g.H);
  x.font=`${clamp(Math.round(RH*.7),9,12)}px monospace`;x.textBaseline='middle';
  for(let row=0;row<t.rings;row++){
    const note=ringNote(t,t.rings-1-row),y=row*RH;
    x.fillStyle=DIM;x.fillText(noteName(note),2,y+RH/2);
    if(note%12===t.base%12){x.fillStyle=PAD;x.fillRect(L,y,gw,RH)}
  }
  x.lineWidth=1;
  for(let k=0;k<=t.div;k++){x.strokeStyle=k%4?FAINT:DIM;x.beginPath();x.moveTo(L+k*g.cw+.5,0);x.lineTo(L+k*g.cw+.5,g.H);x.stroke()}
  x.strokeStyle=FAINT;
  for(let r=0;r<=t.rings;r++){x.beginPath();x.moveTo(L,r*RH+.5);x.lineTo(L+gw,r*RH+.5);x.stroke()}
  for(const d of t.dots){
    if(d.ring>=t.rings)continue;
    const hgt=(RH-1)*(.35+.65*d.v);
    const px=L+rollU(c,d.a)*gw+1,on=rollSel.has(d);
    x.fillStyle=hot(d.flash,on?SEL:FG,x);x.globalAlpha*=.3+.7*d.p;x.fillRect(px,(t.rings-d.ring)*RH-hgt,g.cw-1,hgt);x.globalAlpha=1;
    if(on){x.strokeStyle=SEL;x.lineWidth=1;x.strokeRect(px+.5,(t.rings-1-d.ring)*RH+1.5,g.cw-2,RH-2)}
  }
  x.lineWidth=2;
  for(const p of readerPlayheads(c)){x.strokeStyle=hot(p.rd.flash,READ,x);x.beginPath();x.moveTo(L+p.u*gw,0);x.lineTo(L+p.u*gw,g.H);x.stroke();x.globalAlpha=1}
  const m=roll.marquee;
  if(m){x.lineWidth=1;x.strokeStyle=SEL;x.setLineDash([5,4]);x.strokeRect(L+m.x0*g.cw+.5,m.y0*RH+.5,(m.x1-m.x0)*g.cw,(m.y1-m.y0)*RH);x.setLineDash([])}
}
// The strip goes into `strip`, its controls into `tools`; row and label are in CSS px.
function rollSection(c,{strip,tools,row,label}){
  const t=c.track,cv2=el('canvas'),box=el('div',{className:'roll'},cv2);
  box.style.cssText='overflow-x:auto;margin:4px 0';cv2.style.cssText='display:block;touch-action:none;cursor:crosshair';
  cv2.tabIndex=0;cv2.style.outline='none';
  roll={cv:cv2,box,c,row,label,marquee:null};
  if(rollSelOf!==c){rollSel.clear();rollAnchor=null;rollSelOf=c}
  // A press: ctrl/cmd draws a box (a click toggles one pin), a selected pin drags the set,
  // anything else clears a selection, or paints when there is none.
  let press=null;
  cv2.onpointerdown=e=>{
    const at=rollAt(e),cell=rollCell(e),hit=cell&&t.dots.find(d=>sameCell(d,cell));
    cv2.focus({preventScroll:true});cv2.setPointerCapture(e.pointerId);if(cell)rollAnchor=cell.col;
    if(e.ctrlKey||e.metaKey){press={hit};roll.marquee={x0:at.x,y0:at.y,x1:at.x,y1:at.y};return}
    if(hit&&rollSel.has(hit)){press={from:selPins(t).map(d=>[d,d.a,d.ring]),col:cell.col,ring:cell.ring,moved:false};return}
    if(rollSel.size){rollSel.clear();press=null;return}
    if(!cell){press=null;return}
    press={paint:!hit};setDot(t,pin(cell),press.paint);
  };
  cv2.onpointermove=e=>{
    if(!press)return;const at=rollAt(e);
    if(roll.marquee){Object.assign(roll.marquee,{x1:at.x,y1:at.y});return}
    if(press.from){const dc=Math.floor(at.x)-press.col,dr=t.rings-1-Math.floor(at.y)-press.ring;if(dc||dr||press.moved){press.moved=true;moveSel(c,press.from,dc,dr)}return}
    const cell=rollCell(e);if(cell)setDot(t,pin(cell),press.paint);
  };
  cv2.onpointerup=cv2.onpointercancel=()=>{
    const p=press,m=roll.marquee;press=null;roll.marquee=null;if(!p)return;
    if(m){
      const g=rollGeom(),x0=Math.min(m.x0,m.x1),x1=Math.max(m.x0,m.x1),y0=Math.min(m.y0,m.y1),y1=Math.max(m.y0,m.y1);
      if(Math.hypot((x1-x0)*g.cw,(y1-y0)*row)<4){if(p.hit&&!rollSel.delete(p.hit))rollSel.add(p.hit);return}
      rollSel.clear();
      for(const d of t.dots){const px=rollU(c,d.a)*t.div+.5,py=t.rings-d.ring-.5;if(px>=x0&&px<=x1&&py>=y0&&py<=y1)rollSel.add(d)}
      return;
    }
    if(p.from){if(p.moved){settleSel(t);changed()}return}
    changed();
  };
  const shift=k=>()=>{const s=rollDir(c)*TAU/t.div;for(const d of t.dots)d.a=wrap(d.a-k*s);changed()};
  strip.append(box);
  tools.append(el('button',{textContent:'shift <',onclick:shift(-1)}),el('button',{textContent:'shift >',onclick:shift(1)}));
  numRow(tools,'brush p',0,1,.01,...bind(brush,'p',()=>brushSel(t,'p')),BRUSH_DEFAULT.p);
  numRow(tools,'brush vel',0,1,.01,...bind(brush,'v',()=>brushSel(t,'v')),BRUSH_DEFAULT.v);
  for(const[k,max]of [['ring',35],['k',96],['rot',95]])numRow(tools,'euclid '+k,0,max,1,...bind(eu,k),EU_DEFAULT[k]);
  tools.append(el('button',{textContent:'euclid fill',onclick:()=>{
    const n=t.div,ring=Math.min(eu.ring,t.rings-1);t.dots=t.dots.filter(d=>d.ring!==ring);
    for(let j=0;j<n;j++)if(((j+eu.rot)*eu.k)%n<eu.k)t.dots.push({ring,a:wrap(-rollDir(c)*j*TAU/n),...brush});
    changed()}}));
  for(const rd of new Set(readerPlayheads(c).map(p=>p.rd)))tools.append(el('button',{textContent:'reader '+rd.id+' patch',onclick:()=>select('reader',rd)}));
}

/* ---------- panel ---------- */
const panel=$('#panel'),rows=[],readouts=[];
function el(tag,props={},...kids){const e=Object.assign(document.createElement(tag),props);e.append(...kids);return e}
function section(title,parent=panel){const f=el('fieldset',{},el('legend',{textContent:title}));parent.append(f);return f}
// A slider and its number. Wheel over either nudges by 1% of the range, shift by one step;
// shift-drag on the slider moves ten times finer; double-click resets to `def` when there is one.
function numRow(parent,label,min,max,step,get,set,def){
  const range=el('input',{type:'range',min,max,step}),num=el('input',{type:'number',min,max,step});
  const sync=()=>{const v=get();range.value=v;num.value=+(+v).toFixed(4)};
  const put=v=>{if(!Number.isFinite(v))return;set(clamp(+(Math.round(v/step)*step).toFixed(6),min,max));sync()};
  range.oninput=()=>put(+range.value);num.oninput=()=>{const v=+num.value;if(Number.isFinite(v)){set(clamp(v,min,max));sync()}};num.onchange=sync;
  const coarse=Math.max(1,Math.round((max-min)/100/step))*step;
  const nudge=e=>{e.preventDefault();put(+get()-Math.sign(e.deltaY||e.deltaX)*(e.shiftKey?step:coarse))};
  range.onwheel=num.onwheel=nudge;
  range.onmousedown=e=>{
    if(!e.shiftKey)return;e.preventDefault();
    const x0=e.clientX,v0=+get(),per=(max-min)/range.clientWidth/10;
    const move=m=>put(v0+(m.clientX-x0)*per),up=()=>{removeEventListener('mousemove',move);removeEventListener('mouseup',up)};
    addEventListener('mousemove',move);addEventListener('mouseup',up);
  };
  const row=el('div',{className:'row'},el('span',{textContent:label}),range,num);row.dataset.k=label;
  if(def!==undefined){row.ondblclick=()=>put(def);row.title='double-click: '+def}
  parent.append(row);rows.push(sync);sync();return row;
}
function selRow(parent,label,options,get,set){
  const s=el('select',{},...options.map(o=>el('option',{value:o,textContent:o})));
  const sync=()=>{s.value=get()};s.onchange=()=>set(s.value);
  const row=el('div',{className:'row'},el('span',{textContent:label}),s,el('span'));row.dataset.k=label;
  parent.append(row);rows.push(sync);sync();return row;
}
function roRow(parent,label,fn){const b=el('b');const row=el('div',{className:'ro'},el('span',{textContent:label}),b);row.dataset.k=label;parent.append(row);readouts.push([b,fn])}
const bind=(o,k,after)=>[()=>o[k],v=>{o[k]=v;after&&after()}];
const syncRows=()=>{for(const s of rows)s()};

// Every build shows the same sections; `lead` adds its own transport buttons first.
function buildPanel(lead){
  const tr=section('transport');
  const btn=(label,fn)=>{const b=el('button',{textContent:label,onclick:fn});b.dataset.act=label;tr.append(b);return b};
  lead?.(btn);
  btn('undo',()=>edits.undo());btn('redo',()=>edits.redo());
  btn('audio',initAudio);btn('pause',()=>setPaused(!clock.paused));btn('del',removeSel);
  btn('demo',demo);btn('clear',()=>deserialize({S:{...S},SPACE:{...SPACE},view:{x:0,y:0},cogs:[],arms:[],pads:[],readers:[],belts:[],strings:[]}));
  btn('reset hits',()=>{for(const o of [...W.pads,...W.readers,...W.strings])o.hits=0;stats.events=stats.late=0;stats.log.length=0});
  const st=section('stats');
  roRow(st,'audio',()=>`${ac?ac.state:'off'} ${engineState}${ac?' '+ac.sampleRate+'Hz '+(1000*(ac.baseLatency||0)).toFixed(1)+'ms':''}`);
  roRow(st,'clock',()=>`${clock.audio?'audio':'perf'} ${clock.paused?'paused':'run'} t=${clock.view.toFixed(2)}`);
  roRow(st,'ahead ms',()=>(1000*(simT-clock.view)).toFixed(1));
  roRow(st,'steps/tick',()=>stats.stepsTick);
  roRow(st,'events',()=>`${stats.events} late ${stats.late}`);
  roRow(st,'engine',()=>`v${engineStats.voices??'-'} s${engineStats.strikes??'-'} st${engineStats.steals??'-'} dr${engineStats.drops??'-'}`);
  roRow(st,'peak dB',()=>peak>0?(20*Math.log10(peak)).toFixed(1):'-inf');
  roRow(st,'storage',()=>storage.ok?'local':'off: '+storage.why+', use json export');
  roRow(st,'world',()=>`c${W.cogs.length} a${W.arms.length} p${W.pads.length} r${W.readers.length} b${W.belts.length} s${W.strings.length}`);
  inspector=section('selection');inspector.id='inspector';
  const wf=section('world');
  for(const[k,l,mn,mx,st2]of S_DEFS)numRow(wf,l,mn,mx,st2,...bind(S,k,()=>{applyAudioSettings();changed(/Tol|bpm|beats/.test(k))}),S_DEFAULT[k]);
  const sp=section('space');
  for(const[k,l,mn,mx,st2]of SPACE_DEFS)numRow(sp,l,mn,mx,st2,...bind(SPACE,k,roomEdited),SPACE_DEFAULT[k]);
  const js=section('json'),ta=el('textarea'),msg=el('b');
  js.append(ta,el('button',{textContent:'export',onclick:()=>{ta.value=JSON.stringify(serialize());msg.textContent='ok'}}),
    el('button',{textContent:'import',onclick:()=>{try{deserialize(JSON.parse(ta.value));msg.textContent='ok'}catch(e){msg.textContent=e.message}}}),
    el('button',{textContent:'share',onclick:async()=>{ta.value=await shareURL();
      try{await navigator.clipboard.writeText(ta.value);msg.textContent='url copied'}catch(e){msg.textContent='url in box; clipboard: '+e.name}}}),msg);
  panelRows=rows.length;panelReadouts=readouts.length;
}
function applyAudioSettings(){
  if(master)master.gain.value=S.master;
  if(engineNode)engineNode.port.postMessage({type:'space',...SPACE});
}
let inspector=null,panelRows=0,panelReadouts=0;
const REVS=['-2','-1','-1/2','-1/3','-1/4','0','1/8','1/6','1/4','1/3','1/2','2/3','3/4','1','4/3','3/2','2','3','4','6','8'];
const frac=s=>{const[a,b=1]=s.split('/').map(Number);return a/b};
function select(type,o){sel=type?{type,o}:null;group=[];if(panelOpen())buildInspector()}
function voiceRows(f,owner,withNote){
  const v=owner.voice;
  selRow(f,'kind',Object.keys(VOICES),()=>v.kind,k=>{owner.voice=makeVoice(k,{level:v.level,velSense:v.velSense});buildInspector();changed()});
  for(const[k,l,a,b,s]of [...VOICE_DEFS[v.kind],...COMMON_DEFS]){
    if(k==='note'&&!withNote)continue;
    if(Array.isArray(a))selRow(f,l,a,()=>v[k],x=>{v[k]=x;changed()});else numRow(f,l,a,b,s,...bind(v,k,changed),makeVoice(v.kind)[k]);
  }
}
function buildInspector(){
  if(!inspector)return;
  rows.length=panelRows;readouts.length=panelReadouts;
  roll=null;
  inspector.replaceChildren(el('legend',{textContent:sel?`${sel.type} ${sel.o.id}`:'selection'}));
  const o=sel&&sel.o,f=inspector;
  if(sel&&sel.type==='cog'){
    numRow(f,'teeth',6,240,1,()=>o.n,v=>{resizer(o).set(v);changed(true)});
    numRow(f,'motor rpm',-600,600,.1,...bind(o,'motor',()=>changed(true)));
    selRow(f,'revs/bar',REVS,()=>REVS.reduce((a,b)=>Math.abs(frac(b)-o.revs)<Math.abs(frac(a)-o.revs)?b:a),v=>{o.revs=frac(v);changed(true)});
    numRow(f,'layer',0,7,1,...bind(o,'layer',()=>changed(true)));
    numRow(f,'x',-3000,3000,.5,...bind(o,'x',()=>changed(true)));
    numRow(f,'y',-3000,3000,.5,...bind(o,'y',()=>changed(true)));
    numRow(f,'phase deg',0,360,.1,()=>wrap(o.th)*180/Math.PI,v=>{o.th=v*Math.PI/180;changed()});
    roRow(f,'radius',()=>radius(o).toFixed(2));
    roRow(f,'rpm',()=>(o.w*60/TAU).toFixed(3));
    roRow(f,'period ms',()=>o.w?(1000*TAU/Math.abs(o.w)).toFixed(2):'-');
    roRow(f,'ratio',()=>o.driver?`${(o.w/o.driver.w||0).toFixed(4)} of c${o.driver.id}`:'-');
    roRow(f,'state',()=>o.jam?'JAM':o.w?'turning':'idle');
    if(o.track){
      const t=o.track;
      numRow(f,'rings',1,36,1,...bind(t,'rings',()=>{t.dots=t.dots.filter(d=>d.ring<t.rings);changed()}),TRACK_DEFAULT.rings);
      numRow(f,'base midi',12,108,1,...bind(t,'base',changed),TRACK_DEFAULT.base);
      selRow(f,'scale',Object.keys(SCALES),()=>t.scale,v=>{t.scale=v;changed()});
      numRow(f,'div',1,96,1,...bind(t,'div',changed),TRACK_DEFAULT.div);
      roRow(f,'dots',()=>t.dots.length);
      placeRoll(o,f);
      f.append(el('button',{textContent:'clear dots',onclick:()=>{t.dots=[];changed()}}),
        el('button',{textContent:'drop track',onclick:()=>{o.track=null;buildInspector();changed()}}));
    }else f.append(el('button',{textContent:'add track',onclick:()=>{o.track=newTrack();buildInspector();changed()}}));
  }else if(sel&&sel.type==='arm'){
    numRow(f,'angle deg',0,360,.1,()=>o.a*180/Math.PI,v=>{o.a=v*Math.PI/180;changed()});
    numRow(f,'r0',0,500,.5,...bind(o,'r0',changed));
    numRow(f,'len',1,800,.5,...bind(o,'len',changed));
    numRow(f,'flex',0,1,.01,...bind(o,'flex',changed),.5);
    roRow(f,'cog',()=>'c'+o.cog.id);
    roRow(f,'tip px/s',()=>(Math.abs(o.cog.w)*(o.r0+o.len)).toFixed(1));
  }else if(sel&&sel.type==='pad'){
    numRow(f,'x',-3000,3000,.5,...bind(o,'x',changed));
    numRow(f,'y',-3000,3000,.5,...bind(o,'y',changed));
    numRow(f,'radius',2,200,.5,...bind(o,'r',changed));
    voiceRows(f,o,true);
    roRow(f,'hits',()=>o.hits);
    roRow(f,'last vn',()=>o.lastV.toFixed(1));
  }else if(sel&&sel.type==='reader'){
    for(const k of ['x1','y1','x2','y2'])numRow(f,k,-3000,3000,.5,...bind(o,k,changed));
    voiceRows(f,o,false);
    numRow(f,'comb tines',0,36,1,...bind(o,'tines',changed),0);
    numRow(f,'comb base',12,108,1,...bind(o,'combBase',changed),60);
    roRow(f,'last note',()=>o.lastMidi==null?'-':noteName(o.lastMidi));
    selRow(f,'comb scale',Object.keys(SCALES),()=>o.combScale,v=>{o.combScale=v;changed()});
    roRow(f,'hits',()=>o.hits);
    roRow(f,'last vn',()=>o.lastV.toFixed(1));
  }else if(sel&&sel.type==='belt'){
    numRow(f,'crossed',0,1,1,...bind(o,'cross',()=>changed(true)));
    roRow(f,'ratio',()=>`c${o.b.id}/c${o.a.id} = ${(o.a.w?o.b.w/o.a.w:(o.cross?-1:1)*o.a.n/o.b.n).toFixed(4)}`);
  }else if(sel&&sel.type==='string'){
    selRow(f,'kind',['string','bar'],()=>o.kind,v=>{o.kind=v;changed()});
    numRow(f,'f @200px',20,2000,1,...bind(o,'f200',changed),STRING_DEFAULT.f200);
    numRow(f,'quantize',0,1,1,...bind(o,'quant',changed));
    for(const k of ['decay','damp','hard','level','velSense'])numRow(f,k,0,k==='decay'?8:k==='damp'?2:k==='level'?1.5:1,.01,...bind(o,k,changed),STRING_DEFAULT[k]);
    for(const k of ['x1','y1','x2','y2'])numRow(f,k,-3000,3000,.5,...bind(o,k,changed));
    roRow(f,'length',()=>stringLen(o).toFixed(1));
    roRow(f,'note',()=>{const m=stringMidi(o);return noteName(Math.round(m))+' '+m.toFixed(2)});
    roRow(f,'hits',()=>o.hits);
    roRow(f,'last u',()=>(o.lastU??0).toFixed(3));
  }
}
function refreshInspector(){if(panelOpen())syncRows()}

/* ---------- loop ---------- */
let roAt=0;const meterBuf=new Float32Array(2048);
function tick(){
  syncClock();
  if(!clock.paused)advance(nowSec()-clock.off+S.lookahead);
}
// currentTime advances in whole audio callbacks (10-40 ms on phones) and holds the start of the
// current one, so drawing from it stutters; the largest recent (currentTime - performance.now)
// is the true offset. getOutputTimestamp is not used: headless Chrome reports stale pairs.
const audioOffsets=[];let heardOffset=null,lastFrameSec=0;
function heardAt(frameSec){
  if(!clock.audio)return nowSec()-clock.off;
  audioOffsets.push(ac.currentTime-performance.now()/1000);if(audioOffsets.length>90)audioOffsets.shift();
  const target=Math.max(...audioOffsets)-(ac.outputLatency||ac.baseLatency||0)+clock.shift-clock.off;
  const dt=clamp(frameSec-lastFrameSec,0,.1),err=target-heardOffset;lastFrameSec=frameSec;
  if(heardOffset===null||Math.abs(err)>.25)heardOffset=target;else heardOffset+=clamp(err,-.02*dt,.02*dt);
  return frameSec+heardOffset;
}
function frame(ts){
  syncClock();
  if(!clock.paused)clock.view=Math.max(clock.view,heardAt(ts/1000));
  if(meter){meter.getFloatTimeDomainData(meterBuf);let m=0;for(const x of meterBuf)m=Math.max(m,Math.abs(x));peak=Math.max(m,peak*.95)}
  stepCamera();draw();drawGroup();
  const now=performance.now();
  if(roll&&roll.cv.offsetParent)drawRoll();
  if(panelOpen()&&now-roAt>250){roAt=now;readPalette();for(const[b,fn]of readouts)b.textContent=fn()}
  requestAnimationFrame(frame);
}
