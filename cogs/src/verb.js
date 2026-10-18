/* ---------- the reverb: a VintageVerb-style algorithmic reverb, for the room and for each synth's FX rack ---------- */
// After Sean Costello's Valhalla VintageVerb, in spirit: a predelay, an input low and high cut, an early diffuser of four
// allpasses whose blend sets the echo density from sparse to dense, a swell of two long allpasses for the Attack, a tapped
// early-reflection line, and a late ring of 4 or 6 stages (modulated allpass, allpass, delay, three-band decay) read by
// Dattorro-style output taps. Each mode is a row of lengths, taps and a modulation recipe; each color is an era:
// 1970s runs the tank at half the sample rate, band-limited to 10 kHz, with noisy modulation; 1980s is full band with the
// noisy modulation; now is full band and clean. Included in both scripts: the worklet runs Verb, the page reads the names,
// the knob list and the presets. Everything else here is prefixed so it never meets the engine's or the page's names.
const VERB_MODES=['concert hall','bright hall','plate','room','chamber','random space','chorus space','ambience','sanctuary',
  'dirty hall','dirty plate','smooth plate','smooth room','smooth random','nonlin','chaotic hall','chaotic chamber','chaotic neutral','cathedral','palace'];
const VERB_COLORS=['1970s','1980s','now'];
// [key, label, min, max, step]. Mode and color are indices into VERB_MODES and VERB_COLORS.
const VERB_DEFS=[['mode','mode',0,VERB_MODES.length-1,1],['color','color',0,VERB_COLORS.length-1,1],['wet','wet',0,1.5,.001],
  ['predelay','predelay ms',0,500,1],['decay','decay s',.2,60,.01],['size','size',0,1,.01],['attack','attack',0,1,.01],
  ['early','early diffusion',0,1,.01],['late','late diffusion',0,1,.01],['modRate','mod rate Hz',.05,10,.01],['modDepth','mod depth',0,1,.01],
  ['highCut','high cut Hz',1000,20000,10],['lowCut','low cut Hz',10,1000,1],['bassMult','bass mult',.25,4,.01],['bassXover','bass xover Hz',50,2000,1],
  ['highShelf','high shelf dB',-24,0,.1],['highFreq','high freq Hz',500,16000,10],['earlyLate','early/late',0,1,.01],['width','width',0,1.5,.01],
  ['shimmer','shimmer',0,1,.01],['shimmerPitch','shimmer st',-12,24,1],['freeze','freeze',0,1,1]];
const VERB_DEFAULT={mode:0,color:1,wet:.3,predelay:4,decay:2.2,size:.6,attack:.2,early:.75,late:.7,modRate:.6,modDepth:.5,
  highCut:9000,lowCut:80,bassMult:1.2,bassXover:400,highShelf:-4,highFreq:5000,earlyLate:.6,width:1,shimmer:0,shimmerPitch:12,freeze:0};
const verbMode=name=>Math.max(0,VERB_MODES.indexOf(name)),verbColor=name=>Math.max(0,VERB_COLORS.indexOf(name));
// Musical starting points: each sets a mode and color and the knobs that make it sit.
const VERB_PRESETS={
  'big hall':{mode:0,color:1,predelay:18,decay:3.4,size:.8,attack:.35,early:.8,late:.75,modRate:.5,modDepth:.55,highCut:8000,lowCut:90,bassMult:1.1,bassXover:350,highShelf:-6,highFreq:4500,earlyLate:.7},
  'bright hall':{mode:1,color:2,predelay:12,decay:2.6,size:.7,attack:.25,early:.8,late:.7,modRate:.8,modDepth:.6,highCut:14000,lowCut:120,bassMult:1,bassXover:400,highShelf:-2,highFreq:7000,earlyLate:.65},
  'vocal plate':{mode:2,color:1,predelay:24,decay:2,size:.55,attack:.1,early:.85,late:.7,modRate:.7,modDepth:.45,highCut:11000,lowCut:180,bassMult:.9,bassXover:500,highShelf:-3,highFreq:6000,earlyLate:.8},
  'drum room':{mode:3,color:2,predelay:0,decay:.7,size:.35,attack:0,early:.9,late:.6,modRate:.4,modDepth:.3,highCut:10000,lowCut:100,bassMult:.9,bassXover:300,highShelf:-6,highFreq:4000,earlyLate:.45},
  'tight chamber':{mode:4,color:1,predelay:6,decay:1.2,size:.4,attack:.1,early:.9,late:.75,modRate:.6,modDepth:.4,highCut:9000,lowCut:90,bassMult:1,bassXover:400,highShelf:-4,highFreq:5000,earlyLate:.55},
  'deep space':{mode:5,color:2,predelay:30,decay:7,size:.9,attack:.6,early:.8,late:.8,modRate:.3,modDepth:.7,highCut:7000,lowCut:70,bassMult:1.3,bassXover:300,highShelf:-8,highFreq:3500,earlyLate:.85},
  'chorus space':{mode:6,color:2,predelay:20,decay:5,size:.85,attack:.5,early:.75,late:.75,modRate:.9,modDepth:.75,highCut:9000,lowCut:80,bassMult:1.2,bassXover:350,highShelf:-6,highFreq:4500,earlyLate:.8},
  'ambience':{mode:7,color:2,predelay:0,decay:.5,size:.5,attack:0,early:.9,late:.6,modRate:.5,modDepth:.3,highCut:12000,lowCut:120,bassMult:.8,bassXover:400,highShelf:-3,highFreq:6000,earlyLate:.35},
  'sanctuary':{mode:8,color:2,predelay:40,decay:9,size:.9,attack:.7,early:.75,late:.85,modRate:.35,modDepth:.6,highCut:6500,lowCut:60,bassMult:1.4,bassXover:300,highShelf:-9,highFreq:3000,earlyLate:.9},
  'smooth plate':{mode:11,color:2,predelay:10,decay:2.4,size:.6,attack:.15,early:.85,late:.8,modRate:.6,modDepth:.5,highCut:12000,lowCut:150,bassMult:1,bassXover:450,highShelf:-3,highFreq:6000,earlyLate:.75},
  'smooth room':{mode:12,color:2,predelay:4,decay:1,size:.45,attack:.05,early:.9,late:.75,modRate:.5,modDepth:.4,highCut:10000,lowCut:100,bassMult:1,bassXover:400,highShelf:-4,highFreq:5000,earlyLate:.5},
  'gated':{mode:14,color:1,predelay:0,decay:1,size:.35,attack:.5,early:1,late:.7,modRate:.5,modDepth:.2,highCut:9000,lowCut:120,bassMult:1,bassXover:400,highShelf:-3,highFreq:5000,earlyLate:1},
  'reverse':{mode:14,color:2,predelay:0,decay:1,size:.6,attack:1,early:1,late:.7,modRate:.5,modDepth:.3,highCut:9000,lowCut:120,bassMult:1,bassXover:400,highShelf:-3,highFreq:5000,earlyLate:1},
  'tape hall':{mode:15,color:0,predelay:20,decay:3,size:.75,attack:.3,early:.8,late:.7,modRate:.7,modDepth:.6,highCut:8000,lowCut:90,bassMult:1.2,bassXover:350,highShelf:-6,highFreq:4000,earlyLate:.7},
  'cathedral':{mode:18,color:2,predelay:35,decay:12,size:1,attack:.6,early:.8,late:.85,modRate:.3,modDepth:.5,highCut:6000,lowCut:60,bassMult:1.5,bassXover:250,highShelf:-10,highFreq:3000,earlyLate:.9},
};

