/* ---------- the master's line amp: after Pulsar Modular's P42 Climax ---------- */
// In spirit, not a copy: an input drive into a transformer whose core saturates the lows, a Class A stage that adds even
// harmonics, a tube-like saturation (mostly odd), boost-only low and high shelves, and the Air band, a shelf high up with an
// exciter beside it: the band above Air's corner, normalised by its own envelope, is bent through a curve and high-passed
// again, so it grows new harmonics at any level, not only when it is loud. The drive is compensated after the
// nonlinearities, so turning it changes the colour and not the level (P42's SHIFT+drive). Everything nonlinear and the
// mix run at twice the sample rate, so the harmonics are not folded back down and the dry stays phase-matched with the wet.
// Included in both scripts: the worklet runs Climax, the page reads the knob list and the defaults.
// [key, label, min, max, step]
const CLIMAX_DEFS=[['on','line amp',0,1,1],['drive','drive dB',-6,12,.1],['sat','saturation %',0,150,1],['classA','class A',0,1,1],
  ['excite','excite',0,10,.1],['air','air',0,10,.1],['airFreq','air Hz',5000,16000,100],
  ['low','low boost',0,10,.1],['lowFreq','low Hz',40,500,1],['high','high boost',0,10,.1],['highFreq','high Hz',150,5000,10],
  ['mix','mix',0,1,.01],['out','out dB',-24,6,.1]];
const CLIMAX_DEFAULT={on:1,drive:2,sat:30,classA:1,excite:3,air:2,airFreq:10000,low:0,lowFreq:80,high:0,highFreq:3000,mix:1,out:0};
// A saved line amp, clamped to its ranges; a world from before it takes the defaults.
function climaxIn(j){
  const out={...CLIMAX_DEFAULT};if(!j||typeof j!=='object')return out;
  for(const[k,,lo,hi]of CLIMAX_DEFS)if(Number.isFinite(+j[k]))out[k]=Math.min(hi,Math.max(lo,+j[k]));
  return out;
}

// RBJ biquads, transposed direct form II; state per channel.
class ClxBiquad{
  constructor(){this.b0=1;this.b1=0;this.b2=0;this.a1=0;this.a2=0;this.z=new Float64Array(4)}
  set(type,fs,f,q,db=0){
    const w=2*Math.PI*Math.min(f,.49*fs)/fs,c=Math.cos(w),s=Math.sin(w),A=Math.pow(10,db/40);
    let b0,b1,b2,a0,a1,a2;
    if(type==='lp'){const al=s/(2*q);b0=(1-c)/2;b1=1-c;b2=b0;a0=1+al;a1=-2*c;a2=1-al}
    else if(type==='hp'){const al=s/(2*q);b0=(1+c)/2;b1=-(1+c);b2=b0;a0=1+al;a1=-2*c;a2=1-al}
    else{const al=s/2*Math.sqrt((A+1/A)*(1/q-1)+2),sq=2*Math.sqrt(A)*al;
      if(type==='ls'){b0=A*((A+1)-(A-1)*c+sq);b1=2*A*((A-1)-(A+1)*c);b2=A*((A+1)-(A-1)*c-sq);a0=(A+1)+(A-1)*c+sq;a1=-2*((A-1)+(A+1)*c);a2=(A+1)+(A-1)*c-sq}
      else{b0=A*((A+1)+(A-1)*c+sq);b1=-2*A*((A-1)+(A+1)*c);b2=A*((A+1)+(A-1)*c-sq);a0=(A+1)-(A-1)*c+sq;a1=2*((A-1)-(A+1)*c);a2=(A+1)-(A-1)*c-sq}}
    this.b0=b0/a0;this.b1=b1/a0;this.b2=b2/a0;this.a1=a1/a0;this.a2=a2/a0;
  }
  run(x,ch){const z=this.z,i=ch*2,y=this.b0*x+z[i];z[i]=this.b1*x-this.a1*y+z[i+1];z[i+1]=this.b2*x-this.a2*y;return y}
}
// Shelf slope (S=1) as the q the shelves above take.
const CLX_SHELF=1;
// tanh, as a rational: within 2% and a fraction of the cost, flat at +-1 past 3.
const clxTanh=x=>x>3?1:x<-3?-1:x*(27+x*x)/(27+9*x*x);

