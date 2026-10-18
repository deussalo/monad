/* ---------- the ball pit: a sphere of balls, each a note, that sound on its wall and on each other ---------- */
// The sphere is a circle on the stage: its diameter is both w and h (they are kept equal), so its bounding box, its
// fit and the probe's axes read it as they read any part.
// Monad's container brought into the machine. Tap inside a box to drop a ball, drag to throw one; arms and cogs in
// the box knock them about. The box's palette is its notes: each new ball takes the next of them (a shuffled bag, so
// every note comes round before one repeats) and its size from its pitch, low notes big. The box's voice plays them.
// Balls run on the sim's clock, ahead of the picture by the lookahead, so each keeps a short trail of where it was
// and is drawn where it was at the heard time.
const PIT={wall:.92,ball:.9,arm:.85,drag:.12,minV:18,cool:.045,maxV:1200,dt:1/240,trail:1/120,max:48,armR:2};
const PIT_NOTES=[60,62,64,67,69,72];
const ballR=m=>clamp(11*Math.pow(2,(60-m)/24),6,22);
const pitRect=p=>({L:p.x-p.w/2,R:p.x+p.w/2,T:p.y-p.h/2,B:p.y+p.h/2});
const pitR=p=>p.w/2;
const newPit=(id,p)=>({id,x:p.x,y:p.y,w:220,h:220,voice:readerVoice(),notes:[...PIT_NOTES],balls:[],nb:1,bag:[],hits:0,lastV:0});
function pitNote(p){
  if(!p.bag.length){p.bag=[...p.notes];for(let i=p.bag.length-1;i>0;i--){const j=Math.floor(Math.random()*(i+1));[p.bag[i],p.bag[j]]=[p.bag[j],p.bag[i]]}}
  return p.bag.pop()??60;
}
// A ball as the sim holds it; its trail starts where and when it is drawn from.
const makeBall=(p,note,x,y,vx,vy,id=p.nb++)=>({id,note,r:ballR(note),x,y,vx,vy,at:-9,flash:null,trail:[]});
// The palette changed: a ball whose note is gone takes the nearest that is left.
function pitRetune(p){
  p.bag=[];if(!p.notes.length)return;
  for(const b of p.balls)if(!p.notes.includes(b.note)){b.note=p.notes.reduce((a,n)=>Math.abs(n-b.note)<Math.abs(a-b.note)?n:a);b.r=ballR(b.note)}
}
// Move a box and everything in it, trails too, so its balls ride along when it is dragged.
function shiftPit(p,dx,dy){p.x+=dx;p.y+=dy;for(const b of p.balls){b.x+=dx;b.y+=dy;for(const s of b.trail){s[1]+=dx;s[2]+=dy}}}

