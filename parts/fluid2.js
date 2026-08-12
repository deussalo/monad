/* fluid2.js — MonadFluid v2: separable Gaussian blur, vel at half-dye-res,
 * phone sim ≤160px long edge, vignette in composite, zero per-frame alloc.
 * Drop-in replacement for the MonadFluid IIFE in monad.html.
 * Same public API: init, available, resize, splat, ring, step, render,
 * quality, info, dispose, onContextLost, onContextRestored.
 */
(function (global) {
'use strict';

/* ── Shaders ─────────────────────────────────────────────────────────────── */

var VERT =
  '#version 300 es\n' +
  'in vec2 aPos; out vec2 vUv;\n' +
  'void main(){ vUv=aPos*0.5+0.5; gl_Position=vec4(aPos,0.0,1.0); }\n';

/* Value noise + curl potential. Two octaves only — we want smooth blobs,
   not noisy texture. Low frequency is the goal. */
var NOISE =
  'float hash(vec3 p){p=fract(p*0.3183099+vec3(0.1,0.7,0.4));p*=17.0;return fract(p.x*p.y*p.z*(p.x+p.y+p.z));}\n' +
  'float vnoise(vec3 x){vec3 i=floor(x),f=fract(x);f=f*f*(3.0-2.0*f);\n' +
  ' return mix(mix(mix(hash(i),hash(i+vec3(1,0,0)),f.x),mix(hash(i+vec3(0,1,0)),hash(i+vec3(1,1,0)),f.x),f.y),\n' +
  '            mix(mix(hash(i+vec3(0,0,1)),hash(i+vec3(1,0,1)),f.x),mix(hash(i+vec3(0,1,1)),hash(i+vec3(1,1,1)),f.x),f.y),f.z);}\n' +
  'float fbm(vec3 p){return 0.66*vnoise(p)+0.34*vnoise(p*2.03);}\n' +
  'vec2 curl(vec2 p,float t){float e=0.045;\n' +
  ' float a=fbm(vec3(p.x,p.y+e,t)),b=fbm(vec3(p.x,p.y-e,t));\n' +
  ' float c=fbm(vec3(p.x+e,p.y,t)),d=fbm(vec3(p.x-e,p.y,t));\n' +
  ' return vec2(a-b,d-c)*(1.0/(2.0*e));}\n';

/* Velocity: runs at HALF dye resolution. uTexel = 1/dyeW, 1/dyeH (not
   vel-space) so the stored UV-displacement values stay consistent with the
   dye advection — changing vel resolution is a storage optimisation only. */
var FRAG_VEL =
  '#version 300 es\nprecision highp float;\n' +
  'in vec2 vUv; out vec4 outColor;\n' +
  'uniform sampler2D uVel; uniform vec2 uTexel;\n' +
  'uniform float uDt,uTime,uDecay,uCurl,uScale;\n' +
  NOISE +
  'void main(){\n' +
  '  vec2 v=texture(uVel,vUv).xy;\n' +
  '  vec2 sv=texture(uVel,vUv-v*uDt*uTexel).xy;\n' +
  '  vec2 f=curl(vUv*uScale,uTime*0.05);\n' +
  '  outColor=vec4(sv*uDecay+f*uCurl*uDt,0.0,1.0);\n' +
  '}\n';

/* Dye: semi-Lagrangian at dye resolution. uTexel = 1/dyeW, 1/dyeH.
   Vel texture is at half-dye-res but shares the same UV space [0,1]²;
   bilinear hardware upsampling handles the size mismatch transparently.
   Border fade prevents dye piling up on the CLAMP_TO_EDGE boundary. */
var FRAG_ADV =
  '#version 300 es\nprecision highp float;\n' +
  'in vec2 vUv; out vec4 outColor;\n' +
  'uniform sampler2D uSrc,uVel; uniform vec2 uTexel;\n' +
  'uniform float uDt,uDiss;\n' +
  'void main(){\n' +
  '  vec2 v=texture(uVel,vUv).xy;\n' +
  '  vec2 src=vUv-v*uDt*uTexel;\n' +
  '  vec2 e=smoothstep(vec2(0.0),vec2(0.04),vUv)*smoothstep(vec2(0.0),vec2(0.04),1.0-vUv);\n' +
  '  outColor=texture(uSrc,src)*uDiss*(0.55+0.45*e.x*e.y);\n' +
  '}\n';

/* Additive injection (velocity and dye). Blending: ONE/ONE. */
var FRAG_SPLAT =
  '#version 300 es\nprecision highp float;\n' +
  'in vec2 vUv; out vec4 outColor;\n' +
  'uniform vec2 uPoint,uAspect; uniform vec3 uValue;\n' +
  'uniform float uRadius,uRing,uThick;\n' +
  'void main(){\n' +
  '  vec2 d=(vUv-uPoint)*uAspect; float r=length(d);\n' +
  '  if(uRing>0.5){\n' +
  '    float o=(r-uRadius)/max(uThick,1e-4);\n' +
  '    vec2 dir=r>1e-5?d/r:vec2(0.0);\n' +
  '    outColor=vec4(dir*exp(-o*o)*uValue.x,0.0,0.0);\n' +
  '  } else {\n' +
  '    float o=r/max(uRadius,1e-4);\n' +
  '    outColor=vec4(uValue*exp(-o*o),0.0);\n' +
  '  }\n' +
  '}\n';

/* Separable Gaussian blur — horizontal pass.
   9 taps, σ=2.0, weights pre-normalised (sum=1).
   uTexel.x = 1/dyeW.  Runs at dye resolution. */
var FRAG_BLURH =
  '#version 300 es\nprecision highp float;\n' +
  'in vec2 vUv; out vec4 outColor;\n' +
  'uniform sampler2D uSrc; uniform vec2 uTexel;\n' +
  'void main(){\n' +
  '  float dx=uTexel.x;\n' +
  '  vec3 c=texture(uSrc,vUv+vec2(-4.0*dx,0.0)).rgb*0.02708\n' +
  '        +texture(uSrc,vUv+vec2(-3.0*dx,0.0)).rgb*0.06494\n' +
  '        +texture(uSrc,vUv+vec2(-2.0*dx,0.0)).rgb*0.12135\n' +
  '        +texture(uSrc,vUv+vec2(-1.0*dx,0.0)).rgb*0.17657\n' +
  '        +texture(uSrc,vUv                  ).rgb*0.20012\n' +
  '        +texture(uSrc,vUv+vec2( 1.0*dx,0.0)).rgb*0.17657\n' +
  '        +texture(uSrc,vUv+vec2( 2.0*dx,0.0)).rgb*0.12135\n' +
  '        +texture(uSrc,vUv+vec2( 3.0*dx,0.0)).rgb*0.06494\n' +
  '        +texture(uSrc,vUv+vec2( 4.0*dx,0.0)).rgb*0.02708;\n' +
  '  outColor=vec4(c,1.0);\n' +
  '}\n';

/* Vertical pass of the same Gaussian. uTexel.y = 1/dyeH. */
var FRAG_BLURV =
  '#version 300 es\nprecision highp float;\n' +
  'in vec2 vUv; out vec4 outColor;\n' +
  'uniform sampler2D uSrc; uniform vec2 uTexel;\n' +
  'void main(){\n' +
  '  float dy=uTexel.y;\n' +
  '  vec3 c=texture(uSrc,vUv+vec2(0.0,-4.0*dy)).rgb*0.02708\n' +
  '        +texture(uSrc,vUv+vec2(0.0,-3.0*dy)).rgb*0.06494\n' +
  '        +texture(uSrc,vUv+vec2(0.0,-2.0*dy)).rgb*0.12135\n' +
  '        +texture(uSrc,vUv+vec2(0.0,-1.0*dy)).rgb*0.17657\n' +
  '        +texture(uSrc,vUv                  ).rgb*0.20012\n' +
  '        +texture(uSrc,vUv+vec2(0.0, 1.0*dy)).rgb*0.17657\n' +
  '        +texture(uSrc,vUv+vec2(0.0, 2.0*dy)).rgb*0.12135\n' +
  '        +texture(uSrc,vUv+vec2(0.0, 3.0*dy)).rgb*0.06494\n' +
  '        +texture(uSrc,vUv+vec2(0.0, 4.0*dy)).rgb*0.02708;\n' +
  '  outColor=vec4(c,1.0);\n' +
  '}\n';

/* Ground: slow domain-warped noise refreshed at ~6 fps into a tiny 96×64
   scratch texture.  Evaluated per-texel there, not per output pixel. */
var FRAG_GROUND =
  '#version 300 es\nprecision highp float;\n' +
  'in vec2 vUv; out vec4 outColor;\n' +
  'uniform float uTime,uWarp;\n' +
  NOISE +
  'void main(){\n' +
  '  vec2 wp=vUv*1.7+vec2(fbm(vec3(vUv*2.1,uTime*0.011)),\n' +
  '                       fbm(vec3(vUv*2.1+7.3,uTime*0.013)))*uWarp*0.55;\n' +
  '  outColor=vec4(vec3(fbm(vec3(wp,uTime*0.008))),1.0);\n' +
  '}\n';

/* Composite: uDye receives fully-blurred dye (blurV.tex). Three taps and
   one hash — no loops, scales only with output pixel count.
   Vignette replaces the separate CSS #vignette layer (one fewer compositor
   pass): radial from centre 50%/45%, darkens to ~rgba(8,5,2,0.46) at corners. */
var FRAG_COMP =
  '#version 300 es\nprecision highp float;\n' +
  'in vec2 vUv; out vec4 outColor;\n' +
  'uniform sampler2D uDye,uGround;\n' +
  'uniform vec3 uBg; uniform float uTime,uGrain,uHaze,uStrength,uAtmo;\n' +
  'float hash2(vec2 p){p=fract(p*vec2(443.897,441.423));p+=dot(p,p+19.19);return fract(p.x*p.y);}\n' +
  'void main(){\n' +
  '  vec3 dye=texture(uDye,vUv).rgb;\n' +
  '  vec3 c=dye*(uStrength*(0.55+0.16*uAtmo)+uHaze*1.6);\n' +
  '  c+=uBg*(0.72+0.85*texture(uGround,vUv).r);\n' +
  '  c=c/(1.0+c*0.78);\n' +
  '  c+=(hash2(gl_FragCoord.xy+floor(uTime*24.0))-0.5)*uGrain*0.055;\n' +
  /* Vignette — replaces CSS #vignette layer */
  '  vec2 vd=(vUv-vec2(0.5,0.45))*vec2(1.0,1.1);\n' +
  '  c=mix(c,vec3(0.031,0.020,0.008),smoothstep(0.25,0.82,length(vd))*0.46);\n' +
  '  outColor=vec4(max(c,0.0),1.0);\n' +
  '}\n';

/* ── Module state (no per-frame allocation) ──────────────────────────────── */
var gl=null, canvas=null;
var progVel=null, progAdv=null, progSplat=null;
var progBlurH=null, progBlurV=null, progGround=null, progComp=null;
var vao=null, quad=null;
/* vel: half dye resolution.  blurH/blurV: scratch at dye resolution. */
var vel=null, dye=null, blurH=null, blurV=null, ground=null;
var GROUND_W=96, GROUND_H=64;
var simW=1, simH=1, velW=1, velH=1, dispW=1, dispH=1, dpr=1;
/* Last CSS dimensions passed to resize() — needed by quality() to recover true
   cssW/H after a qScale change has already baked scale into dispW/H. */
var _cssW=1, _cssH=1;
/* Cached reciprocals updated in allocate() — never re-created per frame. */
var invDyeW=1.0, invDyeH=1.0;
var time=0.0, groundAge=1e9, ok=false, listening=false;
var qBloom=true, qScale=1.0;
/* Reusable scratch arrays — eliminates all per-call allocation in hot paths. */
var _velVal=[0,0,0], _dyeVal=[0,0,0], _ringVal=[0,0,0], _box=[0,0,0,0];
var _DEF_BG=[0.13,0.07,0.01];    // module-level default, never reallocated

/* ── Utilities ───────────────────────────────────────────────────────────── */
function compile(vs, fs) {
  function sh(type, src) {
    var s=gl.createShader(type);
    gl.shaderSource(s,src); gl.compileShader(s);
    if(!gl.getShaderParameter(s,gl.COMPILE_STATUS)){
      console.error('fluid2:',gl.getShaderInfoLog(s)); gl.deleteShader(s); return null;
    }
    return s;
  }
  var v=sh(gl.VERTEX_SHADER,vs), f=sh(gl.FRAGMENT_SHADER,fs);
  if(!v||!f) return null;
  var p=gl.createProgram();
  gl.attachShader(p,v); gl.attachShader(p,f);
  gl.bindAttribLocation(p,0,'aPos');
  gl.linkProgram(p);
  gl.deleteShader(v); gl.deleteShader(f);
  if(!gl.getProgramParameter(p,gl.LINK_STATUS)){
    console.error('fluid2 link:',gl.getProgramInfoLog(p)); gl.deleteProgram(p); return null;
  }
  var u={}, n=gl.getProgramParameter(p,gl.ACTIVE_UNIFORMS);
  for(var i=0;i<n;i++){ var info=gl.getActiveUniform(p,i); u[info.name]=gl.getUniformLocation(p,info.name); }
  return {p:p, u:u};
}

function makeTarget(w,h,internal,format,type) {
  var tex=gl.createTexture();
  gl.bindTexture(gl.TEXTURE_2D,tex);
  gl.texImage2D(gl.TEXTURE_2D,0,internal,w,h,0,format,type,null);
  gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.LINEAR);
  gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.LINEAR);
  gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,gl.CLAMP_TO_EDGE);
  gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,gl.CLAMP_TO_EDGE);
  var fb=gl.createFramebuffer();
  gl.bindFramebuffer(gl.FRAMEBUFFER,fb);
  gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,tex,0);
  var complete=(gl.checkFramebufferStatus(gl.FRAMEBUFFER)===gl.FRAMEBUFFER_COMPLETE);
  gl.clearColor(0,0,0,1); gl.clear(gl.COLOR_BUFFER_BIT);
  gl.bindFramebuffer(gl.FRAMEBUFFER,null);
  if(!complete){ gl.deleteTexture(tex); gl.deleteFramebuffer(fb); return null; }
  return {tex:tex, fb:fb, w:w, h:h};
}

