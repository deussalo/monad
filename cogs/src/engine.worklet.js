const TAU=Math.PI*2,LN1000=6.907755278982137,MAXM=12,BLOCK=128;
// A strike: at, key, amp, contact, attack, pan, n, n freqs, n gains, n decays, then its field sends (count, then slot
// and amount for each) and its dry level (a field can take some of the dry away).
const FX_SLOTS=8;
const strikeSize=(d,o)=>{const e=o+7+3*d[o+6];return 9+3*d[o+6]+2*d[e]};
const packStrikes=list=>{const out=[];for(const m of list){out.push(m.at||0,m.k,m.amp,m.c,m.a,m.p,m.f.length,...m.f);
  for(let i=0;i<m.f.length;i++)out.push(m.g[i]);for(let i=0;i<m.f.length;i++)out.push(m.t[i]);
  const fx=m.fx||[];out.push(fx.length/2,...fx,m.dry??1)}return new Float64Array(out)};

class CogsEngine extends AudioWorkletProcessor{
  constructor(options){
    super();
    const o=(options&&options.processorOptions)||{};
    const V=this.V=o.voices||48;
    // the room: a VintageVerb-style reverb (verb.js); the engine mixes the dry signal itself
    this.space=new Verb(sampleRate,{...VERB_DEFAULT,...o.space});this.dry=o.space&&Number.isFinite(+o.space.dry)?+o.space.dry:1;
    this.on=new Uint8Array(V);this.key=new Float64Array(V);this.nm=new Uint8Array(V);
    this.zr=new Float64Array(V*MAXM);this.zi=new Float64Array(V*MAXM);this.kr=new Float64Array(V*MAXM);this.ki=new Float64Array(V*MAXM);this.xg=new Float64Array(V*MAXM);
    this.cN=new Int32Array(V);this.cLen=new Int32Array(V);
    this.env=new Float64Array(V);this.envStep=new Float64Array(V);this.fade=new Float64Array(V);this.fadeStep=new Float64Array(V);this.pend=new Array(V).fill(null);this.pendAt=new Int32Array(V);this.dl=new Int32Array(V);
    this.pl=new Float64Array(V);this.pr=new Float64Array(V);this.plT=new Float64Array(V);this.prT=new Float64Array(V);this.e=new Float64Array(V);
    this.exc=new Float64Array(BLOCK);this.live=new Int32Array(MAXM);this.vb=new Float64Array(BLOCK);
    this.dryL=new Float32Array(BLOCK);this.dryR=new Float32Array(BLOCK);
    // field sends: each voice's amount into each slot, its dry level, and a buffer per slot
    this.snd=new Float64Array(V*FX_SLOTS);this.hasSnd=new Uint8Array(V);this.vdry=new Float64Array(V).fill(1);
    this.auxL=Array.from({length:FX_SLOTS},()=>new Float32Array(BLOCK));this.auxR=Array.from({length:FX_SLOTS},()=>new Float32Array(BLOCK));
    this.vL=new Float32Array(BLOCK);this.vR=new Float32Array(BLOCK);this.auxUsed=new Uint8Array(FX_SLOTS);
    this.maxStarts=4*V;this.maxWait=8*BLOCK;
    this.reset(o);this.warm(o);
    this.damps=[];
    this.port.onmessage=ev=>{const m=ev.data;if(m.type==='strikes')this.queue.push(m.d);else if(m.type==='damp')this.damps.push(m);else if(m.type==='space'){this.space.set(m);if(Number.isFinite(+m.dry))this.dry=+m.dry}};
  }
  // V8 interprets code until it has run enough; cold, the first strike's blocks
  // took 4-8 ms against a 2.7 ms deadline and dropped out as clicks, and the first
  // shimmer blocks took up to 7 ms. Run every path silently at construction:
  // the reverb's paths (every color's rate, the ring and the nonlin taps, shimmer, freeze, a mode switch), full
  // polyphony, stealing and overload. Then reset the same objects in place: a Verb allocated after warm-up runs cold again.
  warm(o){
    const l=new Float32Array(BLOCK),r=new Float32Array(BLOCK),out=[[l,r]],V=this.V;
    const sp=this.space;this.muted=true;this.queue=[];
    const strike=(k,t)=>({k,f:[110,230,370,520,700],g:[1,.6,.4,.3,.2],t:[t,t*.75,t*.5,t*.4,t*.25],amp:.1,c:.0008,a:.004,p:.3});
    for(let i=0;i<400;i++){
      // Both sides of every branch: V8 deoptimises when a path it never saw runs.
      if(i%50===0)sp.clear({...VERB_DEFAULT,...o.space,wet:1,mode:[0,14,5,2][(i/50)&3],color:(i/50)%3,shimmer:i%100?0:1});
      if(i%100===70||i%100===90)sp.set({freeze:i%100===70?1:0});
      if(i===340)sp.set({mode:3,color:1});
      if(i%40===0)this.queue.push(packStrikes([strike(i%3,2)]));
      if(i===100)this.queue.push(packStrikes(Array.from({length:2*V+4},(_,k)=>strike(1000+k,.3))));
      l.fill(0);r.fill(0);this.process([],out);
    }
    this.reset(o);
  }
  reset(o){
    for(const a of [this.on,this.key,this.nm,this.zr,this.zi,this.kr,this.ki,this.xg,this.cN,this.cLen,this.env,this.envStep,this.fade,this.fadeStep,
      this.pl,this.pr,this.plT,this.prT,this.e,this.exc,this.live,this.vb,this.dryL,this.dryR,this.pendAt,this.dl,this.snd,this.hasSnd])a.fill(0);
    this.vdry.fill(1);
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
    const e=t+n,ns=d[e]|0,sb=v*FX_SLOTS;for(let k=0;k<FX_SLOTS;k++)this.snd[sb+k]=0;
    for(let i=0;i<ns;i++){const k=d[e+1+2*i]|0;if(k>=0&&k<FX_SLOTS)this.snd[sb+k]=d[e+2+2*i]}
    this.hasSnd[v]=ns>0?1:0;const dry=d[e+1+2*ns];this.vdry[v]=Number.isFinite(dry)?dry:1;
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
    let voices=0;const aL=this.auxL,aR=this.auxR,used=this.auxUsed;used.fill(0);
    for(let v=0;v<this.V;v++)if(this.on[v]){voices++;
      // a voice in a field renders on its own, then goes to the dry mix at its dry level and to each field it is in
      if(!this.hasSnd[v]&&this.vdry[v]===1){this.renderVoice(v,n,dL,dR);continue}
      const vL=this.vL,vR=this.vR;vL.fill(0,0,n);vR.fill(0,0,n);this.renderVoice(v,n,vL,vR);
      const g=this.vdry[v];for(let s=0;s<n;s++){dL[s]+=vL[s]*g;dR[s]+=vR[s]*g}
      const sb=v*FX_SLOTS;for(let k=0;k<FX_SLOTS;k++){const a=this.snd[sb+k];if(!a)continue;
        if(!used[k]){used[k]=1;aL[k].fill(0);aR[k].fill(0)}const L1=aL[k],R1=aR[k];for(let s=0;s<n;s++){L1[s]+=vL[s]*a;R1[s]+=vR[s]*a}}}
    for(let k=0;k<FX_SLOTS&&k+1<outputs.length;k++){const o=outputs[k+1];if(!o||!o[0])continue;if(used[k]){o[0].set(aL[k].subarray(0,n));(o[1]||o[0]).set(aR[k].subarray(0,n))}}
    // The tail outlives its input by the predelay plus a full pass of the longest line,
    // so idling waits for that many consecutive silent blocks.
    const quiet=voices===0&&inE<1e-12&&this.space.energy<1e-14&&this.space.cur.freeze<.5&&!this.space.pending;
    this.quietBlocks=quiet?this.quietBlocks+1:0;
    this.idle=this.quietBlocks>this.space.holdBlocks;
    if(!this.idle){const d=this.dry;for(let s=0;s<n;s++){L[s]+=dL[s]*d;R[s]+=dR[s]*d}this.space.process(dL,dR,L,R,n)}
    this.frame=end;
    if(this.frame-this.statAt>=sampleRate*.25){this.statAt=this.frame;if(!this.idle||!this.idleReported)this.post({type:'stats',voices,steals:this.steals,drops:this.drops,strikes:this.strikes,idle:this.idle});this.idleReported=this.idle}
    return true;
  }
}
registerProcessor('cogs-engine',CogsEngine);