// What can reach into a box this tick: arms whose sweep crosses it, and cogs that overlap it.
function pitReach(p){
  const {L,R,T,B}=pitRect(p),near=(x,y,r)=>x+r>L&&x-r<R&&y+r>T&&y-r<B;
  return {p,arms:W.arms.filter(a=>a.cog&&near(a.cog.x,a.cog.y,a.r0+a.len+PIT.armR+22)),cogs:W.cogs.filter(c=>near(c.x,c.y,radius(c)+25))};
}
function ballHit(p,b,vn,t,at){
  if(t-b.at<PIT.cool)return;b.at=t;b.flash=t;p.hits++;p.lastV=vn;p.flash=t;
  const v=probed(p.voice,t),note=b.note,key=p.id*128+note;
  fire(v,t,vn,at,'ball'+p.id+'.'+b.id,1,(when,amp,pan,vx,dark)=>playVoice(v,note,when,amp,pan,key,vx,0,dark));
}
function pitStep(q,dt,t){
  const p=q.p,bs=p.balls,{L,R,T,B}=pitRect(p),dk=Math.exp(-PIT.drag*dt);
  for(const b of bs){
    b.vx*=dk;b.vy*=dk;const sp=Math.hypot(b.vx,b.vy);if(sp>PIT.maxV){b.vx*=PIT.maxV/sp;b.vy*=PIT.maxV/sp}
    b.x+=b.vx*dt;b.y+=b.vy*dt;
  }
  // balls on each other: pushed apart by mass (the square of the radius), bounced, and both sound
  for(let i=0;i<bs.length;i++)for(let j=i+1;j<bs.length;j++){
    const a=bs[i],b=bs[j];let dx=b.x-a.x,dy=b.y-a.y;const min=a.r+b.r,d2=dx*dx+dy*dy;if(d2>=min*min)continue;
    let d=Math.sqrt(d2);if(d<1e-6){dx=1e-3;d=1e-3}
    const nx=dx/d,ny=dy/d,ma=a.r*a.r,mb=b.r*b.r,ov=min-d;
    a.x-=nx*ov*mb/(ma+mb);a.y-=ny*ov*mb/(ma+mb);b.x+=nx*ov*ma/(ma+mb);b.y+=ny*ov*ma/(ma+mb);
    const closing=(b.vx-a.vx)*nx+(b.vy-a.vy)*ny;
    if(closing<0){const J=-(1+PIT.ball)*closing/(1/ma+1/mb);a.vx-=J*nx/ma;a.vy-=J*ny/ma;b.vx+=J*nx/mb;b.vy+=J*ny/mb;
      if(-closing>PIT.minV){const at={x:a.x+nx*a.r,y:a.y+ny*a.r};ballHit(p,a,-closing,t,at);ballHit(p,b,-closing,t,at)}}
  }
  // arms sweep through and bat them, cogs shove them along their rims: neither sounds, the walls and balls do
  for(const b of bs){
    for(const a of q.arms){
      const c=a.cog,ang=c.th+a.a,ux=Math.cos(ang),uy=Math.sin(ang),x0=c.x+ux*a.r0,y0=c.y+uy*a.r0;
      const u=clamp((b.x-x0)*ux+(b.y-y0)*uy,0,a.len),qx=x0+ux*u,qy=y0+uy*u;let dx=b.x-qx,dy=b.y-qy;const min=b.r+PIT.armR,d2=dx*dx+dy*dy;
      if(d2>=min*min)continue;let d=Math.sqrt(d2);if(d<1e-6){dx=-uy;dy=ux;d=1}
      const nx=dx/d,ny=dy/d;b.x=qx+nx*min;b.y=qy+ny*min;
      const wx=-c.w*(qy-c.y),wy=c.w*(qx-c.x),vn=(b.vx-wx)*nx+(b.vy-wy)*ny;
      if(vn<0){b.vx-=(1+PIT.arm)*vn*nx;b.vy-=(1+PIT.arm)*vn*ny;a.flash=t}
    }
    for(const c of q.cogs){
      let dx=b.x-c.x,dy=b.y-c.y;const min=radius(c)+b.r,d2=dx*dx+dy*dy;if(d2>=min*min)continue;
      let d=Math.sqrt(d2);if(d<1e-6){dx=1;d=1}
      const nx=dx/d,ny=dy/d;b.x=c.x+nx*min;b.y=c.y+ny*min;
      const rx=-c.w*ny*radius(c),ry=c.w*nx*radius(c),vn=(b.vx-rx)*nx+(b.vy-ry)*ny;
      if(vn<0){b.vx-=(1+PIT.arm)*vn*nx;b.vy-=(1+PIT.arm)*vn*ny}
      // the rim drags the ball a little along with it
      const tx=-ny,ty=nx,vt=(b.vx-rx)*tx+(b.vy-ry)*ty;b.vx-=.25*vt*tx;b.vy-=.25*vt*ty;
    }
  }
  // the wall last, so nothing leaves the sphere: a ball past it is put back on it and bounced off its normal
  const Rw=pitR(p);
  for(const b of bs){
    let dx=b.x-p.x,dy=b.y-p.y;const lim=Rw-b.r;let d=Math.hypot(dx,dy);
    if(lim<=0){b.x=p.x;b.y=p.y}
    else if(d>lim){if(d<1e-9){dx=1;d=1}const nx=dx/d,ny=dy/d,vn=b.vx*nx+b.vy*ny;
      b.x=p.x+nx*lim;b.y=p.y+ny*lim;
      if(vn>0){b.vx-=(1+PIT.wall)*vn*nx;b.vy-=(1+PIT.wall)*vn*ny;if(vn>PIT.minV)ballHit(p,b,vn,t,{x:p.x+nx*Rw,y:p.y+ny*Rw})}}
    const tr=b.trail,last=tr[tr.length-1];
    if(!last||t+dt-last[0]>=PIT.trail){tr.push([t+dt,b.x,b.y]);if(tr.length>4&&tr[1][0]<clock.view-.05)tr.shift();if(tr.length>240)tr.splice(0,tr.length-240)}
  }
}
// Where a ball is at the heard time: between the two trail points either side of it.
function ballAt(b,t=clock.view){
  const tr=b.trail;if(!tr.length)return {x:b.x,y:b.y};
  if(t<=tr[0][0])return {x:tr[0][1],y:tr[0][2]};
  for(let i=tr.length-1;i>0;i--){const a=tr[i-1],c=tr[i];if(t>=a[0]){if(t>=c[0])return {x:c[1],y:c[2]};const f=(t-a[0])/(c[0]-a[0]||1);return {x:a[1]+(c[1]-a[1])*f,y:a[2]+(c[2]-a[2])*f}}}
  return {x:tr[0][1],y:tr[0][2]};
}
// A ball dropped or thrown now: it shows here now, and the sim, already ahead, takes it where it will be by then.
function dropBall(p,at,vx,vy,note=pitNote(p)){
  const r=ballR(note),lim=Math.max(0,pitR(p)-r),cl=q=>{const dx=q.x-p.x,dy=q.y-p.y,d=Math.hypot(dx,dy);return d<=lim?q:{x:p.x+dx/d*lim,y:p.y+dy/d*lim}};
  const {x,y}=cl(at),ahead=Math.max(0,simT-clock.view),w=cl({x:x+vx*ahead,y:y+vy*ahead});
  const b=makeBall(p,note,w.x,w.y,vx,vy);b.trail.push([clock.view,x,y]);
  if(p.balls.length>=PIT.max)p.balls.shift();
  p.balls.push(b);return b;
}
const pitBallAt=(p,w)=>{for(let i=p.balls.length-1;i>=0;i--){const b=p.balls[i],q=ballAt(b);if(Math.hypot(q.x-w.x,q.y-w.y)<=b.r+4/S.zoom)return b}return null};