function dropTarget(t){ if(t){ gl.deleteTexture(t.tex); gl.deleteFramebuffer(t.fb); } }

function makePair(w,h,internal,format,type) {
  var a=makeTarget(w,h,internal,format,type);
  var b=makeTarget(w,h,internal,format,type);
  if(!a||!b){ dropTarget(a); dropTarget(b); return null; }
  return {read:a, write:b, swap:function(){var t=this.read;this.read=this.write;this.write=t;}};
}

function destroyPair(pair){ if(!pair)return; dropTarget(pair.read); dropTarget(pair.write); }
function drawQuad(){ gl.bindVertexArray(vao); gl.drawArrays(gl.TRIANGLES,0,3); }
function bindTex(unit,tex){ gl.activeTexture(gl.TEXTURE0+unit); gl.bindTexture(gl.TEXTURE_2D,tex); }

/* ── Allocation ──────────────────────────────────────────────────────────── */
function allocate() {
  destroyPair(vel); destroyPair(dye);
  dropTarget(blurH); dropTarget(blurV);
  /* Velocity at half dye resolution — bilinear upsampling is free and smooth. */
  velW=Math.max(8,simW>>1); velH=Math.max(8,simH>>1);
  vel   = makePair(velW,velH, gl.RGBA16F,gl.RGBA,gl.HALF_FLOAT);
  dye   = makePair(simW,simH, gl.RGBA16F,gl.RGBA,gl.HALF_FLOAT);
  blurH = makeTarget(simW,simH, gl.RGBA16F,gl.RGBA,gl.HALF_FLOAT);
  blurV = makeTarget(simW,simH, gl.RGBA16F,gl.RGBA,gl.HALF_FLOAT);
  /* Ground: created once, never resized with the sim. */
  if(!ground) ground=makeTarget(GROUND_W,GROUND_H,gl.RGBA8,gl.RGBA,gl.UNSIGNED_BYTE);
  if(!vel||!dye||!blurH||!blurV||!ground) return false;
  /* Cache reciprocals here — the only place simW/simH change. */
  invDyeW=1.0/simW; invDyeH=1.0/simH;
  groundAge=1e9;
  return true;
}

