/* ---------- wave synth: the voices, in their own processor on the engine's input ---------- */
// Tables are built additively: a spectrum per frame, an inverse FFT per mip level (each level halves the harmonics, so
// a note reads the level that stays under Nyquist). Modulation runs per 32-sample slice: sources are sampled at the
// slice start and every routed parameter is recomputed from its knob position plus the routes, as Serum does.
const WN=2048,WMIPS=10,WSLICE=32,WVOICES=24,WPOLY=16,WSTREAMS=64;
function fftInverse(re,im){
  const n=re.length;
  for(let i=1,j=0;i<n;i++){let bit=n>>1;for(;j&bit;bit>>=1)j^=bit;j^=bit;if(i<j){let t=re[i];re[i]=re[j];re[j]=t;t=im[i];im[i]=im[j];im[j]=t}}
  for(let len=2;len<=n;len<<=1){const ang=TAU/len,wr=Math.cos(ang),wi=Math.sin(ang),h=len>>1;
    for(let i=0;i<n;i+=len){let cr=1,ci=0;for(let j=0;j<h;j++){const a=i+j,b=a+h,xr=re[b]*cr-im[b]*ci,xi=re[b]*ci+im[b]*cr;
      re[b]=re[a]-xr;im[b]=im[a]-xi;re[a]+=xr;im[a]+=xi;const t=cr*wr-ci*wi;ci=cr*wi+ci*wr;cr=t}}}
}
function buildTable(spectra){
  const re=new Float64Array(WN),im=new Float64Array(WN),mips=Array.from({length:WMIPS},()=>[]);
  for(const a of spectra){
    let peak=0;const made=[];
    for(let m=0;m<WMIPS;m++){
      const H=Math.min(a.length,(WN/2-1)>>m);re.fill(0);im.fill(0);
      for(let k=1;k<=H;k++){const x=a[k-1];if(x){im[k]=-.5*x;im[WN-k]=.5*x}}
      fftInverse(re,im);
      const t=new Float32Array(WN+1);for(let i=0;i<WN;i++)t[i]=re[i];t[WN]=t[0];
      if(m===0)for(let i=0;i<WN;i++)peak=Math.max(peak,Math.abs(t[i]));
      made.push(t);
    }
    const g=peak>1e-9?1/peak:0;for(let m=0;m<WMIPS;m++){const t=made[m];for(let i=0;i<=WN;i++)t[i]*=g;mips[m].push(t)}
  }
  return {frames:spectra.length,mips};
}
const NP=WAVE_PARAMS.length,NS=WAVE_SOURCES.length,SRC_VEL=WAVE_SOURCES.indexOf('vel'),SRC_NOTE=WAVE_SOURCES.indexOf('note'),SRC_RAND=WAVE_SOURCES.indexOf('rand');
const PI_=(path)=>WAVE_INDEX.get(path);
const OSC_KEYS=['pos','oct','semi','fine','level','pan','uni','detune','blend','spread','phase','warp','warp2','a','d','s','r','cut','res','drive','key','famt','fa','fd','fs','fr'];
const OSC_P=Array.from({length:WAVE_OSCS},(_,i)=>Object.fromEntries(OSC_KEYS.map(k=>[k,PI_(`osc${i+1}.${k}`)])));
const ENV_P=Array.from({length:WAVE_ENVS},(_,i)=>['a','d','s','r'].map(k=>PI_(`env${i+1}.${k}`)));
const LFO_P=Array.from({length:WAVE_LFOS},(_,i)=>[PI_(`lfo${i+1}.rate`),PI_(`lfo${i+1}.phase`)]);
const MAC_P=Array.from({length:WAVE_MACROS},(_,i)=>PI_('m'+(i+1)));
const P_TOCT=PI_('tune.oct'),P_TSEMI=PI_('tune.semi'),P_SUB=PI_('sub.level'),P_SUBOCT=PI_('sub.oct'),P_NOISE=PI_('noise.level'),P_COLOR=PI_('noise.color'),P_VOL=PI_('vol'),P_GLIDE=PI_('glide'),P_GATE=PI_('gate');
const newEnv=()=>({st:0,lv:0});
// An envelope stepped a whole slice at once, in closed form: for those that only modulate, sampled once per slice.
function envSkip(e,c,s,n){
  switch(e.st){
    case 1:e.lv+=c[0]*n;if(e.lv>=1){e.lv=1;e.st=2}break;
    case 2:e.lv=s+(e.lv-s)*Math.pow(c[1],n);if(Math.abs(e.lv-s)<1e-5){e.lv=s;e.st=s>0?3:0}break;
    case 3:e.lv=s;break;
    case 4:e.lv*=Math.pow(c[2],n);if(e.lv<1e-5){e.lv=0;e.st=0}break;
  }
  return e.lv;
}
// Headroom a phase warp needs above the note, for picking a mip level that stays under Nyquist.
const warpRoom=(m,w)=>m>=1&&m<=6?1+Math.abs(w)*(m===2?7:3):1;
// Phase warps: where in the table to read for phase q; -1 is silence (pwm's closed part). m is the modulator sine.
function warpQ(mode,q,w,m){
  if(!w)return q;const a=Math.abs(w);
  switch(mode){
    case 1:return w>0?Math.pow(q,1+3*w):1-Math.pow(1-q,1-3*w);
    case 2:return (q*(1+a*7))%1;
    case 3:{const d=1-.95*a;return q<d?q/d:-1}
    case 4:{const d=.5+.45*w;return q<d?q*.5/d:.5+(q-d)*.5/(1-d)}
    case 5:return q+a*((q<.5?q:1-q)-q);
    case 6:{const r=(q+a*1.5*m)%1;return r<0?r+1:r}
    default:return q;
  }
}
// Amplitude warps on the value read
function warpY(mode,y,q,w,m){
  const a=Math.abs(w);if(!a)return y;
  switch(mode){
    case 7:return q>1-a*.5?-y:y;
    case 8:{const L=Math.pow(2,1+(1-a)*7);return Math.round(y*L)/L}
    case 9:return y*(1-a*(.5-.5*m));
    case 10:return y*(1-a+a*m);
    default:return y;
  }
}
// ADSR: linear attack, exponential decay and release, each reaching ~1% in its time.
function envCoefs(c,a,d,r,fs){c[0]=1/(Math.max(a,.0005)*fs);c[1]=Math.exp(-4.6/(Math.max(d,.001)*fs));c[2]=Math.exp(-4.6/(Math.max(r,.001)*fs))}
function envTick(e,c,s){
  switch(e.st){
    case 1:e.lv+=c[0];if(e.lv>=1){e.lv=1;e.st=2}break;
    case 2:e.lv=s+(e.lv-s)*c[1];if(Math.abs(e.lv-s)<1e-5){e.lv=s;e.st=s>0?3:0}break;
    case 3:e.lv=s;break;
    case 4:e.lv*=c[2];if(e.lv<1e-5){e.lv=0;e.st=0}break;
  }
  return e.lv;
}
function lfoAt(shape,ph,l){
  switch(shape){
    case 1:return ph<.5?ph*2:2-ph*2;
    case 2:return ph;
    case 3:return 1-ph;
    case 4:return ph<.5?1:0;
    case 5:return l.held;
    case 6:{const u=ph*ph*(3-2*ph);return l.prev+(l.held-l.prev)*u}
    default:return .5-.5*Math.cos(TAU*ph);
  }
}

