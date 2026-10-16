const TAU=Math.PI*2,LN1000=6.907755278982137,MAXM=12,BLOCK=128;
const isPrime=n=>{for(let d=2;d*d<=n;d++)if(n%d===0)return false;return n>1};
const primeAtLeast=n=>{n=Math.max(2,Math.round(n));while(!isPrime(n))n++;return n};
const pow2AtLeast=n=>{let p=1;while(p<n)p<<=1;return p};
const softLimit=x=>x/(1+Math.abs(x));
// Sylvester Hadamard row r of length n, used for orthogonal tap/injection signs.
const hadamardSign=(r,c)=>{let b=r&c,p=0;while(b){p^=b&1;b>>=1}return p?-1:1};
const strikeSize=(d,o)=>7+3*d[o+6];
const packStrikes=list=>{const out=[];for(const m of list){out.push(m.at||0,m.k,m.amp,m.c,m.a,m.p,m.f.length,...m.f);
  for(let i=0;i<m.f.length;i++)out.push(m.g[i]);for(let i=0;i<m.f.length;i++)out.push(m.t[i])}return new Float64Array(out)};

class Allpass{
  constructor(len){this.buf=new Float32Array(len);this.len=len;this.i=0}
  run(x,g){const b=this.buf,i=this.i,d=b[i],v=x-g*d;b[i]=v;this.i=i+1===this.len?0:i+1;return d+g*v}
}

class Svf{
  constructor(){this.s1=0;this.s2=0;this.lp=0;this.hp=0}
  // Topology-preserving SVF (Zavalishin); returns [lp,hp] via fields to avoid allocation.
  run(x,g,k){const h=(x-(2*k+g)*this.s1-this.s2)/(1+g*(g+2*k)),b=g*h+this.s1,l=g*b+this.s2;
    this.s1=g*h+b;this.s2=g*b+l;this.lp=l;this.hp=h;return l}
}

class PitchShifter{
  constructor(fs){this.size=pow2AtLeast(fs*.2);this.mask=this.size-1;this.buf=new Float32Array(this.size);
    this.w=0;this.p=0;this.win=Math.round(fs*.07)}
  read(d){const pos=this.w-d,i=Math.floor(pos),f=pos-i,m=this.mask;return this.buf[i&m]*(1-f)+this.buf[(i+1)&m]*f}
  run(x,ratio){
    this.buf[this.w&this.mask]=x;
    const W=this.win;let p=this.p+(1-ratio)/W;p-=Math.floor(p);this.p=p;
    let q=p+.5;if(q>=1)q-=1;
    const a=Math.sin(Math.PI*p),b=Math.sin(Math.PI*q);
    const y=this.read(p*W+1)*a*a+this.read(q*W+1)*b*b;
    this.w++;return y;
  }
}