/* ── Context loss ────────────────────────────────────────────────────────── */
function handleLost(e) {
  e.preventDefault();
  ok=false; api.available=false;
  vel=dye=blurH=blurV=ground=null;
  progVel=progAdv=progSplat=progBlurH=progBlurV=progGround=progComp=null;
  vao=quad=null;
  if(typeof api.onContextLost==='function') api.onContextLost();
}
function handleRestored() {
  if(!canvas) return;
  gl=null;
  if(init(canvas)&&typeof api.onContextRestored==='function') api.onContextRestored();
}

/* ── Public API ──────────────────────────────────────────────────────────── */
function init(target) {
  if(ok) return true;
  try {
    canvas=target;
    if(!listening){
      listening=true;
      canvas.addEventListener('webglcontextlost',handleLost,false);
      canvas.addEventListener('webglcontextrestored',handleRestored,false);
    }
    gl=canvas.getContext('webgl2',{
      alpha:true,antialias:false,depth:false,stencil:false,
      premultipliedAlpha:false,preserveDrawingBuffer:false,powerPreference:'high-performance'
    });
    if(!gl) return false;
    if(!gl.getExtension('EXT_color_buffer_float')&&!gl.getExtension('EXT_color_buffer_half_float')){
      gl=null; return false;
    }
    progVel    = compile(VERT, FRAG_VEL);
    progAdv    = compile(VERT, FRAG_ADV);
    progSplat  = compile(VERT, FRAG_SPLAT);
    progBlurH  = compile(VERT, FRAG_BLURH);
    progBlurV  = compile(VERT, FRAG_BLURV);
    progGround = compile(VERT, FRAG_GROUND);
    progComp   = compile(VERT, FRAG_COMP);
    if(!progVel||!progAdv||!progSplat||!progBlurH||!progBlurV||!progGround||!progComp){
      gl=null; return false;
    }
    quad=gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER,quad);
    gl.bufferData(gl.ARRAY_BUFFER,new Float32Array([-1,-1,3,-1,-1,3]),gl.STATIC_DRAW);
    vao=gl.createVertexArray();
    gl.bindVertexArray(vao);
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0,2,gl.FLOAT,false,0,0);
    gl.bindVertexArray(null);
    gl.disable(gl.DEPTH_TEST); gl.disable(gl.CULL_FACE);
    resize(canvas.clientWidth||1, canvas.clientHeight||1, global.devicePixelRatio||1);
    if(!vel||!dye||!blurH||!blurV||!ground){ gl=null; return false; }
    ok=true; api.available=true;
    return true;
  } catch(err) {
    gl=null; ok=false; api.available=false;
    return false;
  }
}