// The modes. Lengths are ms at size 1 (size 0 is smin of that); d: ring delays, a1: the modulated allpasses, a2: the
// second allpasses, ind: the early diffuser, er: [taps per side, span ms, level], mod: weights of [sine chorus, random
// walk on the allpasses, random delay (no pitch change: slow and wide on the long delays), chaos], depth/dd: chorus and
// delay-randomisation depth, hi: the treble decay against the decay knob, sat: saturation in the loop, att: built-in
// attack, diff: the ceiling of late diffusion, gain: loudness trim.
const VERB_ALGOS=(()=>{
  const hall={k:4,d:[137,113,127,101],a1:[29.3,23.1,26.7,19.9],a2:[47.3,39.1,53.7,31.9],ind:[4.77,3.59,12.73,9.31],er:[12,70,.5],
    mod:[1,0,0,0],depth:1,dd:0,hi:.75,sat:0,smin:.3,att:0,diff:.7,gain:1};
  const plate={k:4,d:[149.6,125,141.7,106.3],a1:[22.6,30.5,24.9,28.1],a2:[60.5,89.2,53.3,71.9],ind:[4.77,3.59,12.73,9.31],er:[12,30,.6],
    mod:[1,0,0,0],depth:.8,dd:0,hi:1,sat:0,smin:.35,att:0,diff:.7,gain:1};
  const room={k:4,d:[53,61,47,67],a1:[11.3,8.9,13.1,7.7],a2:[17.3,23.9,13.7,26.7],ind:[3.1,2.3,5.7,4.1],er:[14,55,1],
    mod:[0,1,0,0],depth:.6,dd:0,hi:.6,sat:0,smin:.25,att:0,diff:.6,gain:1};
  const chamber={k:4,d:[73,89,67,81],a1:[17.3,13.9,19.1,11.3],a2:[27.1,33.9,23.3,29.7],ind:[4.1,3.3,7.9,6.1],er:[12,35,.6],
    mod:[1,.3,0,0],depth:.8,dd:0,hi:.85,sat:0,smin:.3,att:0,diff:.7,gain:1};
  const space={k:6,d:[131,157,113,149,121,163],a1:[29.1,23.3,37.7,19.7,31.3,26.9],a2:[43.1,59.3,37.9,51.7,41.9,62.3],ind:[5.3,4.1,14.3,10.9],er:[16,80,.6],
    mod:[0,0,1,0],depth:.5,dd:1,hi:.75,sat:0,smin:.3,att:.25,diff:.7,gain:1};
  const scale=(o,f,fa=1)=>({...o,d:o.d.map(x=>x*f),a1:o.a1.map(x=>x*fa)});
  const trim=[1,1,1.06,.8,.88,1.5,1.48,.62,1.57,1.03,1.09,1.05,.81,1.45,1.68,1.01,.88,1.06,1.66,1.67];
  return [
    hall,                                                            // concert hall
    {...hall,hi:1,depth:1.6},                                        // bright hall
    plate,                                                           // plate
    room,                                                            // room
    chamber,                                                         // chamber
    space,                                                           // random space
    {...space,mod:[1,0,0,0],depth:1.2,dd:0},                         // chorus space
    {k:4,d:[23,29,19,31],a1:[7.1,5.3,6.7,4.3],a2:[7.9,11.3,6.7,9.1],ind:[1.3,1.9,2.9,3.7],er:[16,28,1.2],
      mod:[1,0,0,0],depth:.4,dd:0,hi:.8,sat:0,smin:.3,att:0,diff:.65,gain:1},  // ambience
    {...scale(space,1.25),mod:[1,.3,.5,0],depth:1.3,dd:.5,hi:.55,att:.45,smin:.35,er:[14,100,.45]}, // sanctuary
    {...hall,mod:[0,1,0,0],depth:1.2,sat:.6,diff:.5,hi:.7},          // dirty hall
    {...plate,mod:[0,.4,0,0],depth:.3,sat:.5,diff:.35},              // dirty plate
    {...plate,mod:[1,0,.6,0],depth:.8,dd:.6,diff:.75,ind:[5.9,4.3,15.1,11.3]}, // smooth plate
    {...room,mod:[1,0,.6,0],dd:.6,diff:.7,er:[14,55,.6]},            // smooth room
    {...space,mod:[.5,0,1,0],diff:.75},                              // smooth random
    {...room,nl:true,ind:[2.3,3.7,5.9,8.9],er:[32,500,1],smin:.12},  // nonlin: er span is the gate length
    {...hall,mod:[0,0,0,1],depth:1,dd:.5,sat:.3},                    // chaotic hall
    {...chamber,mod:[0,0,0,1],depth:1,dd:.4,sat:.2},                 // chaotic chamber
    {...plate,mod:[0,0,0,1],depth:.8,dd:.6,hi:1,diff:.75},           // chaotic neutral
    {...scale(space,1.6,1.3),mod:[1,0,.4,0],depth:1,dd:.4,hi:.6,att:.4,smin:.4,er:[14,120,.45]}, // cathedral
    {...scale(space,2),mod:[1,0,.3,0],depth:1.4,dd:.3,hi:.9,att:.35,smin:.4,er:[14,140,.45]},     // palace
  ].map((a,i)=>({...a,gain:trim[i]}));
})();
const VERB_SWELL=[19.3,31.7],VERB_IND0=[1.13,1.71];
const VERB_P2=n=>{let p=1;while(p<n)p<<=1;return p};
// A seeded generator, so a mode's taps are the same every time.
const verbRng=seed=>()=>{seed=(seed*1664525+1013904223)>>>0;return seed/4294967296};