/* drawing */
function drawPit(p){
  const R=pitR(p);
  cx.fillStyle=PAD;cx.globalAlpha=.35;circle(p.x,p.y,R,true);cx.globalAlpha=1;
  cx.lineWidth=2*PX;cx.strokeStyle=hot(p.flash,DIM);circle(p.x,p.y,R);cx.globalAlpha=1;
  // a glass sphere: a faint highlight arc inside its upper left, and a fainter shadow arc low right
  cx.lineWidth=1.5*PX;cx.strokeStyle=FG;cx.globalAlpha=.18;cx.beginPath();cx.arc(p.x,p.y,R*.86,Math.PI*1.08,Math.PI*1.42);cx.stroke();
  cx.globalAlpha=.08;cx.beginPath();cx.arc(p.x,p.y,R*.9,Math.PI*.1,Math.PI*.55);cx.stroke();cx.globalAlpha=1;cx.lineWidth=PX;
}
// A ball as the machine's other parts are drawn: a raised disc with a fine rim, its note a notch round the rim like
// an hour hand (C at the top), its name inside when there is room. A strike lights the rim and rings out, as a pad's.
function drawBall(q,r,note,flash,big){
  const g=glow(flash);
  if(flash!=null&&clock.view>=flash&&clock.view-flash<.4){const k=(clock.view-flash)/.4;cx.globalAlpha=(1-k)*.5;cx.strokeStyle=HOT;cx.lineWidth=PX;circle(q.x,q.y,r+2*PX+k*10*PX);cx.globalAlpha=1}
  cx.fillStyle=PAD;circle(q.x,q.y,r,true);
  cx.lineWidth=PX;cx.strokeStyle=g>.03?HOT:FG;circle(q.x,q.y,r);
  const a=(note%12)/12*TAU-Math.PI/2,ca=Math.cos(a),sa=Math.sin(a);
  cx.strokeStyle=g>.03?HOT:DIM;cx.lineWidth=1.5*PX;line(q.x+ca*r*.55,q.y+sa*r*.55,q.x+ca*r,q.y+sa*r);cx.lineWidth=PX;
  if(big&&r*S.zoom>=14){cx.fillStyle=DIM;cx.font=`${Math.min(11,r*S.zoom*.5)*PX}px monospace`;cx.textAlign='center';cx.textBaseline='middle';cx.fillText(NOTE_NAMES[note%12],q.x,q.y);cx.textAlign='start';cx.textBaseline='alphabetic'}
}
function drawBalls(p){
  const big=S.zoom>=1.2;
  for(const b of p.balls)drawBall(ballAt(b),b.r,b.note,b.flash,big);
}
// Its shelf icon: the sphere, balls of three sizes inside, and the path of one that has just struck the wall.
function drawBoxIcon(){
  const p={x:0,y:0,w:40,h:40,balls:[]};drawPit(p);
  cx.strokeStyle=DIM;cx.lineWidth=PX;cx.setLineDash([2,2]);line(-8,8,6,-17);line(6,-17,13,-3);cx.setLineDash([]);
  {const a=-Math.PI/2+.33;cx.strokeStyle=HOT;cx.lineWidth=1.5*PX;cx.beginPath();cx.arc(0,0,20,a-.3,a+.3);cx.stroke();cx.lineWidth=PX}
  for(const[x,y,m]of [[-7,8,48],[11,-2,60],[4,11,72]])drawBall({x,y},ballR(m)*.42,m,null,false);
}
// the handle on the rim, low right, that sizes a selected sphere
const pitCorner=p=>({x:p.x+pitR(p)*Math.SQRT1_2,y:p.y+pitR(p)*Math.SQRT1_2});
function drawPitSel(p){
  cx.lineWidth=1.5*PX;cx.strokeStyle=SEL;circle(p.x,p.y,pitR(p)+4*PX);
  const k=pitCorner(p);cx.fillStyle=SEL;circle(k.x,k.y,5*PX,true);cx.lineWidth=PX;
}