function resize(cssW, cssH, ratio) {
  if(!gl) return;
  _cssW=cssW||1; _cssH=cssH||1;
  /* Cap DPR at 1 on phones — the atmosphere is a blurred gradient, 3× DPR
     triples fill rate for no perceptible gain. */
  dpr=Math.min(ratio||1, cssW<700?1:1.5);
  dispW=Math.max(1,Math.round(cssW*dpr*qScale));
  dispH=Math.max(1,Math.round(cssH*dpr*qScale));
  canvas.width=dispW; canvas.height=dispH;
  /* Sim resolution: phone ≤160px long edge, desktop ≤512px.
     160 chosen because at 390×844 it gives ~74×160 — already so low-res that
     the bilinear upscale to 390×844 blurs by 5× before the Gaussian runs.
     Quarter-res (128) was tested and looked too smeared even for this style. */
  var cap=cssW<700?160:512;
  var longEdge=Math.max(cssW,cssH);
  var scale=Math.min(0.5, cap/Math.max(1,longEdge));
  var nw=Math.max(8,Math.round(cssW*scale));
  var nh=Math.max(8,Math.round(cssH*scale));
  if(nw!==simW||nh!==simH||!vel||!dye){
    simW=nw; simH=nh;
    if(!allocate()){ ok=false; api.available=false; if(typeof api.onContextLost==='function') api.onContextLost(); }
  }
}