class Space{
  constructor(fs,lines,space){
    this.fs=fs;const N=this.N=lines;
    const minMs=37,maxMs=113;
    this.base=new Float64Array(N);this.len=new Float64Array(N);
    for(let i=0;i<N;i++)this.base[i]=primeAtLeast(fs*(minMs*Math.pow(maxMs/minMs,i/(N-1)))/1000);
    const maxLen=maxMs*fs/1000*3.2+fs*.004+8;
    this.size=pow2AtLeast(maxLen);this.mask=this.size-1;
    this.buf=new Float32Array(this.size*N);
    this.lp1=new Float64Array(N);this.lp2=new Float64Array(N);
    this.gl=new Float64Array(N);this.gm=new Float64Array(N);this.gh=new Float64Array(N);
    this.y=new Float64Array(N);
    this.inj=new Float64Array(N);this.oL=new Float64Array(N);this.oR=new Float64Array(N);
    const sc=1/Math.sqrt(N);
    for(let i=0;i<N;i++){this.inj[i]=hadamardSign(3,i)*sc;this.oL[i]=hadamardSign(5,i)*sc;this.oR[i]=hadamardSign(10,i)*sc}
    this.mc=new Float64Array(N);this.ms=new Float64Array(N);this.mrc=new Float64Array(N);this.mrs=new Float64Array(N);this.mdepth=new Float64Array(N);
    this.apLen=new Int32Array(N);this.apPos=new Int32Array(N);this.apOff=new Int32Array(N);let off=0;
    for(let i=0;i<N;i++){const l=primeAtLeast(fs*(2.9+i*.53)/1000);this.apLen[i]=l;this.apOff[i]=off;off+=l}
    this.apBuf=new Float32Array(off);
    this.diffusers=[4.77,3.59,12.73,9.31].map(ms=>new Allpass(primeAtLeast(fs*ms/1000)));
    this.preSize=pow2AtLeast(fs*.52);this.preMask=this.preSize-1;this.pre=new Float32Array(this.preSize);
    this.shift=new PitchShifter(fs);
    this.eqL=[new Svf(),new Svf()];this.eqR=[new Svf(),new Svf()];
    this.keys=Object.keys(space).filter(k=>typeof space[k]==='number');
    this.target={};this.cur={};
    this.holdBlocks=Math.ceil((fs*.52+this.size)/BLOCK);
    this.glide=1-Math.exp(-BLOCK/(fs*.05));this.sizeGlide=1-Math.exp(-BLOCK/(fs*.35));
    this.clear(space);
  }
  clear(space){
    const N=this.N,sh=this.shift;
    this.buf.fill(0);this.apBuf.fill(0);this.apPos.fill(0);this.pre.fill(0);this.w=0;this.preW=0;
    this.lp1.fill(0);this.lp2.fill(0);this.y.fill(0);
    for(let i=0;i<N;i++){const ph=TAU*i/N*2.37;this.mc[i]=Math.cos(ph);this.ms[i]=Math.sin(ph)}
    sh.buf.fill(0);sh.w=0;sh.p=0;
    for(const f of [...this.eqL,...this.eqR]){f.s1=0;f.s2=0;f.lp=0;f.hp=0}
    for(const d of this.diffusers){d.buf.fill(0);d.i=0}
    this.hpIn=0;this.shimLp=0;this.shimOut=0;this.duckEnv=0;this.denorm=1e-18;this.energy=0;
    this.assign(space);this.retune(true);
  }
  assign(space){for(const k of this.keys)this.target[k]=this.cur[k]=space[k]}
  set(p){for(const k in p)if(k in this.target&&Number.isFinite(+p[k]))this.target[k]=+p[k]}
  retune(snap){
    const t=this.target,c=this.cur,fs=this.fs,N=this.N;
    const k=snap?1:this.glide,ks=snap?1:this.sizeGlide;
    let moved=snap;
    for(const key of this.keys){const was=c[key];if(key==='size')c.size+=(t.size-c.size)*ks;else if(key==='freeze'||key==='shimmerPitch')c[key]=t[key];else c[key]+=(t[key]-c[key])*k;if(c[key]!==was)moved=true}
    for(let i=0;i<N;i++){const n=Math.hypot(this.mc[i],this.ms[i]);this.mc[i]/=n;this.ms[i]/=n}
    if(!moved)return;
    const size=Math.max(.2,Math.min(3,c.size)),frozen=c.freeze>=.5;
    const T=Math.max(.1,c.decay),TL=T*Math.max(.05,c.lowMult),TH=T*Math.max(.02,c.highMult);
    const cap=frozen?.99999:.9999;
    for(let i=0;i<N;i++){
      const L=this.base[i]*size;this.len[i]=L;
      const sec=(L+this.apLen[i])/fs;
      this.gm[i]=frozen?cap:Math.min(cap,Math.exp(-LN1000*sec/T));
      this.gl[i]=frozen?cap:Math.min(cap,Math.exp(-LN1000*sec/TL));
      this.gh[i]=frozen?cap:Math.min(cap,Math.exp(-LN1000*sec/TH));
      const hz=c.modRate*(.62+.76*i/(N-1)),w=TAU*hz/fs;
      this.mrc[i]=Math.cos(w);this.mrs[i]=Math.sin(w);
      this.mdepth[i]=c.modDepth*fs/1000*(.55+.45*((i*7)%N)/(N-1));
    }
    this.a1=1-Math.exp(-TAU*Math.min(c.lowCross,c.highCross*.8)/fs);
    this.a2=1-Math.exp(-TAU*c.highCross/fs);
    this.gDiff=Math.min(.78,c.diffusion*.78);this.gInner=Math.min(.7,c.density*.7);
    this.eqHpG=Math.tan(Math.PI*Math.min(c.lowCut,fs*.45)/fs);this.eqLpG=Math.tan(Math.PI*Math.min(c.highCut,fs*.45)/fs);
    this.ratio=Math.pow(2,c.shimmerPitch/12);
  }
  // inL/inR: dry block. Adds the wet signal into outL/outR.
  process(inL,inR,outL,outR,n){
    this.retune(false);
    const c=this.cur,N=this.N,fs=this.fs,mask=this.mask,size=this.size,buf=this.buf,len=this.len,y=this.y,apBuf=this.apBuf,apLen=this.apLen,apPos=this.apPos,apOff=this.apOff;
    const lp1=this.lp1,lp2=this.lp2,gl=this.gl,gm=this.gm,gh=this.gh,inj=this.inj,oL=this.oL,oR=this.oR;
    const mc=this.mc,ms=this.ms,mrc=this.mrc,mrs=this.mrs,md=this.mdepth,dif=this.diffusers;
    const a1=this.a1,a2=this.a2,gD=this.gDiff,gI=this.gInner,frozen=c.freeze>=.5;
    const preD=Math.max(1,Math.min(this.preSize-2,c.predelay*fs)),aHp=1-Math.exp(-TAU*30/fs);
    const shim=frozen?0:Math.min(.6,c.shimmer*.6),hiShim=1-Math.exp(-TAU*6000/fs);
    const duckAtk=1-Math.exp(-1/(fs*.004)),duckRel=1-Math.exp(-1/(fs*.35)),duck=c.duck;
    const width=c.width,wet=c.wet,dry=c.dry,houseK=2/N,inGain=frozen?0:1;
    const gHp=this.eqHpG,gLp=this.eqLpG,kq=1.41421356;
    let energy=0,w=this.w,dn=this.denorm;
    for(let s=0;s<n;s++){
      const x=(inL[s]+inR[s])*.5;
      // predelay (linear interp, smoothed length)
      this.pre[this.preW&this.preMask]=x;
      const pp=this.preW-preD,pi=Math.floor(pp),pf=pp-pi;
      let v=this.pre[pi&this.preMask]*(1-pf)+this.pre[(pi+1)&this.preMask]*pf;this.preW++;
      this.hpIn+=aHp*(v-this.hpIn);v-=this.hpIn;
      for(let d=0;d<4;d++)v=dif[d].run(v,d<2?gD:gD*.82);
      v=v*inGain+this.shimOut*shim;
      let sum=0;
      for(let i=0;i<N;i++){
        const o=i*size,cc=mc[i],ss=ms[i];
        mc[i]=cc*mrc[i]-ss*mrs[i];ms[i]=ss*mrc[i]+cc*mrs[i];
        const pos=w-len[i]-md[i]*(1+ss),ip=Math.floor(pos),f=pos-ip;
        const x0=buf[o+((ip-1)&mask)],x1=buf[o+(ip&mask)],x2=buf[o+((ip+1)&mask)],x3=buf[o+((ip+2)&mask)];
        const c1=.5*(x2-x0),c2=x0-2.5*x1+2*x2-.5*x3,c3=.5*(x3-x0)+1.5*(x1-x2);
        const r=((c3*f+c2)*f+c1)*f+x1;
        const l1=lp1[i]+a1*(r-lp1[i]),rest=r-l1,l2=lp2[i]+a2*(rest-lp2[i]);lp1[i]=l1;lp2[i]=l2;
        const absorbed=gl[i]*l1+gm[i]*l2+gh[i]*(rest-l2);
        const ap=apOff[i]+apPos[i],d=apBuf[ap],u=absorbed-gI*d;apBuf[ap]=u;
        apPos[i]=apPos[i]+1===apLen[i]?0:apPos[i]+1;
        const out=d+gI*u;y[i]=out;sum+=out;
      }
      const f=sum*houseK;let tl=0,tr=0;
      for(let i=0;i<N;i++){const r=y[i];tl+=r*oL[i];tr+=r*oR[i];buf[i*size+(w&mask)]=r-f+v*inj[i]+dn}
      dn=-dn;w++;
      // shimmer: octave(ish)-shifted tank fed back into the input, low-passed and soft-limited
      if(shim>0){this.shimLp+=hiShim*(this.shift.run((tl+tr)*.5,this.ratio)-this.shimLp);this.shimOut=softLimit(this.shimLp)}else this.shimOut=0;
      // wet EQ: 12 dB/oct high-pass then low-pass
      let l=tl,rr=tr;
      this.eqL[0].run(l,gHp,kq);l=this.eqL[0].hp;l=this.eqL[1].run(l,gLp,kq);
      this.eqR[0].run(rr,gHp,kq);rr=this.eqR[0].hp;rr=this.eqR[1].run(rr,gLp,kq);
      const m=(l+rr)*.5,sd=(l-rr)*.5*width;l=m+sd;rr=m-sd;
      const ax=Math.abs(x);this.duckEnv+=(ax>this.duckEnv?duckAtk:duckRel)*(ax-this.duckEnv);
      const g=wet*(1-duck*Math.min(1,this.duckEnv*6));
      outL[s]+=inL[s]*dry+l*g;outR[s]+=inR[s]*dry+rr*g;
      energy+=l*l+rr*rr;
    }
    this.w=w;this.denorm=dn;
    this.energy=energy/n;
  }
}