/* the FX rack: hyper/dimension, distortion, flanger, phaser, chorus, delay, compressor, EQ, run in the patch's order on its summed voices */
const FX_IDX=WAVE_PARAMS.map((P,i)=>WAVE_FX_RE.test(P[0])?i:-1).filter(i=>i>=0);
const FXI=Object.fromEntries(FX_IDX.map(i=>[WAVE_PARAMS[i][0],i]));
const MAC0=WAVE_ENVS+WAVE_LFOS;
function newFxState(fs){
  // the delay's 2 s lines are made the first time a patch turns its delay on
  return {held:[0,0],hc:0,cb:[new Float32Array(4096),new Float32Array(4096)],cw:0,cph:0,db:null,dw:0,dlp:[0,0],
    eq:[[0,0,0,0],[0,0,0,0],[0,0,0,0],[0,0,0,0]],eqKey:'',eqc:null,env:0,
    hb:[new Float32Array(4096),new Float32Array(4096)],hw:0,hph:new Float64Array(7),fb_:[new Float32Array(1024),new Float32Array(1024)],fw:0,fph:0,fz:[0,0],
    ap:new Float64Array(12),pph:0,pz:[0,0],pa:[0,0]};
}
const fxTail=cp=>cp.fxOn.delay?2+12*cp.val[FXI['delay.fb']]:Object.values(cp.fxOn).some(Boolean)?.3:0;
// RBJ shelving biquad coefficients
function shelf(f,db,fs,high){const A=Math.pow(10,db/40),w=TAU*f/fs,cs=Math.cos(w),sn=Math.sin(w),al=sn/2*Math.SQRT2,sq=2*Math.sqrt(A)*al;
  let b0,b1,b2,a0,a1,a2;
  if(high){b0=A*((A+1)+(A-1)*cs+sq);b1=-2*A*((A-1)+(A+1)*cs);b2=A*((A+1)+(A-1)*cs-sq);a0=(A+1)-(A-1)*cs+sq;a1=2*((A-1)-(A+1)*cs);a2=(A+1)-(A-1)*cs-sq}
  else{b0=A*((A+1)-(A-1)*cs+sq);b1=2*A*((A-1)-(A+1)*cs);b2=A*((A+1)-(A-1)*cs-sq);a0=(A+1)+(A-1)*cs+sq;a1=-2*((A-1)+(A+1)*cs);a2=(A+1)+(A-1)*cs-sq}
  return [b0/a0,b1/a0,b2/a0,a1/a0,a2/a0];
}
function biquad(c,z,x){const y=c[0]*x+c[1]*z[0]+c[2]*z[1]-c[3]*z[2]-c[4]*z[3];z[1]=z[0];z[0]=x;z[3]=z[2];z[2]=y;return y}
const FX_RUN={
  dist(cp,v,st,L,R,n,bpm,fs){const dr=v[FXI['dist.drive']],mix=v[FXI['dist.mix']],g=1+dr*30,inv=1/Math.sqrt(g),mode=cp.distMode,q=Math.pow(2,16-dr*14),hold=1+Math.floor(dr*16);
    for(let s=0;s<n;s++)for(let c=0;c<2;c++){const B=c?R:L,x=B[s];let y;
      if(mode===0)y=Math.tanh(x*g)*inv;else if(mode===1)y=Math.max(-1,Math.min(1,x*g))*inv;else if(mode===2)y=Math.sin(x*g)*inv;
      else{if(c===0&&++st.hc>=hold)st.hc=0;if(st.hc===0)st.held[c]=Math.round(x*q)/q;y=st.held[c]}
      B[s]=x+(y-x)*mix}},
  chorus(cp,v,st,L,R,n,bpm,fs){const rate=v[FXI['chorus.rate']],dep=v[FXI['chorus.depth']]*.006*fs,mix=v[FXI['chorus.mix']],base=.012*fs,M=4095;
    for(let s=0;s<n;s++){st.cph=(st.cph+rate/fs)%1;
      for(let c=0;c<2;c++){const B=c?R:L,b=st.cb[c],x=B[s];b[st.cw&M]=x;const d=base+dep*(.5+.5*Math.sin(TAU*(st.cph+c*.25))),p=st.cw-d,i=Math.floor(p),f=p-i,w=b[i&M]+(b[(i+1)&M]-b[i&M])*f;
        B[s]=x*(1-mix*.5)+w*mix}
      st.cw++}},
  delay(cp,v,st,L,R,n,bpm,fs){if(!st.db){const N=Math.ceil(fs*2.1);st.db=[new Float32Array(N),new Float32Array(N)]}const D=st.db[0].length,T=Math.min(D-2,Math.max(1,(cp.delaySync?60/bpm*cp.delaySync:v[FXI['delay.time']])*fs)),fb=v[FXI['delay.fb']],mix=v[FXI['delay.mix']],
      tc=1-Math.exp(-TAU*Math.min(.45*fs,400*Math.pow(2,v[FXI['delay.tone']]*5.5))/fs),bl=st.db[0],br=st.db[1],ping=cp.ping;
    for(let s=0;s<n;s++){let p=st.dw-T;if(p<0)p+=D;const i=Math.floor(p),f=p-i,j=(i+1)%D,dl=bl[i]+(bl[j]-bl[i])*f,dr=br[i]+(br[j]-br[i])*f;
      st.dlp[0]+=(dl-st.dlp[0])*tc;st.dlp[1]+=(dr-st.dlp[1])*tc;const xl=L[s],xr=R[s];
      if(ping){bl[st.dw]=(xl+xr)*.5+fb*st.dlp[1];br[st.dw]=fb*st.dlp[0]}else{bl[st.dw]=xl+fb*st.dlp[0];br[st.dw]=xr+fb*st.dlp[1]}
      L[s]=xl+dl*mix;R[s]=xr+dr*mix;st.dw=(st.dw+1)%D}},
  eq(cp,v,st,L,R,n,bpm,fs){const lo=v[FXI['eq.low']],hi=v[FXI['eq.high']],key=lo.toFixed(2)+'/'+hi.toFixed(2);
    if(key!==st.eqKey){st.eqKey=key;st.eqc=[shelf(200,lo,fs,false),shelf(4000,hi,fs,true)]}
    for(let s=0;s<n;s++){L[s]=biquad(st.eqc[1],st.eq[2],biquad(st.eqc[0],st.eq[0],L[s]));R[s]=biquad(st.eqc[1],st.eq[3],biquad(st.eqc[0],st.eq[1],R[s]))}},
  comp(cp,v,st,L,R,n,bpm,fs){const th=v[FXI['comp.thresh']],ra=v[FXI['comp.ratio']],mk=Math.pow(10,v[FXI['comp.gain']]/20),at=Math.exp(-1/(.005*fs)),re=Math.exp(-1/(.12*fs));
    let e=st.env;for(let s=0;s<n;s++){const x=Math.max(Math.abs(L[s]),Math.abs(R[s]));e=x>e?x+(e-x)*at:x+(e-x)*re;
      const db=20*Math.log10(e+1e-9),over=db-th,g=(over>0?Math.pow(10,-over*(1-1/ra)/20):1)*mk;L[s]*=g;R[s]*=g}st.env=e},
  // Hyper: up to 7 micro-delay voices drifting sharp and flat; Dimension: cross-fed short delays for width
  hyper(cp,v,st,L,R,n,bpm,fs){const nv=Math.max(1,Math.round(v[FXI['hyper.voices']])),rate=v[FXI['hyper.rate']],dep=v[FXI['hyper.detune']]*.004*fs,mix=v[FXI['hyper.mix']],dim=v[FXI['hyper.dim']],M=4095,b=st.hb,g=1/Math.sqrt(nv);
    for(let s=0;s<n;s++){const xl=L[s],xr=R[s];b[0][st.hw&M]=xl;b[1][st.hw&M]=xr;let wl=0,wr=0;
      for(let j=0;j<nv;j++){st.hph[j]=(st.hph[j]+rate*(1+j*.13)/fs)%1;const d=(.006+.0025*j)*fs+dep*(.5+.5*Math.sin(TAU*(st.hph[j]+j/nv))),c=j&1,p=st.hw-d,i=Math.floor(p),f=p-i,B=b[c],w=B[i&M]+(B[(i+1)&M]-B[i&M])*f;
        if(c)wr+=w;else wl+=w}
      // wet voices added over the dry, not crossfaded: a short-delay crossfade would comb-cancel the dry
      let ol=xl*(1-mix*.3)+wl*g*mix,or=xr*(1-mix*.3)+wr*g*mix;
      if(dim>0){const il=Math.floor(st.hw-(.011+.012*dim)*fs),ir=Math.floor(st.hw-(.017+.015*dim)*fs);ol+=dim*.45*b[1][il&M];or+=dim*.45*b[0][ir&M]}
      L[s]=ol;R[s]=or;st.hw++}},
  // Flanger: a 0.3-6 ms delay swept by an LFO, fed back
  flanger(cp,v,st,L,R,n,bpm,fs){const rate=v[FXI['flanger.rate']],dep=v[FXI['flanger.depth']],fb=v[FXI['flanger.fb']],mix=v[FXI['flanger.mix']],sp=v[FXI['flanger.spread']],M=1023,b=st.fb_;
    for(let s=0;s<n;s++){st.fph=(st.fph+rate/fs)%1;
      for(let c=0;c<2;c++){const B=c?R:L,x=B[s],d=(.0003+.0057*dep*(.5+.5*Math.sin(TAU*(st.fph+c*sp*.5))))*fs,p=st.fw-d,i=Math.floor(p),f=p-i,bb=b[c],w=bb[i&M]+(bb[(i+1)&M]-bb[i&M])*f;
        bb[st.fw&M]=x+fb*w;B[s]=x*(1-mix*.5)+w*mix}
      st.fw++}},
  // Phaser: six allpass stages per side swept around a centre frequency, with feedback and a stereo phase offset
  phaser(cp,v,st,L,R,n,bpm,fs){const rate=v[FXI['phaser.rate']],dep=v[FXI['phaser.depth']],f0=v[FXI['phaser.freq']],fb=v[FXI['phaser.fb']],mix=v[FXI['phaser.mix']],sp=v[FXI['phaser.spread']],ap=st.ap;
    for(let s=0;s<n;s++){st.pph=(st.pph+rate/fs)%1;
      if((s&7)===0)for(let c=0;c<2;c++){const fc=Math.min(.45*fs,f0*Math.pow(2,dep*3*Math.sin(TAU*(st.pph+c*sp*.5)))),t=Math.tan(Math.PI*fc/fs);st.pa[c]=(t-1)/(t+1)}
      for(let c=0;c<2;c++){const B=c?R:L,x=B[s],a=st.pa[c];let y=x+fb*st.pz[c];
        for(let k=0;k<6;k++){const z=c*6+k,o=a*y+ap[z];ap[z]=y-a*o;y=o}
        st.pz[c]=y;B[s]=x*(1-mix*.5)+y*mix}}},
};
// A motion's value at a frame: its table (64 points, wrapping) at phase ph0 + rate·(frame − f0)/sampleRate, linear or
// stepped, mapped into lo..hi. One-pole smoothing (about 3 ms) takes the edges off a stepped shape.
const MOTION_GLIDE=1-Math.exp(-WSLICE/(.003*sampleRate));
function motionAt(p,frame){
  const t=p.table,N=t.length,ph=p.ph0+p.rate*(frame-p.f0)/sampleRate,u=(ph-Math.floor(ph))*N,i=Math.floor(u)%N;
  const y=p.step?t[i]:t[i]+(t[(i+1)%N]-t[i])*(u-Math.floor(u));
  return p.lo+(p.hi-p.lo)*y;
}
function runFx(cp,L,R,n,bpm){
  const fs=sampleRate,v=cp.fxv,st=cp.fxs;
  // FX knobs take the macros' routes (a rack is shared by every voice, so per-note sources do not reach it)
  for(const i of FX_IDX)v[i]=cp.base[i];
  for(const[s,d,amt,bi,ax,ai,cv]of cp.routes)if(s>=MAC0&&s<MAC0+WAVE_MACROS&&FXI[WAVE_PARAMS[d][0]]===d){const x=modCurve(cp.mac[s-MAC0],cv);
    const a=ax>=MAC0&&ax<MAC0+WAVE_MACROS?cp.mac[ax-MAC0]:1;v[d]+=(bi?x*2-1:x)*amt*(ax<0?1:ai?1-a:a)}
  for(const i of FX_IDX)v[i]=waveDenorm(WAVE_PARAMS[i],v[i]);
  for(const k of cp.fxOrder)if(cp.fxOn[k])FX_RUN[k](cp,v,st,L,R,n,bpm,fs);
}
class WaveSynth extends AudioWorkletProcessor{
  constructor(){
    super();
    this.tables={};for(const n of WAVE_TABLES)if(n!=='harmonics')this.tables[n]=buildTable(waveSpectra(n));
    this.patches=new Map();this.bpm=120;this.streams=0;this.uniScale=1;this.bws=new Float64Array(8);this.queue=[];this.offs=[];this.motions=new Map();this.frame=0;this.statAt=0;this.ver=0;
    this.voices=Array.from({length:WVOICES},()=>this.voice());
    this.mixL=new Float32Array(WSLICE);this.mixR=new Float32Array(WSLICE);this.ol=new Float32Array(WSLICE);this.or=new Float32Array(WSLICE);
    this.port.onmessage=e=>{const m=e.data;
      if(m.type==='patch')this.setPatch(m.id,m.p);else if(m.type==='note')this.queue.push(m);else if(m.type==='off')this.offs.push(m);
      else if(m.type==='motion')this.motion(m);
      else if(m.type==='tempo')this.bpm=m.bpm>0?m.bpm:120;
      else if(m.type==='panic')for(const v of this.voices)v.on=false};
  }
  voice(){
    const osc=()=>({ph:new Float64Array(8),env:newEnv(),fenv:newEnv(),ec:new Float64Array(3),fc:new Float64Array(3),z:new Float64Array(8)});
    return {on:false,cp:null,ver:-1,vals:new Float64Array(NP),src:new Float64Array(NS),osc:Array.from({length:WAVE_OSCS},osc),
      envs:Array.from({length:WAVE_ENVS},newEnv),ecs:Array.from({length:WAVE_ENVS},()=>new Float64Array(3)),
      lfo:Array.from({length:WAVE_LFOS},()=>({ph:0,held:.5,prev:.5})),subPh:0,nz:0,dl:0,dr:0,kill:0,age:0,released:false};
  }
  // A patch arrives whole; it is compiled once: knob positions normalised, routes packed, and a table for each oscillator.
  setPatch(id,p){
    const old=this.patches.get(id),base=new Float64Array(NP),val=new Float64Array(NP);
    for(let i=0;i<NP;i++){const P=WAVE_PARAMS[i];let x=+waveGet(p,P[0]);if(!Number.isFinite(x))x=P[4];base[i]=Math.min(1,Math.max(0,waveNorm(P,x)));val[i]=waveDenorm(P,base[i])}
    const routes=[],modded=new Set();
    for(const m of p.mods||[]){const s=WAVE_SOURCES.indexOf(m.src),d=WAVE_INDEX.get(m.dst);if(s<0||d===undefined||!m.amt||m.off)continue;routes.push([s,d,+m.amt,m.bi?1:0,WAVE_SOURCES.indexOf(m.aux),m.auxInv?1:0,+m.curve||0]);modded.add(d)}
    const tabs=p.osc.map((o,i)=>o.table==='harmonics'?(old&&old.harmKey[i]===JSON.stringify(o.harm)?old.tabs[i]:buildTable(waveSpectra('harmonics',o.harm))):this.tables[o.table]||this.tables.basic);
    this.patches.set(id,{id,ver:++this.ver,base,val,routes,modded:[...modded],tabs,harmKey:p.osc.map(o=>JSON.stringify(o.harm)),
      on:p.osc.map(o=>!!o.on),warpMode:p.osc.map(o=>Math.max(0,WARP_MODES.indexOf(o.warpMode??'bend'))),warpMode2:p.osc.map(o=>Math.max(0,WARP_MODES.indexOf(o.warp2Mode??'off'))),ftype:p.osc.map(o=>Math.max(0,WAVE_FILTERS.indexOf(o.filter))),
      lfoShape:p.lfo.map(l=>Math.max(0,LFO_SHAPES.indexOf(l.shape))),lfoRetrig:p.lfo.map(l=>l.retrig!==false),lfoSync:p.lfo.map(l=>LFO_SYNCS[l.sync]||0),mode:Math.max(0,VOICE_MODES.indexOf(p.voicing?.mode)),always:p.voicing?.always!==false,glideScale:!!p.voicing?.scale,last:old?old.last:null,
      fxOn:Object.fromEntries(WAVE_FX.map(k=>[k,!!(p.fx&&p.fx[k]&&p.fx[k].on)])),fxOrder:waveFxOrder(p.fxOrder),distMode:Math.max(0,DIST_MODES.indexOf(p.fx?.dist?.mode)),delaySync:DELAY_SYNCS[p.fx?.delay?.sync]||0,ping:p.fx?.delay?.ping!==false,
      busL:old?old.busL:new Float32Array(128),busR:old?old.busR:new Float32Array(128),fxs:old?old.fxs:newFxState(sampleRate),fxv:new Float64Array(NP),quiet:old?old.quiet:1e9,live:false});
    // the macros as they play: the knobs' values, or where a motion part (a cam) has turned them
    const cp=this.patches.get(id);cp.mac=Float64Array.from(MAC_P,i=>cp.base[i]);
    for(let s=0;s<WAVE_MACROS;s++){const mo=this.motions.get(id*8+s);if(mo)cp.mac[s]=mo.y}
    for(const v of this.voices)if(v.on&&v.cp&&v.cp.id===id)v.cp=cp;
  }
  // A motion: a shape read at a phase that moves linearly with the frame, as a cam turns with its cog. It is posted
  // once, when the train or the part changes, and evaluated here every slice, on the notes' own clock.
  motion(m){
    const k=m.id*8+m.slot;
    if(!m.prog){this.motions.delete(k);const cp=this.patches.get(m.id);if(cp)cp.mac[m.slot]=cp.base[MAC_P[m.slot]];return}
    const old=this.motions.get(k);this.motions.set(k,{...m.prog,id:m.id,slot:m.slot,y:old?old.y:motionAt(m.prog,this.frame)});
  }
  runMotions(frame){
    for(const mo of this.motions.values()){const cp=this.patches.get(mo.id);if(!cp)continue;
      const x=motionAt(mo,frame);mo.y+=(x-mo.y)*MOTION_GLIDE;cp.mac[mo.slot]=mo.y}
  }
  start(m){
    const cp=this.patches.get(m.id);if(!cp)return;
    let live=0,free=null,oldest=null,mine=null,held=false;
    for(const v of this.voices){if(!v.on){free??=v;continue}if(v.kill)continue;live++;if(!oldest||v.age<oldest.age)oldest=v;
      if(v.cp.id===cp.id){if(!mine||v.age>mine.age)mine=v;if(!v.released)held=true}}
    const gl=cp.val[P_GLIDE];
    // mono and legato: one voice per patch, gliding to the new note; mono restarts the envelopes from where they are,
    // legato only when the last note had already let go
    if(cp.mode&&mine){
      for(const v of this.voices)if(v!==mine&&v.on&&v.cp.id===cp.id)v.kill=1;
      const v=mine,from=gl>0&&(cp.always||held)?this.pitch(v,m.at):m.midi;
      v.from=from;v.midi=m.midi;v.t0=m.at;v.key=m.key;v.age=this.frame;v.amp=m.amp;v.vel=Math.min(1,Math.max(0,m.vel??m.amp));v.glideT=this.glideTime(cp,from,m.midi);
      v.offAt=m.at+Math.max(m.hold||0,cp.val[P_GATE])*sampleRate;cp.last=m.midi;
      if(cp.mode===1||!held){for(const o of v.osc){o.env.st=1;o.fenv.st=1}for(const e of v.envs)e.st=1}
      v.released=false;return;
    }
    if(live>=WPOLY&&oldest)oldest.kill=1;
    const v=free||oldest;if(!v)return;
    v.on=true;v.kill=0;v.cp=cp;v.ver=-1;v.age=this.frame;v.key=m.key;v.midi=m.midi;v.from=gl>0&&(cp.always||held)&&cp.last!=null?cp.last:m.midi;v.glideT=this.glideTime(cp,v.from,m.midi);cp.last=m.midi;
    v.amp=m.amp;v.pan=m.pan||0;v.dark=m.dark||0;v.vel=Math.min(1,Math.max(0,m.vel??m.amp));v.rand=Math.random();
    v.t0=m.at;v.offAt=m.at+Math.max(m.hold||0,cp.val[P_GATE])*sampleRate;v.released=false;v.dl=v.dr=0;
    v.vals.set(cp.val);
    for(let i=0;i<WAVE_OSCS;i++){const o=v.osc[i];o.env.st=1;o.env.lv=0;o.fenv.st=1;o.fenv.lv=0;o.z.fill(0);
      const ph=cp.val[OSC_P[i].phase];for(let j=0;j<8;j++)o.ph[j]=ph+(j?Math.random():0)}
    for(const e of v.envs){e.st=1;e.lv=0}
    for(let i=0;i<WAVE_LFOS;i++){const l=v.lfo[i];l.ph=0;l.held=Math.random();l.prev=Math.random()}
    v.subPh=0;
  }
  // the voice's pitch now, partway along its glide
  pitch(v,at){const T=v.glideT||0,tt=(at-v.t0)/sampleRate;return T>0?v.midi+(v.from-v.midi)*Math.exp(-tt/(T/4.6)):v.midi}
  // glide time; scaled, it is per octave of the interval, so short steps glide faster
  glideTime(cp,a,b){const g=cp.val[P_GLIDE];return cp.glideScale?g*Math.abs(a-b)/12:g}
  release(v){if(v.released)return;v.released=true;for(const o of v.osc){if(o.env.st)o.env.st=4;if(o.fenv.st)o.fenv.st=4}for(const e of v.envs)if(e.st)e.st=4}
  // One voice for one slice: sources, routed values, then the oscillators through their filters.
  lfoRate(cp,vals,i){const b=cp.lfoSync[i];return b?this.bpm/60/b:vals[LFO_P[i][0]]}
  // One oscillator of one voice for one slice, into the voice's mix; false once its envelope has finished.
  osc(v,cp,vals,i,n,midi,fs){
    const ol=this.ol,or=this.or;
const P=OSC_P[i],o=v.osc[i],level=vals[P.level];
      envCoefs(o.ec,vals[P.a],vals[P.d],vals[P.r],fs);envCoefs(o.fc,vals[P.fa],vals[P.fd],vals[P.fr],fs);
      if(o.env.st===0)return false;
      const sus=vals[P.s];
      // the filter envelope only moves the cutoff, which is set once per slice: it steps a slice at a time
      const fenv=o.fenv.lv;envSkip(o.fenv,o.fc,vals[P.fs],n);
      // a silent oscillator (level at 0, as B and C start) costs only its envelope
      if(level<1e-4){envSkip(o.env,o.ec,sus,n);return true}
      const f=440*Math.pow(2,(midi+12*vals[P.oct]+vals[P.semi]+vals[P.fine]/100-69)/12);
      // unison under a budget: past WSTREAMS table reads per sample across all voices, every oscillator's unison thins
      const want=Math.max(1,Math.min(8,Math.round(vals[P.uni])));this.streams+=want;
      const nu=this.uniScale<1?Math.max(1,Math.round(want*this.uniScale)):want,span=vals[P.detune]*vals[P.detune]*1.2,tab=cp.tabs[i];
      const wm=cp.warpMode[i],wm2=cp.warpMode2[i],warp=vals[P.warp],w2=vals[P.warp2];
      const fmax=f*Math.pow(2,span/24)*warpRoom(wm,warp)*warpRoom(wm2,w2),lim=.45*fs/Math.max(1,fmax);
      let mip=0;while(mip<WMIPS-1&&(1023>>mip)>lim)mip++;
      const fp=vals[P.pos]*(tab.frames-1),f0=Math.floor(fp),ff=fp-f0,f1=Math.min(f0+1,tab.frames-1),A=tab.mips[mip][f0],B=tab.mips[mip][f1];
      const norm=1/Math.sqrt(nu),pan0=vals[P.pan],spread=vals[P.spread],blend=vals[P.blend];
      // the filter: cutoff from the knob, the key and the filter envelope (±6 octaves), set once per slice; a low-pass
      // left wide open (above 18 kHz, little resonance) is skipped
      let ft=cp.ftype[i];
      let cut=vals[P.cut]*Math.pow(2,vals[P.key]*(midi-60)/12+vals[P.famt]*6*fenv);cut=Math.min(fs*.45,Math.max(20,cut));
      if((ft===1||ft===2)&&cut>=18000&&vals[P.res]<.2)ft=0;
      const g=Math.tan(Math.PI*cut/fs),k=2-1.96*vals[P.res],a1=1/(1+g*(g+k)),a2=g*a1,a3=g*a2,drive=1+vals[P.drive]*8,dn=1/Math.sqrt(drive);
      const z=o.z;
      // one voice of unison is mono: read, drive and filter one channel, pan at the end
      const mono=nu===1,plain=(wm===0||wm===1&&!warp)&&(wm2===0||wm2===1&&!w2);
      ol.fill(0,0,n);if(!mono)or.fill(0,0,n);
      const bws=this.bws;let sw=0;for(let j=0;j<nu;j++){bws[j]=nu>1?Math.max(0,1+(blend-.5)*2*(Math.abs(j/(nu-1)-.5)*4-1)):1;sw+=bws[j]*bws[j]}
      const bn=sw>1e-9?Math.sqrt(nu/sw):1;if(sw<=1e-9)bws.fill(1);
      const src=OSC_P[i?0:1];
      const minc=(wm>=6&&wm!==7&&wm!==8||wm2>=6&&wm2!==7&&wm2!==8)?440*Math.pow(2,(midi+12*vals[src.oct]+vals[src.semi]+vals[src.fine]/100-69)/12)/fs:0;let mp=o.mph||0;const mp0=mp;
      let mgl=0,mgr=0;
      for(let j=0;j<nu;j++){mp=mp0;
        const off=nu>1?(j/(nu-1)-.5)*span:0,inc=f*Math.pow(2,off/12)/fs,p=Math.max(-1,Math.min(1,pan0+(nu>1?spread*(j/(nu-1)*2-1):0))),ang=(p+1)*Math.PI/4;
        const bw=bws[j]*bn,gl_=Math.cos(ang)*norm*level*Math.SQRT2*bw,gr_=Math.sin(ang)*norm*level*Math.SQRT2*bw;let ph=o.ph[j];
        if(mono){mgl=gl_;mgr=gr_}
        const wl=mono?1:gl_;
        if(plain)for(let s=0;s<n;s++){
          const x=ph*WN,xi=x|0,xf=x-xi,a=A[xi]+(A[xi+1]-A[xi])*xf,y=a+(B[xi]+(B[xi+1]-B[xi])*xf-a)*ff;
          ol[s]+=y*wl;if(!mono)or[s]+=y*gr_;ph+=inc;if(ph>=1)ph-=1;
        }
        else for(let s=0;s<n;s++){
          const m=minc?Math.sin(TAU*mp):0;let q=warpQ(wm,ph,warp,m),y=0;if(q>=0&&wm2)q=warpQ(wm2,q,w2,m);
          if(q>=0){const x=q*WN,xi=x|0,xf=x-xi,a=A[xi]+(A[xi+1]-A[xi])*xf,b=B[xi]+(B[xi+1]-B[xi])*xf;y=a+(b-a)*ff;
            if(wm>=7)y=warpY(wm,y,q,warp,m);if(wm2>=7)y=warpY(wm2,y,q,w2,m)}
          ol[s]+=y*wl;if(!mono)or[s]+=y*gr_;ph+=inc;if(ph>=1)ph-=1;mp+=minc;if(mp>=1)mp-=1;
        }
        o.ph[j]=ph;
      }
      o.mph=mp;
      // amp envelope, drive and filter per sample, in place, then into the voice's mix
      // the amp envelope is stepped per slice in closed form and ramped linearly across it (32 samples, 0.7 ms)
      const chans=mono?1:2,e0=o.env.lv,e1=envSkip(o.env,o.ec,sus,n),de=(e1-e0)/n;
      for(let s=0;s<n;s++){
        const e=e0+de*(s+1);
        for(let c=0;c<chans;c++){
          let x=c?or[s]:ol[s];
          if(drive>1)x=Math.tanh(x*drive)*dn;
          if(ft){
            const zb=c*4;
            let v3=x-z[zb+1],v1=a1*z[zb]+a2*v3,v2=z[zb+1]+a2*z[zb]+a3*v3;z[zb]=2*v1-z[zb];z[zb+1]=2*v2-z[zb+1];
            const lp=v2,bp=v1,hp=x-k*v1-v2;
            if(ft===2||ft===4){const x2=ft===2?lp:hp;v3=x2-z[zb+3];const u1=a1*z[zb+2]+a2*v3,u2=z[zb+3]+a2*z[zb+2]+a3*v3;z[zb+2]=2*u1-z[zb+2];z[zb+3]=2*u2-z[zb+3];x=ft===2?u2:x2-k*u1-u2}
            else x=ft===1?lp:ft===3?hp:ft===5?bp*k:ft===6?lp+hp:lp-hp;
          }
          if(c)or[s]=x*e;else ol[s]=x*e;
        }
      }
      if(mono)for(let s=0;s<n;s++){this.mixL[s]+=ol[s]*mgl;this.mixR[s]+=ol[s]*mgr}
      else for(let s=0;s<n;s++){this.mixL[s]+=ol[s];this.mixR[s]+=or[s]}
          return true;
  }
  render(v,n,L,R,at){
    const cp=v.cp,vals=v.vals,src=v.src,fs=sampleRate,now=this.frame+at;
    if(!v.released&&now>=v.offAt)this.release(v);
    if(v.ver!==cp.ver){vals.set(cp.val);v.ver=cp.ver}
    for(let i=0;i<WAVE_ENVS;i++)src[i]=v.envs[i].lv;
    for(let i=0;i<WAVE_LFOS;i++){const l=v.lfo[i];let ph=l.ph;if(!cp.lfoRetrig[i])ph=now/fs*this.lfoRate(cp,vals,i);ph=(ph+vals[LFO_P[i][1]])%1;src[WAVE_ENVS+i]=lfoAt(cp.lfoShape[i],ph,l)}
    for(let i=0;i<WAVE_MACROS;i++)src[WAVE_ENVS+WAVE_LFOS+i]=cp.mac[i];
    src[SRC_VEL]=v.vel;src[SRC_NOTE]=Math.min(1,Math.max(0,(v.midi-24)/84));src[SRC_RAND]=v.rand;
    if(cp.routes.length){
      for(const d of cp.modded)vals[d]=cp.base[d];
      // a route: its source through the curve, bipolar or not, times the depth, times the aux source (or its inverse)
      for(const[s,d,amt,bi,ax,ai,cv]of cp.routes){const x=modCurve(src[s],cv);vals[d]+=(bi?x*2-1:x)*amt*(ax<0?1:ai?1-src[ax]:src[ax])}
      for(const d of cp.modded)vals[d]=waveDenorm(WAVE_PARAMS[d],vals[d]);
    }
    for(let i=0;i<WAVE_LFOS;i++){const l=v.lfo[i],nph=l.ph+this.lfoRate(cp,vals,i)*n/fs;if(nph>=1){l.prev=l.held;l.held=Math.random()}l.ph=nph%1}
    // the patch transposes itself: every note into it, from any part, sounds that far up or down
    const midi=this.pitch(v,now)+12*vals[P_TOCT]+vals[P_TSEMI];
    let alive=false;
    for(let i=0;i<WAVE_OSCS;i++)if(cp.on[i]&&this.osc(v,cp,vals,i,n,midi,fs))alive=true;
    // sub and noise ride ENV1, sample by sample; the other envelopes only modulate, so they step a slice at a time
    const subL=vals[P_SUB],nzL=vals[P_NOISE],e1=v.envs[0];
    for(let i=0;i<WAVE_ENVS;i++){const E=ENV_P[i];envCoefs(v.ecs[i],vals[E[0]],vals[E[1]],vals[E[3]],fs)}
    for(let i=1;i<WAVE_ENVS;i++)envSkip(v.envs[i],v.ecs[i],vals[ENV_P[i][2]],n);
    const s1=vals[ENV_P[0][2]];let sp=v.subPh,nz=v.nz;
    if(subL>0||nzL>0){
      const sinc=440*Math.pow(2,(midi+12*vals[P_SUBOCT]-69)/12)/fs,ca=1-Math.exp(-TAU*Math.min(.45*fs,200*Math.pow(2,vals[P_COLOR]*7))/fs);
      const q0=e1.lv,q1=envSkip(e1,v.ecs[0],s1,n),dq=(q1-q0)/n;
      for(let s=0;s<n;s++){
        const e=q0+dq*(s+1);let y=0;if(subL>0){y+=Math.sin(TAU*sp)*subL;sp+=sinc;if(sp>=1)sp-=1}if(nzL>0){nz+=(Math.random()*2-1-nz)*ca;y+=nz*nzL*1.6}
        this.mixL[s]+=y*e;this.mixR[s]+=y*e;
      }
    }else envSkip(e1,v.ecs[0],s1,n);
    v.subPh=sp;v.nz=nz;
    if((subL>0||nzL>0)&&e1.st)alive=true;
    // the voice's level, the room's distance darkening and its place in the stereo field
    const amp=v.amp*vals[P_VOL]*.25,pa=(Math.max(-1,Math.min(1,v.pan))+1)*Math.PI/4,pl=Math.cos(pa)*Math.SQRT2*amp,pr=Math.sin(pa)*Math.SQRT2*amp;
    const dc=v.dark>.02?1-Math.exp(-TAU*16000*Math.exp(-v.dark*3.2)/fs):1;let dl=v.dl,dr=v.dr,kill=v.kill;
    for(let s=0;s<n;s++){
      let xl=this.mixL[s],xr=this.mixR[s];if(dc<1){dl+=(xl-dl)*dc;dr+=(xr-dr)*dc;xl=dl;xr=dr}
      let g=1;if(kill){kill=Math.max(0,kill-1/(.005*fs));g=kill}
      L[at+s]+=xl*pl*g;R[at+s]+=xr*pr*g;
    }
    v.dl=dl;v.dr=dr;
    if(v.kill){v.kill=kill;if(kill<=0)v.on=false}
    if(!alive)v.on=false;
  }
  process(inputs,outputs){
    const out=outputs[0],L=out[0],R=out[1]||out[0],n=L.length,end=currentFrame+n;this.frame=currentFrame;
    // last block's demand sets this block's unison budget
    {const per=this.streams/Math.max(1,n/WSLICE);this.uniScale=per>WSTREAMS?WSTREAMS/per:1;this.streams=0}
    for(let at=0;at<n;at+=WSLICE){
      const sl=Math.min(WSLICE,n-at),t=currentFrame+at+sl;
      if(this.motions.size)this.runMotions(currentFrame+at);
      if(this.queue.length){const keep=[];for(const m of this.queue){if(m.at<t)this.start(m);else keep.push(m)}this.queue=keep}
      if(this.offs.length){const keep=[];for(const m of this.offs){if(m.at<t){for(const v of this.voices)if(v.on&&v.key===m.key&&!v.released)this.release(v)}else keep.push(m)}this.offs=keep}
      for(const v of this.voices)if(v.on){const cp=v.cp;if(!cp.live){cp.live=true;cp.busL.fill(0);cp.busR.fill(0)}this.mixL.fill(0,0,sl);this.mixR.fill(0,0,sl);this.render(v,sl,cp.busL,cp.busR,at)}
    }
    // each patch's voices are summed on its bus and go through its FX rack; a rack with a tail (delay, chorus) keeps running
    for(const cp of this.patches.values()){
      if(cp.live)cp.quiet=0;else{if(cp.quiet>fxTail(cp)*sampleRate)continue;cp.quiet+=n;cp.busL.fill(0);cp.busR.fill(0)}
      runFx(cp,cp.busL,cp.busR,n,this.bpm);
      for(let s=0;s<n;s++){L[s]+=cp.busL[s];R[s]+=cp.busR[s]}
      cp.live=false;
    }
    if(end-this.statAt>=sampleRate*.25){this.statAt=end;let nv=0;for(const v of this.voices)nv+=v.on?1:0;this.port.postMessage({type:'stats',voices:nv})}
    return true;
  }
}
registerProcessor('cogs-wave',WaveSynth);