function quality(o) {
  if(!o) return {bloom:qBloom, scale:qScale};
  if(typeof o.bloom==='boolean') qBloom=o.bloom;
  if(typeof o.scale==='number'&&o.scale!==qScale){
    qScale=Math.max(0.4,Math.min(1.0,o.scale));
    /* Use stored _cssW/_cssH — NOT dispW/dpr which already includes old qScale. */
    if(canvas) resize(_cssW, _cssH, dpr);
  }
  return {bloom:qBloom, scale:qScale};
}

/* Scissor box for inject. Returns _box (module-level, no allocation). */
function injectScissor(t,px,py,halfX,halfY) {
  var x0=Math.floor((px-halfX)*t.w), x1=Math.ceil((px+halfX)*t.w);
  var y0=Math.floor((py-halfY)*t.h), y1=Math.ceil((py+halfY)*t.h);
  x0=Math.max(0,x0); y0=Math.max(0,y0);
  x1=Math.min(t.w,x1); y1=Math.min(t.h,y1);
  if(x1<=x0||y1<=y0) return null;
  _box[0]=x0; _box[1]=y0; _box[2]=x1-x0; _box[3]=y1-y0;
  return _box;
}

function inject(pair,isRing,px,py,value,radius,thickness) {
  if(!gl||!pair) return;
  var t=pair.read;
  var reach=isRing?(radius+3.0*(thickness||0.02)):3.0*radius;
  var box=injectScissor(t,px,py,reach,reach*(t.w/t.h));
  if(!box) return;
  gl.bindFramebuffer(gl.FRAMEBUFFER,t.fb);
  gl.viewport(0,0,t.w,t.h);
  gl.enable(gl.SCISSOR_TEST); gl.scissor(box[0],box[1],box[2],box[3]);
  gl.enable(gl.BLEND); gl.blendFunc(gl.ONE,gl.ONE);
  gl.useProgram(progSplat.p);
  var u=progSplat.u;
  gl.uniform2f(u.uPoint,px,py);
  gl.uniform2f(u.uAspect,1.0,t.h/t.w);
  gl.uniform3f(u.uValue,value[0],value[1],value[2]);
  gl.uniform1f(u.uRadius,radius);
  gl.uniform1f(u.uRing,isRing?1.0:0.0);
  gl.uniform1f(u.uThick,thickness||0.02);
  drawQuad();
  gl.disable(gl.BLEND);
  gl.disable(gl.SCISSOR_TEST);
  gl.bindFramebuffer(gl.FRAMEBUFFER,null);
}