class Climax{
  constructor(fs){
    this.fs=fs;const f2=this.f2=fs*2;
    // 2x: a 4th-order Butterworth low-pass at 0.45 of the base rate, each way
    this.up=[new ClxBiquad(),new ClxBiquad()];this.dn=[new ClxBiquad(),new ClxBiquad()];
    for(const[k,q]of [[0,.5412],[1,1.3066]]){this.up[k].set('lp',f2,.45*fs,q);this.dn[k].set('lp',f2,.45*fs,q)}
    this.lowS=new ClxBiquad();this.highS=new ClxBiquad();this.airS=new ClxBiquad();
    this.exHp1=new ClxBiquad();this.exHp2=new ClxBiquad();this.exHp3=new ClxBiquad();
    // the transformer's low band (one pole at 150 Hz), the exciter's envelope, the DC blocker
    this.tfA=1-Math.exp(-2*Math.PI*150/f2);this.tf=new Float64Array(2);
    this.envA=1-Math.exp(-1/(.004*f2));this.envR=1-Math.exp(-1/(.06*f2));this.env=new Float64Array(2);
    this.dcA=1-Math.exp(-2*Math.PI*8/fs);this.dc=new Float64Array(2);
    // the level match (P42's GCC, always on): the colour stage's power in and out over ~300 ms, both channels together
    this.gcA=1-Math.exp(-1/(.3*f2));this.msIn=1e-9;this.msOut=1e-9;this.gc=1;
    this.p={...CLIMAX_DEFAULT};this.cur=null;this.byp=1;this.key='';
    this.set(CLIMAX_DEFAULT);
  }
  set(p){for(const[k,,lo,hi]of CLIMAX_DEFS)if(Number.isFinite(+p[k]))this.p[k]=Math.min(hi,Math.max(lo,+p[k]))}
  // Gains glide a block at a time toward their targets; the filters follow the same glide.
  glide(){
    const p=this.p,t={drive:Math.pow(10,p.drive/20),sat:p.sat/100,classA:p.classA,excite:p.excite,air:p.air,airFreq:p.airFreq,low:p.low,lowFreq:p.lowFreq,
      high:p.high,highFreq:p.highFreq,mix:p.mix,out:Math.pow(10,p.out/20),on:p.on?1:0};
    if(!this.cur){this.cur={...t};this.byp=t.on}
    const c=this.cur;let moved=false;
    for(const k in t){const d=t[k]-c[k];if(Math.abs(d)>1e-6){c[k]+=Math.abs(d)<1e-4?d:d*.25;moved=true}}
    if(moved||!this.key){const f2=this.f2;this.key='1';
      // boost-only 0..10 as 0..+12 dB for the shelves, 0..+9 dB for Air
      this.lowS.set('ls',f2,c.lowFreq,CLX_SHELF,c.low*1.2);this.highS.set('hs',f2,c.highFreq,CLX_SHELF,c.high*1.2);
      this.airS.set('hs',f2,c.airFreq,CLX_SHELF,c.air*.9);
      this.exHp1.set('hp',f2,c.airFreq*.5,.707);this.exHp2.set('hp',f2,c.airFreq*.5,.707);this.exHp3.set('hp',f2,c.airFreq*.7,.707)}
    return c;
  }
  // One sample of the wet path at 2x, channel ch.
  wet(x,ch,c){
    const g=c.drive,x0=x;let y=x*g;
    // the transformer: its low band saturates first and hardest
    const tf=this.tf;tf[ch]+=(y-tf[ch])*this.tfA;const lo=tf[ch];
    y+=clxTanh(lo*1.4)/1.4-lo;
    // Class A: a little second harmonic, more as it is driven
    if(c.classA>.001)y+=c.classA*.12*y*y/(1+.25*y*y);
    // saturation: tanh(k y)/k is unity when quiet and rounds the peaks, k from the knob (0..150%)
    if(c.sat>.001){const k=c.sat*2.2;y=clxTanh(k*y)/k}
    y/=g;
    // matched in level to what came in, so drive and saturation change the colour and not the loudness
    this.msIn+=(x0*x0-this.msIn)*this.gcA;this.msOut+=(y*y-this.msOut)*this.gcA;y*=this.gc;
    if(c.low>.01)y=this.lowS.run(y,ch);if(c.high>.01)y=this.highS.run(y,ch);
    // the exciter: the band above Air, at unit level against its own envelope, bent, then only what is new kept
    if(c.excite>.001){
      const h=this.exHp2.run(this.exHp1.run(y,ch),ch),e=this.env,a=Math.abs(h);e[ch]+=(a-e[ch])*(a>e[ch]?this.envA:this.envR);
      let u=h/(e[ch]+1e-5);u=u>4?4:u<-4?-4:u;const bent=(clxTanh(1.8*u)/1.8-u+.3*u*u)*e[ch];
      y+=this.exHp3.run(bent,ch)*c.excite*.25;
    }
    return c.air>.01?this.airS.run(y,ch):y;
  }
  // In place, on a stereo block.
  run(L,R,n){
    const c=this.glide(),on=c.on;
    if(on<.001&&this.byp<.001)return;
    const up=this.up,dn=this.dn,mix=c.mix,dc=this.dc,dcA=this.dcA,out=c.out;
    for(let s=0;s<n;s++){
      this.byp+=(on-this.byp)*.004;const b=this.byp;
      if((s&15)===0){const m=this.msIn>1e-8?Math.sqrt(this.msIn/Math.max(this.msOut,1e-12)):1;this.gc+=(Math.min(4,Math.max(.25,m))-this.gc)*.05}
      for(let ch=0;ch<2;ch++){
        const x=ch?R[s]:L[s];let o=0;
        for(let k=0;k<2;k++){
          const v=up[1].run(up[0].run(k?0:2*x,ch),ch);
          const m=v+(this.wet(v,ch,c)-v)*mix;
          const d=dn[1].run(dn[0].run(m,ch),ch);if(k===0)o=d;
        }
        // the DC the even harmonics leave, taken off
        dc[ch]+=(o-dc[ch])*dcA;o=(o-dc[ch])*out;
        const r=x+(o-x)*b;if(ch)R[s]=r;else L[s]=r;
      }
    }
  }
}
// The worklet's side: one stereo in, one stereo out; settings arrive as {type:'climax', ...}.
if(typeof registerProcessor==='function')registerProcessor('cogs-climax',class extends AudioWorkletProcessor{
  constructor(){super();this.clx=new Climax(sampleRate);this.port.onmessage=e=>{if(e.data.type==='climax')this.clx.set(e.data)}}
  process(inputs,outputs){
    const i=inputs[0],o=outputs[0],L=o[0],R=o[1]||o[0];
    if(!i||!i.length){L.fill(0);R.fill(0);return true}
    L.set(i[0]);R.set(i[1]||i[0]);this.clx.run(L,R,L.length);return true;
  }
});