/* gestures */
Object.assign(G,{
  // the wall: drag to move the box and its balls; tap to select it
  pitBox(p,s0){
    let at=screenToWorld(s0);
    return {o:p,move(s){trackScrap(s);const q=screenToWorld(s);shiftPit(p,q.x-at.x,q.y-at.y);at=q},
      up(){endScrap('pit',p)},tap(){choose('pit',p)},dbl(s){focus('pit',p,s)}};
  },
  // the handle: size the sphere about its centre; balls left outside are pushed back in by the wall
  pitSize(p){
    return {o:p,readout:()=>`sphere: ${Math.round(p.w)} across`,
      move(s){const q=screenToWorld(s);p.w=p.h=Math.max(60,2*Math.hypot(q.x-p.x,q.y-p.y))},
      up(){changed()},tap(){choose('pit',p)}};
  },
  // inside: a tap drops a ball, a drag throws one the way the finger went; on a ball, a tap flicks it,
  // a drag throws it, and a hold takes it out
  pitIn(p,s0){
    const w0=screenToWorld(s0),ball=pitBallAt(p,w0),t0=performance.now();let held=false,last=null;
    const hold=ball&&setTimeout(()=>{held=true;p.balls.splice(p.balls.indexOf(ball),1);ui.pitBand=null;buzz(18);changed()},450);
    const pick=()=>{if(!isSel(p))select('pit',p)};
    const fling=s=>{const q=screenToWorld(s),dt=Math.max(.06,(performance.now()-t0)/1000),k=clamp(1/dt,2,8);
      let vx=(q.x-w0.x)*k,vy=(q.y-w0.y)*k;const sp=Math.hypot(vx,vy);if(sp>900){vx*=900/sp;vy*=900/sp}return [vx,vy]};
    return {o:p,readout:()=>ball?`ball ${noteName(ball.note)}`:`${p.balls.length} ball${p.balls.length===1?'':'s'}`,
      move(s){if(held)return;clearTimeout(hold);last=s;ui.pitBand={a:ball?ballAt(ball):w0,p:screenToWorld(s)}},
      up(s){clearTimeout(hold);ui.pitBand=null;if(held)return;const [vx,vy]=fling(s||last);
        if(ball){ball.vx=vx;ball.vy=vy}else{const b=dropBall(p,w0,vx,vy);sound(p,b.note,p.id*128+b.note)}
        pick();changed();buzz(6)},
      cancel(){clearTimeout(hold);ui.pitBand=null},
      tap(){clearTimeout(hold);if(held)return;const a=Math.random()*TAU;
        if(ball){ball.vx+=Math.cos(a)*220;ball.vy+=Math.sin(a)*220;sound(p,ball.note,p.id*128+ball.note)}
        else{const b=dropBall(p,w0,Math.cos(a)*160,Math.sin(a)*160);sound(p,b.note,p.id*128+b.note)}
        pick();changed();buzz(6)}};
  },
});
// What lies under a press on a sphere: its size handle when selected, its wall, or its inside.
function pitTouch(p,s,near){
  const q=screenToWorld(s),Z=S.zoom,d=Math.hypot(q.x-p.x,q.y-p.y);
  if(isSel(p)){const k=pitCorner(p);near(Math.hypot(q.x-k.x,q.y-k.y)*Z,18,()=>G.pitSize(p))}
  const inside=d<=pitR(p),edge=Math.abs(d-pitR(p))*Z;
  if(edge<=12)near(edge+6,18,()=>G.pitBox(p,s));
  return inside?p:null;
}