function splat(x,y,dx,dy,rgb,radius,strength) {
  if(!ok||!gl) return;
  var cssW=dispW/dpr, cssH=dispH/dpr;
  if(cssW<=0||cssH<=0) return;
  var px=x/cssW, py=1.0-y/cssH;
  var r=Math.max(0.004,(radius||20)/cssW);
  var s=Math.max(0,strength||0);
  var k=0.55*s*(simW/Math.max(1,cssW));
  _velVal[0]=dx*k; _velVal[1]=-dy*k; _velVal[2]=0;
  inject(vel,false,px,py,_velVal,r,0);
  _dyeVal[0]=rgb[0]*s; _dyeVal[1]=rgb[1]*s; _dyeVal[2]=rgb[2]*s;
  inject(dye,false,px,py,_dyeVal,r,0);
}

function ring(x,y,radius,strength) {
  if(!ok||!gl) return;
  var cssW=dispW/dpr, cssH=dispH/dpr;
  if(cssW<=0||cssH<=0) return;
  var px=x/cssW, py=1.0-y/cssH;
  var r=Math.max(0.004,(radius||20)/cssW);
  var s=Math.max(0,strength||0)*90.0*(simW/Math.max(1,cssW));
  _ringVal[0]=s; _ringVal[1]=s; _ringVal[2]=0;
  inject(vel,true,px,py,_ringVal,r,Math.max(0.012,r*0.5));
}