class CogsEngine extends AudioWorkletProcessor{
  constructor(options){
    super();
    const o=(options&&options.processorOptions)||{};
    const V=this.V=o.voices||48;
    this.space=new Space(sampleRate,o.lines||16,o.space);
    this.on=new Uint8Array(V);this.key=new Float64Array(V);this.nm=new Uint8Array(V);
    this.zr=new Float64Array(V*MAXM);this.zi=new Float64Array(V*MAXM);this.kr=new Float64Array(V*MAXM);this.ki=new Float64Array(V*MAXM);this.xg=new Float64Array(V*MAXM);
    this.cN=new Int32Array(V);this.cLen=new Int32Array(V);
    this.env=new Float64Array(V);this.envStep=new Float64Array(V);this.fade=new Float64Array(V);this.fadeStep=new Float64Array(V);this.pend=new Array(V).fill(null);this.pendAt=new Int32Array(V);this.dl=new Int32Array(V);
    this.pl=new Float64Array(V);this.pr=new Float64Array(V);this.plT=new Float64Array(V);this.prT=new Float64Array(V);this.e=new Float64Array(V);
    this.exc=new Float64Array(BLOCK);this.live=new Int32Array(MAXM);this.vb=new Float64Array(BLOCK);
    this.dryL=new Float32Array(BLOCK);this.dryR=new Float32Array(BLOCK);
    this.maxStarts=4*V;this.maxWait=8*BLOCK;
    this.reset(o);this.warm(o);
    this.damps=[];
    this.port.onmessage=ev=>{const m=ev.data;if(m.type==='strikes')this.queue.push(m.d);else if(m.type==='damp')this.damps.push(m);else if(m.type==='space')this.space.set(m)};
  }
  // V8 interprets code until it has run enough; cold, the first strike's blocks
  // took 4-8 ms against a 2.7 ms deadline and dropped out as clicks, and the first
  // shimmer blocks took up to 7 ms. Run every path silently at construction:
  // shimmer, full polyphony, stealing and overload. Then reset the same objects in
  // place: a Space allocated after warm-up runs cold again.
  warm(o){
    const l=new Float32Array(BLOCK),r=new Float32Array(BLOCK),out=[[l,r]],V=this.V;
    const sp=this.space;this.muted=true;this.queue=[];
    const strike=(k,t)=>({k,f:[110,230,370,520,700],g:[1,.6,.4,.3,.2],t:[t,t*.75,t*.5,t*.4,t*.25],amp:.1,c:.0008,a:.004,p:.3});
    for(let i=0;i<400;i++){
      // Both sides of every branch: V8 deoptimises when a path it never saw runs.
      if(i%50===0)sp.target.shimmer=sp.cur.shimmer=i%100?0:1;
      if(i%100===70||i%100===90)sp.target.freeze=sp.cur.freeze=i%100===70?1:0;
      if(i%40===0)this.queue.push(packStrikes([strike(i%3,2)]));
      if(i===100)this.queue.push(packStrikes(Array.from({length:2*V+4},(_,k)=>strike(1000+k,.3))));
      l.fill(0);r.fill(0);this.process([],out);
    }
    this.reset(o);
  }
  reset(o){
    for(const a of [this.on,this.key,this.nm,this.zr,this.zi,this.kr,this.ki,this.xg,this.cN,this.cLen,this.env,this.envStep,this.fade,this.fadeStep,
      this.pl,this.pr,this.plT,this.prT,this.e,this.exc,this.live,this.vb,this.dryL,this.dryR,this.pendAt,this.dl])a.fill(0);
    this.pend.fill(null);this.space.clear(o.space);
    this.queue=[packStrikes((o.schedule||[]).slice().sort((a,b)=>(a.at||0)-(b.at||0)))];this.queueAt=0;this.queueOff=0;this.damps=[];
    this.frame=0;this.quietBlocks=0;this.steals=0;this.drops=0;this.strikes=0;this.statAt=0;this.idle=false;this.muted=false;this.idleReported=false;this.waitFrom=-1;
  }
  post(m,transfer){if(!this.muted)this.port.postMessage(m,transfer)}
  active(){let n=0;for(let v=0;v<this.V;v++)n+=this.on[v];return n}
  load(v,d,o,off){
    const b=v*MAXM,fs=sampleRate,n=d[o+6],f=o+7,g=f+n,t=g+n,amp=d[o+2];let k=0;
    for(let i=0;i<n&&k<MAXM;i++){
      if(d[f+i]>=fs*.45||d[f+i]<=0)continue;
      const r=Math.exp(-LN1000/(Math.max(.01,d[t+i])*fs)),w=TAU*d[f+i]/fs;
      this.kr[b+k]=r*Math.cos(w);this.ki[b+k]=r*Math.sin(w);this.xg[b+k]=d[g+i]*amp;k++;
    }
    this.nm[v]=k;
    const len=Math.max(8,Math.round((d[o+3]||.002)*fs));this.cLen[v]=len;this.cN[v]=len;
    const p=Math.max(-1,Math.min(1,d[o+5]||0)),a=(p+1)*Math.PI/4;this.plT[v]=Math.cos(a);this.prT[v]=Math.sin(a);this.dl[v]=off||0;
  }
  offset(d,o){return Math.max(0,Math.min(BLOCK-1,Math.round(d[o]-this.frame)))}
  // false: every voice is already fading out, so the strike has to wait.
  start(d,o){
    const V=this.V,key=d[o+1],off=this.offset(d,o);
    for(let v=0;v<V;v++)if(this.on[v]&&this.key[v]===key&&this.fade[v]===0){this.strikes++;this.load(v,d,o,off);return true}
    let free=-1;for(let v=0;v<V;v++)if(!this.on[v]){free=v;break}
    const n=this.active(),poly=Math.sqrt(8/Math.max(8,n+1));
    if(free<0){
      let best=-1,be=Infinity;
      for(let v=0;v<V;v++)if(this.fade[v]===0&&this.e[v]<be){be=this.e[v];best=v}
      if(best<0)return false;
      this.strikes++;d[o+2]*=poly;this.steals++;this.fade[best]=1;this.fadeStep[best]=1/96;this.pend[best]=d;this.pendAt[best]=o;return true;
    }
    this.strikes++;d[o+2]*=poly;this.begin(free,d,o,off);return true;
  }
  begin(v,d,o,off){
    const b=v*MAXM;for(let k=0;k<MAXM;k++){this.zr[b+k]=0;this.zi[b+k]=0}
    this.on[v]=1;this.key[v]=d[o+1];this.fade[v]=0;this.pend[v]=null;this.load(v,d,o,off);this.pl[v]=this.plT[v];this.pr[v]=this.prT[v];
    const a=d[o+4]||0;if(a>.002){this.env[v]=0;this.envStep[v]=1/(a*sampleRate)}else{this.env[v]=1;this.envStep[v]=0}
  }
  renderVoice(v,n,L,R){
    const b=v*MAXM,nm=this.nm[v],exc=this.exc,vb=this.vb,zr=this.zr,zi=this.zi,kr=this.kr,ki=this.ki,xg=this.xg;
    let cN=this.cN[v];const cLen=this.cLen[v],norm=2/cLen;
    const exciting=cN>0;
    let delay=this.dl[v];
    if(exciting)for(let s=0;s<n;s++){if(delay>0){exc[s]=0;delay--}else if(cN>0){const ph=(cLen-cN+.5)/cLen;exc[s]=(1-Math.cos(TAU*ph))*.5*norm;cN--}else exc[s]=0}
    vb.fill(0,0,n);this.cN[v]=cN;this.dl[v]=delay;
    let e=0,k=0;
    // Partials ring independently; one that has decayed below -110 dB stops costing.
    const live=this.live;let nl=0;
    for(let j=0;j<nm;j++)if(exciting||zr[b+j]*zr[b+j]+zi[b+j]*zi[b+j]>1e-11)live[nl++]=b+j;
    for(;k+1<nl;k+=2){
      const p=live[k],q=live[k+1];
      let r1=zr[p],i1=zi[p],r2=zr[q],i2=zi[q];const a1=kr[p],c1=ki[p],g1=xg[p],a2=kr[q],c2=ki[q],g2=xg[q];
      if(exciting)for(let s=0;s<n;s++){const x=exc[s],n1=a1*r1-c1*i1+x*g1,n2=a2*r2-c2*i2+x*g2;i1=a1*i1+c1*r1;i2=a2*i2+c2*r2;r1=n1;r2=n2;vb[s]+=r1+r2}
      else for(let s=0;s<n;s++){const n1=a1*r1-c1*i1,n2=a2*r2-c2*i2;i1=a1*i1+c1*r1;i2=a2*i2+c2*r2;r1=n1;r2=n2;vb[s]+=r1+r2}
      zr[p]=r1;zi[p]=i1;zr[q]=r2;zi[q]=i2;e+=r1*r1+i1*i1+r2*r2+i2*i2;
    }
    if(k<nl){
      const p=live[k];let r=zr[p],i=zi[p];const a=kr[p],c=ki[p],g=xg[p];
      if(exciting)for(let s=0;s<n;s++){const nr=a*r-c*i+exc[s]*g;i=a*i+c*r;r=nr;vb[s]+=r}
      else for(let s=0;s<n;s++){const nr=a*r-c*i;i=a*i+c*r;r=nr;vb[s]+=r}
      zr[p]=r;zi[p]=i;e+=r*r+i*i;
    }
    let env=this.env[v],es=this.envStep[v],fl=this.fade[v],pl=this.pl[v],pr=this.pr[v];const fstep=this.fadeStep[v],dl=(this.plT[v]-pl)/n,dr=(this.prT[v]-pr)/n;
    if(env===1&&es===0&&fl===0&&pl+dl===pl&&pr+dr===pr)for(let s=0;s<n;s++){const x=vb[s];L[s]+=x*pl;R[s]+=x*pr}
    else for(let s=0;s<n;s++){
      if(es>0){env+=es;if(env>=1){env=1;es=0}}
      let g=env;if(fl>0){fl=Math.max(1e-9,fl-fstep);g*=fl<=1e-9?0:fl}
      pl+=dl;pr+=dr;const x=vb[s]*g;L[s]+=x*pl;R[s]+=x*pr;
    }
    this.pl[v]=pl;this.pr[v]=pr;this.env[v]=env;this.envStep[v]=es;this.e[v]=e;
    if(fl>0){
      this.fade[v]=fl;
      if(fl<=1e-9){const d=this.pend[v];this.on[v]=0;this.fade[v]=0;if(d){const o=this.pendAt[v];this.begin(v,d,o,this.offset(d,o));this.renderVoice(v,n,L,R)}}
    }else if(!exciting&&cN===0&&e<1e-11)this.on[v]=0;
  }
  process(inputs,outputs){
    const out=outputs[0],L=out[0],R=out[1]||out[0],n=L.length;
    if(!this.muted)this.frame=currentFrame;
    const end=this.frame+n;
    const q=this.queue;let starts=0;
    while(this.queueAt<q.length&&starts<this.maxStarts){
      const d=q[this.queueAt];
      if(this.queueOff>=d.length){this.queueAt++;this.queueOff=0;continue}
      if(d[this.queueOff]>=end)break;
      starts++;
      if(!this.start(d,this.queueOff)){
        if(this.waitFrom<0)this.waitFrom=this.frame;
        if(this.frame-this.waitFrom<this.maxWait)break;
        this.strikes++;this.drops++;
      }
      this.waitFrom=-1;this.queueOff+=strikeSize(d,this.queueOff);
    }
    if(this.queueAt===q.length){q.length=0;this.queueAt=0}
    if(this.damps.length){const keep=[];for(const m of this.damps){if(m.at>=end){keep.push(m);continue}
      for(let v=0;v<this.V;v++)if(this.on[v]&&this.key[v]===m.key&&this.fade[v]===0){this.fade[v]=1;this.fadeStep[v]=1/(sampleRate*.06);this.pend[v]=null}}
      this.damps=keep}
    const dL=this.dryL,dR=this.dryR;dL.fill(0);dR.fill(0);
    const inp=inputs[0];let inE=0;
    if(inp&&inp.length){const a=inp[0],b=inp[1]||a;for(let s=0;s<n;s++){dL[s]+=a[s];dR[s]+=b[s];inE+=a[s]*a[s]}}
    let voices=0;
    for(let v=0;v<this.V;v++)if(this.on[v]){voices++;this.renderVoice(v,n,dL,dR)}
    // The tail outlives its input by the predelay plus a full pass of the longest line,
    // so idling waits for that many consecutive silent blocks.
    const quiet=voices===0&&inE<1e-12&&this.space.energy<1e-14&&this.space.cur.freeze<.5;
    this.quietBlocks=quiet?this.quietBlocks+1:0;
    this.idle=this.quietBlocks>this.space.holdBlocks;
    if(!this.idle)this.space.process(dL,dR,L,R,n);
    this.frame=end;
    if(this.frame-this.statAt>=sampleRate*.25){this.statAt=this.frame;if(!this.idle||!this.idleReported)this.post({type:'stats',voices,steals:this.steals,drops:this.drops,strikes:this.strikes,idle:this.idle});this.idleReported=this.idle}
    return true;
  }
}
registerProcessor('cogs-engine',CogsEngine);