/* the sphere's sheet: its palette, a two-octave keyboard to choose the notes its balls take */
const pitKeysAt={oct:4};
function pitPalette(f,p){
  const cv=el('canvas',{className:'pit-keys'}),H=64,WHITE=[0,2,4,5,7,9,11],BLACK={1:0,3:1,6:3,8:4,10:5};
  const lab=el('span',{className:'pit-oct'}),count=el('span',{className:'pit-count'});
  const geom=()=>{const Wd=cv.clientWidth||300;return {Wd,ww:Wd/14}};
  const draw=()=>{const {Wd,ww}=geom();if(cv.width!==Math.round(Wd*dpr)){cv.width=Math.round(Wd*dpr);cv.height=H*dpr}
    const x=cv.getContext('2d'),on=new Set(p.notes),base=12*(pitKeysAt.oct+1),col=k=>getComputedStyle(document.documentElement).getPropertyValue(k).trim();
    const fg=col('--fg'),bg=col('--bg'),pad=col('--pad'),hotc=col('--hot');
    x.setTransform(dpr,0,0,dpr,0,0);x.clearRect(0,0,Wd,H);
    for(let i=0;i<14;i++){const m=base+12*Math.floor(i/7)+WHITE[i%7];x.fillStyle=on.has(m)?hotc:fg;x.beginPath();x.roundRect(i*ww+1,0,ww-2,H,[0,0,4,4]);x.fill()}
    for(let o=0;o<2;o++)for(const[k,after]of Object.entries(BLACK)){const m=base+12*o+ +k,xx=(o*7+after+1)*ww-ww*.3;x.fillStyle=on.has(m)?hotc:bg;x.beginPath();x.roundRect(xx,0,ww*.6,H*.6,[0,0,3,3]);x.fill();
      if(!on.has(m)){x.strokeStyle=pad;x.lineWidth=1;x.stroke()}}
    lab.textContent='C'+pitKeysAt.oct;count.textContent=`${p.notes.length} note${p.notes.length===1?'':'s'} · ${p.balls.length} ball${p.balls.length===1?'':'s'}`};
  const noteAt=e=>{const r=cv.getBoundingClientRect(),{ww}=geom(),px=e.clientX-r.left,py=e.clientY-r.top,base=12*(pitKeysAt.oct+1);
    if(px<0||px>=ww*14)return null;
    if(py<H*.6)for(let o=0;o<2;o++)for(const[k,after]of Object.entries(BLACK)){const xx=(o*7+after+1)*ww-ww*.3;if(px>=xx&&px<xx+ww*.6)return base+12*o+ +k}
    const i=Math.floor(px/ww);return base+12*Math.floor(i/7)+WHITE[i%7]};
  cv.style.touchAction='none';
  cv.onpointerdown=e=>{const m=noteAt(e);if(m==null)return;const i=p.notes.indexOf(m);
    if(i>=0){if(p.notes.length===1){buzz(18);return}p.notes.splice(i,1)}else{p.notes.push(m);p.notes.sort((a,b)=>a-b);sound(p,m,p.id*128+m)}
    pitRetune(p);buzz(4);draw();changed()};
  const shift=d=>{pitKeysAt.oct=clamp(pitKeysAt.oct+d,1,7);draw()};
  const bar=el('div',{className:'pit-keybar'},el('button',{textContent:'‹',ariaLabel:'octave down',onclick:()=>shift(-1)}),cv,el('button',{textContent:'›',ariaLabel:'octave up',onclick:()=>shift(1)}));
  const tools=el('div',{className:'pit-tools'},lab,count,el('span',{className:'wave-grow'}),
    el('button',{textContent:'clear balls',onclick:()=>{p.balls=[];draw();changed()}}));
  f.append(el('div',{className:'pit'},bar,tools));
  requestAnimationFrame(draw);new ResizeObserver(draw).observe(cv);rows.push(draw);
}