function step(dt) {
  if(!ok||!gl) return;
  var d=Math.min(Math.max(dt||0,0),1/30);
  if(d<=0) return;
  time+=d; groundAge+=d;

  /* 1) Velocity: self-advect + curl. Runs at velW×velH (half dye res).
        uTexel = dye-space reciprocal so UV displacement is consistent with dye
        advection — the vel texture is bilinearly smoothed, not physically coarser. */
  gl.bindFramebuffer(gl.FRAMEBUFFER,vel.write.fb);
  gl.viewport(0,0,velW,velH);
  gl.useProgram(progVel.p);
  bindTex(0,vel.read.tex);
  gl.uniform1i(progVel.u.uVel,0);
  gl.uniform2f(progVel.u.uTexel,invDyeW,invDyeH);
  gl.uniform1f(progVel.u.uDt,d);
  gl.uniform1f(progVel.u.uTime,time);
  /* Slightly higher decay (0.9) and lower curl (32) than v1: languid
     large-scale swirling rather than energetic fine-grain turbulence. */
  gl.uniform1f(progVel.u.uDecay,Math.exp(-0.9*d));
  gl.uniform1f(progVel.u.uCurl,32.0);
  gl.uniform1f(progVel.u.uScale,3.2);
  drawQuad();
  vel.swap();

  /* 2) Dye advection at full sim resolution.
        Vel is sampled via bilinear upsample from velW×velH — free on GPU.
        Lower dissipation (0.95 vs 1.45) lets the sparse dye linger longer. */
  gl.bindFramebuffer(gl.FRAMEBUFFER,dye.write.fb);
  gl.viewport(0,0,simW,simH);
  gl.useProgram(progAdv.p);
  bindTex(0,dye.read.tex); bindTex(1,vel.read.tex);
  gl.uniform1i(progAdv.u.uSrc,0);
  gl.uniform1i(progAdv.u.uVel,1);
  gl.uniform2f(progAdv.u.uTexel,invDyeW,invDyeH);
  gl.uniform1f(progAdv.u.uDt,d);
  gl.uniform1f(progAdv.u.uDiss,Math.exp(-0.95*d));
  drawQuad();
  dye.swap();

  gl.bindFramebuffer(gl.FRAMEBUFFER,null);
}