class Verb{
  constructor(fs,params){
    this.fs=fs;
    const off=[],mask=[];let total=0;
    const alloc=ms=>{const L=VERB_P2(Math.ceil(ms*fs/1000)+8);off.push(total);mask.push(L-1);total+=L;return off.length-1};
    const most=(f)=>Math.max(...VERB_ALGOS.map(f));
    this.lPre=[alloc(501),alloc(501)];
    this.lInd=[];for(let c=0;c<2;c++)for(let j=0;j<6;j++)this.lInd.push(alloc(j<2?VERB_IND0[j]*1.12:most(a=>a.ind[j-2])*1.12));
    this.lSw=[];for(let c=0;c<2;c++)for(let j=0;j<2;j++)this.lSw.push(alloc(VERB_SWELL[j]*1.12));
    this.lEr=[alloc(most(a=>a.er[1])*1.02+2),alloc(most(a=>a.er[1])*1.02+2)];
    this.lA1=[];this.lA2=[];this.lD=[];
    for(let i=0;i<6;i++){
      this.lA1.push(alloc(most(a=>a.a1[i]||0)+3.5));this.lA2.push(alloc(most(a=>a.a2[i]||0)+1));this.lD.push(alloc(most(a=>a.d[i]||0)+9));
    }
    this.mem=new Float32Array(total);this.off=Int32Array.from(off);this.mask=Int32Array.from(mask);
    const at=ls=>[Int32Array.from(ls,l=>off[l]),Int32Array.from(ls,l=>mask[l])];
    [this.iO,this.iM]=at(this.lInd);[this.sO,this.sM]=at(this.lSw);[this.a1O,this.a1M]=at(this.lA1);[this.a2O,this.a2M]=at(this.lA2);[this.dO,this.dM]=at(this.lD);
    this.shSize=VERB_P2(fs*.2);this.shBuf=new Float32Array(this.shSize);
    const B=128;this.cap=0;this.grow(B);
    // per stage
    const S=6,f64=()=>new Float64Array(S);
    Object.assign(this,{dLen:f64(),a1Len:f64(),a2Len:f64(),gl:f64(),gm:f64(),gh:f64(),lo:f64(),hp:f64(),o:f64(),mc:f64(),ms:f64(),rc:f64(),rs:f64(),
      r1:f64(),r2:f64(),tg:f64(),cnt:f64(),chaos:f64(),injL:f64(),injR:f64(),mA:f64()});this.injK=new Int32Array(S);
    // taps: early (up to 64 per side) and late (3 per stage per side)
    this.eO=new Int32Array(128);this.eM=new Int32Array(128);this.eD=new Int32Array(128);this.eGa=new Float64Array(128);this.eU=new Float64Array(128);
    this.tO=new Int32Array(64);this.tM=new Int32Array(64);this.tD=new Int32Array(64);this.tGa=new Float64Array(64);this.tF=new Float64Array(64);this.tLine=new Int32Array(64);
    this.indD=new Int32Array(12);this.swD=new Float64Array(4);this.gIn=new Float64Array(6);
    this.target={...VERB_DEFAULT};this.cur={...VERB_DEFAULT};this.energy=0;this.holdBlocks=8;
    this.glide=1-Math.exp(-B/(fs*.05));this.sizeGlide=1-Math.exp(-B/(fs*.35));this.preGlide=1-Math.exp(-B/(fs*.12));
    this.clear(params);
  }
  grow(n){if(n<=this.cap)return;this.cap=n;for(const k of ['tinL','tinR','toutL','toutR','xL','xR'])this[k]=new Float64Array(n)}
  // Silence everything and take params (over the defaults) at once, without a glide.
  clear(params){
    this.mem.fill(0);this.shBuf.fill(0);this.target={...VERB_DEFAULT};
    for(const k of VERB_DEFS)if(params&&Number.isFinite(+params[k[0]]))this.target[k[0]]=+params[k[0]];
    this.target.mode=Math.round(Math.min(VERB_MODES.length-1,Math.max(0,this.target.mode)));this.target.color=Math.round(Math.min(2,Math.max(0,this.target.color)));
    Object.assign(this.cur,this.target);
    this.w=0x100000;this.pw=0;this.ph=0;this.hold=[0,0];this.shW=0;this.shP=0;this.shLp=0;this.shOut=0;this.dn=1e-18;this.energy=0;
    this.fz={lcL:0,lcR:0,h1L:0,h1R:0,h2L:0,h2R:0};this.bq=[new Float64Array(4),new Float64Array(4),new Float64Array(4),new Float64Array(4)];
    this.inGain=this.cur.freeze>=.5?0:1;this.fade=1;this.pending=false;
    for(const k of ['lo','hp','o','r1','r2','tg','mA'])this[k].fill(0);
    for(let i=0;i<6;i++){const ph=i*2.399;this.mc[i]=Math.cos(ph);this.ms[i]=Math.sin(ph);this.chaos[i]=.13+.11*i;this.cnt[i]=0}
    this.build();this.retune(true);
  }
  // A partial set of params; continuous ones glide, mode and color crossfade through silence.
  set(p){
    for(const k in p){if(!(k in this.target))continue;const x=+p[k];if(!Number.isFinite(x))continue;
      if(k==='mode'||k==='color'){const r=Math.round(Math.min(k==='mode'?VERB_MODES.length-1:2,Math.max(0,x)));if(r!==this.target[k]){this.target[k]=r;this.pending=true}}
      else if(this.target[k]!==x){this.target[k]=x;this.moving=true}}
  }
  // The structure of the current mode and color: lengths, taps and how the input enters the ring.
  build(){
    const c=this.cur,A=this.algo=VERB_ALGOS[c.mode],K=this.K=A.nl?0:A.k,rnd=verbRng(7919*(c.mode+1));
    this.decim=c.color===0?2:1;this.fsT=this.fs/this.decim;this.nl=!!A.nl;
    // the 1970s band: 2-pole lowpasses before the decimation and after the hold
    const fc=Math.min(9500,this.fs*.2),w=2*Math.PI*fc/this.fs,al=Math.sin(w)/(2*Math.SQRT1_2),cs=Math.cos(w),a0=1+al;
    this.bqc=[(1-cs)/2/a0,(1-cs)/a0,(1-cs)/2/a0,-2*cs/a0,(1-al)/a0];
    // early taps: per side, times bunched later as a room's reflections thicken, gains falling, a third crossing over
    const n=A.er[0],E=this.nE=2*n;
    for(let j=0;j<E;j++){
      const side=j<n?0:1,k=j%n,u=Math.pow((k+.5+.8*(rnd()-.5))/n,A.nl?1:.75),src=rnd()<(A.nl?.5:.3)?1-side:side,sgn=rnd()<.5?-1:1;
      this.eU[j]=Math.min(1,Math.max(.002,u));this.eO[j]=this.off[this.lEr[src]];this.eM[j]=this.mask[this.lEr[src]];
      this.eGa[j]=sgn;
    }
    // late taps: two in each stage's delay (one near its head, so the ring speaks at once) and one in its second allpass,
    // per side, with alternating signs
    const tq=K>4?2:3;let t=0;for(let side=0;side<2;side++)for(let i=0;i<K;i++)for(let q=3-tq;q<3;q++){
      const sgn=((i+q+side)&1)?-1:1,line=q<2?this.lD[i]:this.lA2[i];
      this.tLine[t]=q<2?0:1;this.tO[t]=this.off[line];this.tM[t]=this.mask[line];this.tF[t]=q===0?.02+.16*rnd():q===1?(tq<3?.02+.4*rnd():.3+.65*rnd()):.05+.45*rnd();this.tD[t]=i;
      this.tGa[t]=sgn;t++;
    }
    this.nT=t;this.tStage=Int32Array.from(this.tD.subarray(0,t));
    this.tapNorm=K?1.6/Math.sqrt(t/2):0;
  }
  retune(snap){
    const t=this.target,c=this.cur;
    if(snap||this.moving){
      let moved=false;
      for(const k in t){if(k==='mode'||k==='color')continue;const was=c[k];
        if(snap||k==='freeze'||k==='shimmerPitch')c[k]=t[k];else{const g=k==='size'?this.sizeGlide:k==='predelay'?this.preGlide:this.glide;c[k]+=(t[k]-c[k])*g;
          if(Math.abs(t[k]-c[k])<=1e-6*(1+Math.abs(t[k])))c[k]=t[k]}
        if(c[k]!==t[k])moved=true}
      this.moving=moved;
    }else return;
    const A=this.algo,K=this.K,fs=this.fs,fsT=this.fsT,ms=fsT/1000,TAU_=2*Math.PI;
    const sc=A.smin+(1-A.smin)*Math.min(1,Math.max(0,c.size));
    const frozen=c.freeze>=.5,T=Math.max(.05,c.decay),TL=T*Math.max(.1,c.bassMult),TH=Math.min(T*1.2,T*A.hi*Math.pow(10,Math.min(0,c.highShelf)/20));
    // a short decay shrinks a big space: a stage that lost more than 5 dB a pass would come round again as a repeat
    let ring=0,stage=0;for(let i=0;i<K;i++)stage=Math.max(stage,A.d[i]+A.a1[i]+A.a2[i]);
    const scR=frozen?sc:Math.min(sc,T*1000/12/stage);
    for(let i=0;i<K;i++){
      // frozen, every length is whole and nothing moves, so no interpolation dulls the held tail
      const rd=frozen?Math.round:x=>x,D=rd(Math.max(4,A.d[i]*scR*ms)),a1=rd(Math.max(2,A.a1[i]*scR*ms)),a2=rd(Math.max(2,A.a2[i]*scR*ms));
      this.dLen[i]=D;this.a1Len[i]=a1;this.a2Len[i]=a2;ring+=D+a1+a2;
      const sec=(D+a1+a2)/fsT;
      this.gm[i]=frozen?1:Math.pow(10,-3*sec/T);this.gl[i]=frozen?1:Math.pow(10,-3*sec/TL);this.gh[i]=frozen?1:Math.pow(10,-3*sec/TH);
    }
    for(let j=0;j<this.nT;j++){const i=this.tStage[j],L=this.tLine[j]?this.a2Len[i]:this.dLen[i];this.tD[j]=Math.max(1,Math.round(this.tF[j]*L))}
    // early: the diffuser (lengths fixed, so a size change never sweeps the onset), the swell and the reflection taps
    for(let side=0;side<2;side++)for(let j=0;j<6;j++)this.indD[side*6+j]=Math.max(1,Math.round((j<2?VERB_IND0[j]:A.ind[j-2])*(side?1.11:1)*ms));
    const ge=.5+.25*Math.min(1,Math.max(0,c.early));this.gIn[0]=this.gIn[1]=.6;this.gIn[2]=this.gIn[3]=ge;this.gIn[4]=this.gIn[5]=ge*.83;
    this.blend=Math.min(1,Math.max(0,c.early)*1.6);
    const att=Math.min(1,Math.max(0,c.attack+A.att*(1-c.attack)));this.att=att;this.gSw=.7*att;
    for(let side=0;side<2;side++)for(let j=0;j<2;j++)this.swD[side*2+j]=Math.max(1,VERB_SWELL[j]*(side?1.11:1)*sc*ms);
    const span=A.er[1]*(A.nl?.12+.88*Math.min(1,Math.max(0,c.size)):sc)*ms;let erMax=0;
    const nl=this.nl,shape=c.attack*2-1,env=j=>{const u=this.eU[j];
      // nonlin: attack bends the gate from a cut-off decay (0) through flat (.5) to a reverse swell (1)
      return nl?Math.exp(shape<0?shape*4*u:-shape*4*(1-u)):Math.exp(-2.3*u)};
    // the taps' energy is held level, so a gate of any shape and length sounds as loud
    let sum=0;for(let j=0;j<this.nE;j++)sum+=env(j)**2;const norm=nl?.73/Math.sqrt(sum/2):1/Math.sqrt(this.nE/2);
    for(let j=0;j<this.nE;j++){const d=Math.max(1,Math.round(this.eU[j]*span));this.eD[j]=d;erMax=Math.max(erMax,d);
      this.eGa[j]=Math.sign(this.eGa[j])*env(j)*norm}
    // inject: the first stage takes the left, the middle stage the right, through their allpasses; a fast attack also
    // feeds the other stages, straight into their delays at staggered depths, so the first pass reaches the next stage
    // spread over the loop and never comes round again as one wave
    for(let i=0;i<K;i++){const h=K>>1,spread=(1-att)*.7,s=(i&2)?-1:1,odd=i&1;
      this.injL[i]=i===0||i===h?0:spread*s*(odd?.35:.9);this.injR[i]=i===0||i===h?0:spread*s*(odd?.9:.35)*(i>h?-1:1);
      this.injK[i]=Math.round(this.dLen[i]*(.2+.6*((i*.618034)%1)))-Math.round(this.dLen[i])}
    // early/late balance: each full at the middle
    const el=Math.min(1,Math.max(0,c.earlyLate));
    const cg=A.gain*(this.decim>1?1.27:1);
    this.eG=(Math.min(1,2*(1-el))*A.er[2]*(1-.75*att)*(nl?0:1)+(nl?1:0))*cg;this.lG=nl?0:Math.min(1,2*el)*this.tapNorm*cg;
    // modulation: [sine, random walk, random delay, chaos]; the old colors add the noisy walk
    const m=A.mod,dep=Math.min(1,Math.max(0,c.modDepth)),noisy=c.color<2?.5:0;
    this.kS=m[0];this.kR=m[1]+m[3]+noisy;this.kD=(m[2]+m[3]*.5)*A.dd;this.chaotic=m[3]>0;
    this.dA=frozen?0:dep*A.depth*.0009*fsT;this.dD=frozen?0:dep*.0025*fsT;
    for(let i=0;i<K;i++)this.dA=Math.min(this.dA,this.a1Len[i]-1.5);
    const rate=Math.max(.01,c.modRate);this.rate=rate;
    for(let i=0;i<K;i++){const w=TAU_*rate*(.8+.4*i/Math.max(1,K-1))/fsT;this.rc[i]=Math.cos(w);this.rs[i]=Math.sin(w)}
    this.ra=1-Math.exp(-TAU_*rate*(this.chaotic?3:1.2)/fsT);
    this.gL1=Math.min(1,Math.max(0,c.late))*A.diff;this.gL2=this.gL1*.72;
    this.aLo=1-Math.exp(-TAU_*Math.min(c.bassXover,fsT*.45)/fsT);this.aHi=1-Math.exp(-TAU_*Math.min(c.highFreq,fsT*.45)/fsT);
    this.sat=A.sat;
    this.aLC=1-Math.exp(-TAU_*Math.min(c.lowCut,fs*.45)/fs);this.aHC=1-Math.exp(-TAU_*Math.min(c.highCut,fs*.45)/fs);
    this.preLen=Math.max(0,Math.min(500,c.predelay))*fs/1000;
    this.shim=frozen?0:Math.min(.6,Math.max(0,c.shimmer)*.6);this.shRatio=Math.pow(2,c.shimmerPitch/12);this.shWin=Math.round(fsT*.07);
    this.shA=1-Math.exp(-TAU_*Math.min(6000,fsT*.4)/fsT);
    // the tail outlives its input by the predelay, the early taps, the swell and one pass of the ring
    this.holdBlocks=Math.ceil((this.preLen+this.decim*(erMax+ring+this.swD[2]+this.swD[3]+40))/128)+2;
  }
  // inL/inR: the dry block. Adds only the wet signal, scaled by wet, into outL/outR.
  process(inL,inR,outL,outR,n){
    this.grow(n);
    const switching=this.pending;
    this.retune(false);
    const c=this.cur,mem=this.mem,fs=this.fs,decim=this.decim,tinL=this.tinL,tinR=this.tinR,f=this.fz;
    // predelay, then the input's low cut and 2-pole high cut, at the full rate
    const pL=this.off[this.lPre[0]],pR=this.off[this.lPre[1]],pM=this.mask[this.lPre[0]],pre=this.preLen,aLC=this.aLC,aHC=this.aHC;
    const ph0=this.ph;let pw=this.pw,lcL=f.lcL,lcR=f.lcR,h1L=f.h1L,h1R=f.h1R,h2L=f.h2L,h2R=f.h2R,m=0,ph=this.ph;
    const ig=this.inGain,igT=c.freeze>=.5?0:1,igS=(igT-ig)/n;let g=ig;
    const bq=this.bqc,b0=bq[0],b1=bq[1],b2=bq[2],a1=bq[3],a2=bq[4],zL=this.bq[0],zR=this.bq[1];
    for(let s=0;s<n;s++){
      mem[pL+(pw&pM)]=inL[s];mem[pR+(pw&pM)]=inR[s];
      let xl,xr;
      if(pre<1){xl=inL[s];xr=inR[s]}else{const pos=pw-pre,i=Math.floor(pos),fr=pos-i,i0=i&pM,i1=(i+1)&pM;
        xl=mem[pL+i0]+(mem[pL+i1]-mem[pL+i0])*fr;xr=mem[pR+i0]+(mem[pR+i1]-mem[pR+i0])*fr}
      pw=(pw+1)&0x3fffffff;g+=igS;
      lcL+=aLC*(xl-lcL);lcR+=aLC*(xr-lcR);xl=(xl-lcL)*g;xr=(xr-lcR)*g;
      h1L+=aHC*(xl-h1L);h1R+=aHC*(xr-h1R);h2L+=aHC*(h1L-h2L);h2R+=aHC*(h1R-h2R);xl=h2L;xr=h2R;
      if(decim===1){tinL[s]=xl;tinR[s]=xr;m++}
      else{
        const yl=b0*xl+b1*zL[0]+b2*zL[1]-a1*zL[2]-a2*zL[3];zL[1]=zL[0];zL[0]=xl;zL[3]=zL[2];zL[2]=yl;
        const yr=b0*xr+b1*zR[0]+b2*zR[1]-a1*zR[2]-a2*zR[3];zR[1]=zR[0];zR[0]=xr;zR[3]=zR[2];zR[2]=yr;
        ph^=1;if(ph){tinL[m]=yl;tinR[m]=yr;m++}
      }
    }
    this.inGain=igT;this.pw=pw;f.lcL=lcL;f.lcR=lcR;f.h1L=h1L;f.h1R=h1R;f.h2L=h2L;f.h2R=h2R;
    this.tank(m);
    // back to the full rate (the 1970s hold and lowpass), width, the switch fade, wet
    const toL=this.toutL,toR=this.toutR,wet=c.wet,wd=c.width,yL=this.bq[2],yR=this.bq[3];
    let e=0,fade=this.fade;const fStep=switching?-1/n:(fade<1?1/n:0);
    let t=0,hl=this.hold[0],hr=this.hold[1];ph=ph0;
    for(let s=0;s<n;s++){
      let l,r;
      if(decim===1){l=toL[s];r=toR[s]}
      else{ph^=1;if(ph){const nl=toL[t],nr=toR[t];t++;l=.5*(hl+nl);r=.5*(hr+nr);hl=nl;hr=nr}else{l=hl;r=hr}
        const ol=b0*l+b1*yL[0]+b2*yL[1]-a1*yL[2]-a2*yL[3];yL[1]=yL[0];yL[0]=l;yL[3]=yL[2];yL[2]=ol;
        const or=b0*r+b1*yR[0]+b2*yR[1]-a1*yR[2]-a2*yR[3];yR[1]=yR[0];yR[0]=r;yR[3]=yR[2];yR[2]=or;l=ol;r=or}
      const mid=(l+r)*.5,sd=(l-r)*.5*wd;l=mid+sd;r=mid-sd;
      e+=l*l+r*r;
      fade=Math.min(1,Math.max(0,fade+fStep));const k=wet*fade;
      outL[s]+=l*k;outR[s]+=r*k;
    }
    this.hold[0]=hl;this.hold[1]=hr;this.ph=ph;this.fade=fade;
    this.energy=e/n;
    // a new mode or color: the old tail has faded out over this block; rebuild in silence and fade in on the next
    if(switching){this.pending=false;c.mode=this.target.mode;c.color=this.target.color;const keep={...this.target};
      this.mem.fill(0);this.shBuf.fill(0);for(const k of ['lo','hp','o','r1','r2','mA'])this[k].fill(0);
      for(const z of this.bq)z.fill(0);this.hold[0]=this.hold[1]=0;this.shLp=this.shOut=0;
      this.build();this.target=keep;this.retune(true);this.fade=0}
  }
  tank(m){
    const mem=this.mem,off=this.off,mask=this.mask,K=this.K,tinL=this.tinL,tinR=this.tinR,toL=this.toutL,toR=this.toutR;
    const indD=this.indD,gIn=this.gIn,blend=this.blend,att=this.att,gSw=this.gSw,swD=this.swD;
    const iO=this.iO,iM=this.iM,sO=this.sO,sM=this.sM;
    const eOL=off[this.lEr[0]],eOR=off[this.lEr[1]],eMk=mask[this.lEr[0]],nE=this.nE,eO=this.eO,eD=this.eD,eGa=this.eGa,nE2=nE>>1,eG=this.eG,lG=this.lG;
    const nT=this.nT,tO=this.tO,tM=this.tM,tD=this.tD,tGa=this.tGa,nT2=nT>>1;
    const a1O=this.a1O,a1M=this.a1M,a2O=this.a2O,a2M=this.a2M,dO=this.dO,dM=this.dM;
    const dLen=this.dLen,a1Len=this.a1Len,a2Len=this.a2Len,gl=this.gl,gm=this.gm,gh=this.gh,lo=this.lo,hp=this.hp,o=this.o,injL=this.injL,injR=this.injR,injK=this.injK,hK=K>>1;
    const mc=this.mc,ms=this.ms,rc=this.rc,rs=this.rs,r1=this.r1,r2=this.r2,tg=this.tg,cnt=this.cnt,chaos=this.chaos,mA=this.mA;
    const kN=1/Math.max(1,this.kS+this.kR),kS=this.kS*kN,kR=this.kR*kN,kD=this.kD,dA=this.dA,dD=this.dD,ra=this.ra,chaotic=this.chaotic,period=this.fsT/(this.rate*(chaotic?3:2));
    const g1=-this.gL1,g2=this.gL2,aLo=this.aLo,aHi=this.aHi,sat=this.sat,shim=this.shim;
    let w=this.w,dn=this.dn;
    for(let t=0;t<m;t++){
      let xl=tinL[t],xr=tinR[t];
      // early diffusion: six allpasses a side (two short ones to thicken the first milliseconds, then the mode's four), blended in from the raw input (sparse) to all of it (dense)
      let yl=xl,yr=xr;
      for(let j=0;j<6;j++){const g=gIn[j];
        let k=iM[j],oo=iO[j],d=mem[oo+((w-indD[j])&k)],v=yl-g*d;mem[oo+(w&k)]=v;yl=d+g*v;
        k=iM[j+6];oo=iO[j+6];d=mem[oo+((w-indD[j+6])&k)];v=yr-g*d;mem[oo+(w&k)]=v;yr=d+g*v}
      xl+=(yl-xl)*blend;xr+=(yr-xr)*blend;
      mem[eOL+(w&eMk)]=xl;mem[eOR+(w&eMk)]=xr;
      let el=0,er=0;
      for(let j=0;j<nE2;j++)el+=mem[eO[j]+((w-eD[j])&eMk)]*eGa[j];
      for(let j=nE2;j<nE;j++)er+=mem[eO[j]+((w-eD[j])&eMk)]*eGa[j];
      if(K===0){toL[t]=el*eG;toR[t]=er*eG;w=w+1>=0x40000000?0x100000:w+1;continue}
      // the swell: two long allpasses a side, blended in by the attack
      let sl=xl,sr=xr;
      for(let j=0;j<2;j++){
        let k=sM[j],oo=sO[j],pos=w-swD[j],i=(pos|0),fr=pos-i,d=mem[oo+(i&k)]+(mem[oo+((i+1)&k)]-mem[oo+(i&k)])*fr,v=sl-gSw*d;mem[oo+(w&k)]=v;sl=d+gSw*v;
        k=sM[j+2];oo=sO[j+2];pos=w-swD[j+2];i=(pos|0);fr=pos-i;d=mem[oo+(i&k)]+(mem[oo+((i+1)&k)]-mem[oo+(i&k)])*fr;v=sr-gSw*d;mem[oo+(w&k)]=v;sr=d+gSw*v}
      sl=xl+(sl-xl)*att;sr=xr+(sr-xr)*att;
      if(shim>0){sl+=this.shOut*shim;sr+=this.shOut*shim}
      // the ring: read every stage's delay (randomised for the random modes), its three-band decay and saturation
      for(let i=0;i<K;i++){
        const cc=mc[i],ss=ms[i];mc[i]=cc*rc[i]-ss*rs[i];ms[i]=ss*rc[i]+cc*rs[i];
        if(--cnt[i]<=0){cnt[i]=period*(.75+.5*Math.random());
          if(chaotic){let x=chaos[i];x=3.97*x*(1-x);if(x<=0||x>=1)x=.37;chaos[i]=x;tg[i]=2*x-1}else tg[i]=Math.random()*2-1}
        const q1=r1[i]+ra*(tg[i]-r1[i]),q2=r2[i]+ra*(q1-r2[i]);r1[i]=q1;r2[i]=q2;
        mA[i]=dA*(1+kS*ms[i]+kR*q2);
        const k=dM[i],oo=dO[i],pos=w-dLen[i]-dD*kD*(1+q2),ii=(pos|0),fr=pos-ii,a=mem[oo+(ii&k)],r=a+(mem[oo+((ii+1)&k)]-a)*fr;
        const l=lo[i]+aLo*(r-lo[i]),h=hp[i]+aHi*(r-hp[i]);lo[i]=l;hp[i]=h;
        let y=gl[i]*l+gm[i]*(h-l)+gh[i]*(r-h);
        if(sat>0)y=y/(1+sat*Math.abs(y));
        o[i]=y;
      }
      // and write each stage from the one before: the modulated allpass, the second allpass, the delay
      for(let i=0;i<K;i++){
        let x=o[i===0?K-1:i-1]+(i===0?sl:i===hK?sr:0);
        let k=a1M[i],oo=a1O[i],pos=w-a1Len[i]-mA[i],ii=(pos|0),fr=pos-ii,a=mem[oo+(ii&k)],d=a+(mem[oo+((ii+1)&k)]-a)*fr,v=x-g1*d;
        mem[oo+(w&k)]=v;x=d+g1*v;
        k=a2M[i];oo=a2O[i];pos=w-a2Len[i];ii=(pos|0);fr=pos-ii;a=mem[oo+(ii&k)];d=a+(mem[oo+((ii+1)&k)]-a)*fr;v=x-g2*d;
        mem[oo+(w&k)]=v;x=d+g2*v;
        mem[dO[i]+(w&dM[i])]=x+dn;
      }
      for(let i=1;i<K;i++)if(i!==hK){const k=dM[i];mem[dO[i]+((w+injK[i])&k)]+=sl*injL[i]+sr*injR[i]}
      dn=-dn;
      let ll=0,lr=0;
      for(let j=0;j<nT2;j++)ll+=mem[tO[j]+((w-tD[j])&tM[j])]*tGa[j];
      for(let j=nT2;j<nT;j++)lr+=mem[tO[j]+((w-tD[j])&tM[j])]*tGa[j];
      ll*=lG;lr*=lG;
      if(shim>0)this.shift((ll+lr)*.5);
      toL[t]=el*eG+ll;toR[t]=er*eG+lr;
      w=w+1>=0x40000000?0x100000:w+1;
    }
    this.w=w;this.dn=dn;
    for(let i=0;i<K;i++){const n=Math.hypot(mc[i],ms[i]);mc[i]/=n;ms[i]/=n}
  }
  // shimmer: two crossfaded grains read at the pitch ratio, lowpassed and soft-limited, fed back into the ring
  shift(x){
    const b=this.shBuf,M=this.shSize-1,W=this.shWin,w=this.shW;b[w&M]=x;
    let p=this.shP+(1-this.shRatio)/W;p-=Math.floor(p);this.shP=p;let q=p+.5;if(q>=1)q-=1;
    let pos=w-p*W-1,i=Math.floor(pos),f=pos-i;const ya=b[i&M]*(1-f)+b[(i+1)&M]*f;
    pos=w-q*W-1;i=Math.floor(pos);f=pos-i;const yb=b[i&M]*(1-f)+b[(i+1)&M]*f;
    const sa=Math.sin(Math.PI*p),sb=Math.sin(Math.PI*q),y=ya*sa*sa+yb*sb*sb;
    this.shW=(this.shW+1)&0x3fffffff;this.shLp+=this.shA*(y-this.shLp);this.shOut=this.shLp/(1+Math.abs(this.shLp));
  }
}
