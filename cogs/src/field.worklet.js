/* ---------- the fields: an effect send for every FX field on the stage ---------- */
// One input per field slot (stereo): every sound struck inside a field arrives here scaled by how far in it is.
// Each slot runs its field's effect (delay, reverb or distortion) and they are summed to one stereo return.
// Where in the field the latest sounds were struck (q: 0 at the rim, 1 at the centre) bends the effect's
// parameters, by as much as the field is diffuse: a ball rattling through a delay field bends its echoes.
const FIELD_SLOTS=8,FIELD_MODE_IDX={delay:0,reverb:1,dist:2};
const FIELD_SYNCS={'1/16':.25,'1/8':.5,'1/8.':.75,'1/4':1,'1/4.':1.5,'1/2':2,'1/2.':3};
const FDIST={tube:0,fold:1,fuzz:2,crush:3};
class FieldSlot{
  constructor(fs){
    this.fs=fs;this.p=null;this.mode=-1;this.q=.5;this.tq=.5;this.k=0;this.quiet=1e9;this.events=[];
    const N=Math.ceil(fs*3.2);this.dl=new Float32Array(N);this.dr=new Float32Array(N);this.dN=N;this.dw=0;this.dT=-1;this.dlp=[0,0];this.dhp=[0,0];
    this.verb=null;this.dz=[0,0,0,0,0,0];this.crush=[0,0,0];this.env=0;
  }
  set(p){
    const m=FIELD_MODE_IDX[p.mode]??0;
    if(m!==this.mode){this.mode=m;this.dl.fill(0);this.dr.fill(0);this.dT=-1;this.dz.fill(0);this.crush.fill(0);if(this.verb)this.verb.clear(this.verbParams(p,0))}
    this.p=p;
  }
  // q glides to the latest strike's place: fast enough to follow a moving thing, slow enough to bend, not click
  hit(q,amt){this.tq+=Math.max(.35,Math.min(1,amt))*(q-this.tq)}
  verbParams(p,k){return {...(p.verb||{}),wet:1,size:Math.min(1,Math.max(0,(p.size??.6)+.35*k)),decay:Math.max(.2,(p.decay??2.5)*Math.pow(2,1.3*k)),predelay:Math.max(0,(p.predelay??10)*(1-.6*k))}}
  run(inL,inR,L,R,n,bpm){
    const p=this.p,fs=this.fs;if(!p)return;
    let inE=0;if(inL)for(let s=0;s<n;s+=4)inE+=Math.abs(inL[s])+Math.abs(inR[s]);
    if(inE>1e-7)this.quiet=0;else this.quiet+=n;
    const tail=this.mode===0?(60/bpm*(FIELD_SYNCS[p.sync]||0)||p.time||.4)*(4+30*(p.fb||0))*fs:this.mode===1?(p.decay||2.5)*2.5*fs+fs:.05*fs;
    if(this.quiet>tail)return;
    const gq=1-Math.exp(-n/(fs*.06));this.q+=(this.tq-this.q)*gq;
    const D=Math.max(0,Math.min(1,p.diffuse??.5)),k=this.k=D*(2*this.q-1),lv=p.level??.8;
    const zl=inL||ZERO,zr=inR||ZERO;
    if(this.mode===0)this.delay(p,k,zl,zr,L,R,n,bpm,lv);
    else if(this.mode===1)this.reverb(p,k,zl,zr,L,R,n,lv);
    else this.dist(p,k,zl,zr,L,R,n,lv);
  }
  // A tape-ish delay: its length glides, so where the strikes fall bends the echoes' pitch as they move.
  delay(p,k,inL,inR,L,R,n,bpm,lv){
    const fs=this.fs,N=this.dN,bl=this.dl,br=this.dr,sync=FIELD_SYNCS[p.sync]||0;
    const T0=sync?60/bpm*sync:Math.max(.02,p.time||.375),T=Math.min(N-4,Math.max(8,T0*Math.pow(2,.5*k)*fs));
    if(this.dT<0)this.dT=T;
    const fb=Math.min(.95,Math.max(0,(p.fb??.45)+.25*k)),tone=Math.max(0,Math.min(1,(p.tone??.5)-.25*k));
    const lc=1-Math.exp(-TAU*Math.min(.45*fs,500*Math.pow(2,tone*5))/fs),hc=1-Math.exp(-TAU*90/fs),ping=p.ping!==false,gT=1-Math.exp(-1/(fs*.12));
    let w=this.dw,dT=this.dT,[l1,l2]=this.dlp,[h1,h2]=this.dhp;
    for(let s=0;s<n;s++){
      dT+=(T-dT)*gT;
      const pos=w-dT,i=Math.floor(pos),f=pos-i,a=((i%N)+N)%N,b=a+1===N?0:a+1;
      const yl=bl[a]+(bl[b]-bl[a])*f,yr=br[a]+(br[b]-br[a])*f;
      l1+=(yl-l1)*lc;l2+=(yr-l2)*lc;h1+=(l1-h1)*hc;h2+=(l2-h2)*hc;
      const el=Math.tanh((l1-h1)*fb),er=Math.tanh((l2-h2)*fb);
      if(ping){const m=(inL[s]+inR[s])*.5;bl[w]=m+er;br[w]=el}else{bl[w]=inL[s]+el;br[w]=inR[s]+er}
      L[s]+=yl*lv;R[s]+=yr*lv;w=w+1===N?0:w+1;
    }
    this.dw=w;this.dT=dT;this.dlp=[l1,l2];this.dhp=[h1,h2];
  }
  reverb(p,k,inL,inR,L,R,n,lv){
    if(!this.verb){const V=typeof Verb==='function'?Verb:FieldMiniVerb;this.verb=new V(this.fs,this.verbParams(p,k))}
    this.verb.set(this.verbParams(p,k));
    const tl=this.tl||(this.tl=new Float32Array(128)),tr=this.tr||(this.tr=new Float32Array(128));tl.fill(0,0,n);tr.fill(0,0,n);
    this.verb.process(inL,inR,tl,tr,n);
    for(let s=0;s<n;s++){L[s]+=tl[s]*lv;R[s]+=tr[s]*lv}
  }
  // Drive, shaped by its kind, then a tone low-pass and a DC block; nearer the centre drives harder and darker.
  dist(p,k,inL,inR,L,R,n,lv){
    const fs=this.fs,kind=FDIST[p.kind]??0,dr=Math.max(0,Math.min(1,(p.drive??.5)+.35*k)),g=1+dr*dr*40,inv=1/Math.pow(g,kind===2?.75:.5);
    const tone=Math.max(0,Math.min(1,(p.tone??.6)-.2*k)),lc=1-Math.exp(-TAU*Math.min(.45*fs,700*Math.pow(2,tone*4.8))/fs),hc=1-Math.exp(-TAU*25/fs);
    const z=this.dz,c=this.crush,hold=1+Math.floor(dr*20),bits=Math.pow(2,15-dr*12);
    for(let s=0;s<n;s++){
      for(let ch=0;ch<2;ch++){
        let x=(ch?inR[s]:inL[s])*g,y;
        if(kind===0)y=Math.tanh(x+.18)-.178;
        else if(kind===1)y=Math.sin(x*.6);
        else if(kind===2)y=Math.max(-1,Math.min(1,x*1.5));
        else{if(c[2]%hold===0)c[ch]=Math.round(Math.tanh(x*.3)*bits)/bits;y=c[ch]}
        y*=kind===3?.6:inv;
        z[ch]+=(y-z[ch])*lc;z[2+ch]+=(z[ch]-z[2+ch])*hc;const o=z[ch]-z[2+ch];
        if(ch)R[s]+=o*lv;else L[s]+=o*lv;
      }
      c[2]++;
    }
  }
}
const ZERO=new Float32Array(128);
// Until the main reverb arrives in the same script, a small FDN stands in for it.
class FieldMiniVerb{
  constructor(fs,p){this.fs=fs;const ms=[29.7,37.1,41.1,43.7,53.3,59.9,67.1,73.3];this.len=ms.map(m=>Math.round(m*fs/1000));
    this.buf=this.len.map(l=>new Float32Array(Math.ceil(l*2.2)+4));this.i=0;this.lp=new Float64Array(8);this.ap=[new Float32Array(Math.round(fs*.0047)),new Float32Array(Math.round(fs*.0113)),new Float32Array(Math.round(fs*.0037)),new Float32Array(Math.round(fs*.0091))];this.api=[0,0,0,0];this.set(p)}
  clear(){for(const b of this.buf)b.fill(0);for(const a of this.ap)a.fill(0);this.lp.fill(0)}
  set(p){this.p=p;const sz=.5+(p.size??.6);this.L=this.len.map(l=>Math.min(Math.round(l*sz),l*2.2|0));this.g=this.L.map(l=>Math.exp(-6.9*l/this.fs/Math.max(.2,p.decay||2.5)))}
  process(inL,inR,oL,oR,n){
    const N=8,b=this.buf,L=this.L,g=this.g,lp=this.lp,y=new Float64Array(N);
    for(let s=0;s<n;s++){
      let x=(inL[s]+inR[s])*.5;for(let j=0;j<4;j++){const a=this.ap[j],i=this.api[j]%a.length,d=a[i],v=x-.6*d;a[i]=v;this.api[j]++;x=d+.6*v}
      let sum=0;for(let j=0;j<N;j++){const B=b[j],r=B[(this.i-L[j]+B.length*4)%B.length];lp[j]+=(r-lp[j])*.45;y[j]=lp[j]*g[j];sum+=y[j]}
      sum*=2/N;let l=0,r=0;for(let j=0;j<N;j++){const B=b[j];B[this.i%B.length]=y[j]-sum+x*.35;if(j&1)r+=y[j];else l+=y[j]}
      this.i++;oL[s]+=l*.5;oR[s]+=r*.5;
    }
  }
}
class CogsFields extends AudioWorkletProcessor{
  constructor(){
    super();this.slots=Array.from({length:FIELD_SLOTS},()=>new FieldSlot(sampleRate));this.bpm=120;this.events=[];
    this.port.onmessage=e=>{const m=e.data;
      if(m.type==='field'){const sl=this.slots[m.slot];if(sl){if(m.p)sl.set(m.p);else{sl.p=null;sl.verb=null}}}
      else if(m.type==='hits'){for(let i=0;i<m.d.length;i+=4)this.events.push([m.d[i],m.d[i+1],m.d[i+2],m.d[i+3]])}
      else if(m.type==='tempo')this.bpm=m.bpm>0?m.bpm:120};
  }
  process(inputs,outputs){
    const out=outputs[0],L=out[0],R=out[1]||out[0],n=L.length,end=currentFrame+n;
    if(this.events.length){const keep=[];for(const ev of this.events){if(ev[1]<end){const sl=this.slots[ev[0]];if(sl)sl.hit(ev[2],ev[3])}else keep.push(ev)}this.events=keep}
    for(let k=0;k<FIELD_SLOTS;k++){const sl=this.slots[k];if(!sl.p)continue;const inp=inputs[k],a=inp&&inp[0],b=inp&&(inp[1]||inp[0]);sl.run(a,b,L,R,n,this.bpm)}
    if(currentFrame-(this.statAt||0)>=sampleRate*.1){this.statAt=currentFrame;this.port.postMessage({type:'stats',q:this.slots.map(s=>s.p?+s.q.toFixed(3):0),k:this.slots.map(s=>+s.k.toFixed(3)),quiet:this.slots.map(s=>s.quiet)})}
    return true;
  }
}
registerProcessor('cogs-fields',CogsFields);