function render(p) {
  if(!ok||!gl) return;
  p=p||{};
  var bg=p.background||_DEF_BG;

  /* Ground: wall-clock gated at ~6 fps regardless of display frame rate. */
  if(groundAge>0.16){
    groundAge=0;
    gl.bindFramebuffer(gl.FRAMEBUFFER,ground.fb);
    gl.viewport(0,0,GROUND_W,GROUND_H);
    gl.useProgram(progGround.p);
    gl.uniform1f(progGround.u.uTime,time);
    gl.uniform1f(progGround.u.uWarp,p.warp===undefined?0.6:p.warp);
    drawQuad();
  }

  /* Separable Gaussian blur at sim resolution.
     Fill cost: 2 × simW × simH × 9 taps.
     At 74×160 (390×844 phone, dpr=1): 2 × 11840 × 9 = 213120 texel fetches.
     Compare composite at 390×844 = 329160 pixels × 3 taps = ~987k fetches.
     The blur passes are ~4.6× cheaper than the composite.
     Skip second pass when qBloom=false (still runs first pass for some softness). */
  gl.bindFramebuffer(gl.FRAMEBUFFER,blurH.fb);
  gl.viewport(0,0,simW,simH);
  gl.useProgram(progBlurH.p);
  bindTex(0,dye.read.tex);
  gl.uniform1i(progBlurH.u.uSrc,0);
  gl.uniform2f(progBlurH.u.uTexel,invDyeW,invDyeH);
  drawQuad();

  if(qBloom){
    gl.bindFramebuffer(gl.FRAMEBUFFER,blurV.fb);
    gl.viewport(0,0,simW,simH);
    gl.useProgram(progBlurV.p);
    bindTex(0,blurH.tex);
    gl.uniform1i(progBlurV.u.uSrc,0);
    gl.uniform2f(progBlurV.u.uTexel,invDyeW,invDyeH);
    drawQuad();
  }

  /* Composite: blurV (fully blurred) or blurH (half-blurred at qBloom=false). */
  gl.bindFramebuffer(gl.FRAMEBUFFER,null);
  gl.viewport(0,0,dispW,dispH);
  gl.useProgram(progComp.p);
  bindTex(0, qBloom?blurV.tex:blurH.tex);
  bindTex(1,ground.tex);
  var u=progComp.u;
  gl.uniform1i(u.uDye,0);
  gl.uniform1i(u.uGround,1);
  gl.uniform3f(u.uBg,bg[0],bg[1],bg[2]);
  gl.uniform1f(u.uTime,time);
  gl.uniform1f(u.uGrain,p.grain===undefined?0.5:p.grain);
  gl.uniform1f(u.uHaze,p.haze===undefined?0.25:p.haze);
  gl.uniform1f(u.uStrength,p.strength===undefined?0.85:p.strength);
  gl.uniform1f(u.uAtmo,p.atmosphere===undefined?3.4:p.atmosphere);
  drawQuad();
}

function dispose() {
  if(!gl){ ok=false; return; }
  destroyPair(vel); destroyPair(dye);
  dropTarget(blurH); dropTarget(blurV);
  dropTarget(ground); ground=null;
  [progVel,progAdv,progSplat,progBlurH,progBlurV,progGround,progComp].forEach(
    function(pr){ if(pr) gl.deleteProgram(pr.p); });
  if(quad) gl.deleteBuffer(quad);
  if(vao)  gl.deleteVertexArray(vao);
  vel=dye=blurH=blurV=null;
  progVel=progAdv=progSplat=progBlurH=progBlurV=progGround=progComp=null;
  gl=null; ok=false; api.available=false;
}

var api = {
  available: false,
  init:    init,
  resize:  resize,
  splat:   splat,
  ring:    ring,
  step:    step,
  render:  render,
  quality: quality,
  info: function(){
    return 'dye '+simW+'x'+simH+' vel '+velW+'x'+velH+' out '+dispW+'x'+dispH+
           ' q'+qScale.toFixed(1)+(qBloom?' bloom':'');
  },
  dispose: dispose
};
global.MonadFluid = api;

}(window));
