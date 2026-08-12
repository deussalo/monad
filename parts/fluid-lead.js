/* MonadFluid — WebGL2 curl-noise advected dye for monad.html
 * window.MonadFluid. No imports, no external deps, no text.
 *
 * Bodies inject dye + velocity; pressure fronts push annular impulses; the
 * field advects itself through a divergence-free curl-noise flow so colours
 * smear and swirl instead of sitting still. The composite pass carries the
 * visual identity: soft-clipped dye over a slow domain-warped ground, lifted
 * by a cheap bloom and finished with animated hash grain.
 *
 * Passes are discrete named programs so a real pressure-projection solver can
 * replace the curl-noise velocity stage later without touching the rest.
 */
(function (global) {
  'use strict';

  var VERT =
    '#version 300 es\n' +
    'in vec2 aPos; out vec2 vUv;\n' +
    'void main(){ vUv = aPos*0.5+0.5; gl_Position = vec4(aPos,0.0,1.0); }\n';

  /* Shared noise: value noise + a scalar potential whose curl is, by
     construction, divergence-free in 2D. */
  var NOISE =
    'float hash(vec3 p){ p = fract(p*0.3183099+vec3(0.1,0.7,0.4));' +
    ' p *= 17.0; return fract(p.x*p.y*p.z*(p.x+p.y+p.z)); }\n' +
    'float vnoise(vec3 x){ vec3 i=floor(x), f=fract(x); f=f*f*(3.0-2.0*f);\n' +
    ' return mix(mix(mix(hash(i+vec3(0,0,0)),hash(i+vec3(1,0,0)),f.x),\n' +
    '                mix(hash(i+vec3(0,1,0)),hash(i+vec3(1,1,0)),f.x),f.y),\n' +
    '            mix(mix(hash(i+vec3(0,0,1)),hash(i+vec3(1,0,1)),f.x),\n' +
    '                mix(hash(i+vec3(0,1,1)),hash(i+vec3(1,1,1)),f.x),f.y),f.z); }\n' +
    'float fbm(vec3 p){ return 0.60*vnoise(p) + 0.28*vnoise(p*2.03) + 0.12*vnoise(p*4.11); }\n' +
    'vec2 curl(vec2 p, float t){ float e=0.045;\n' +
    ' float a=fbm(vec3(p.x, p.y+e, t)), b=fbm(vec3(p.x, p.y-e, t));\n' +
    ' float c=fbm(vec3(p.x+e, p.y, t)), d=fbm(vec3(p.x-e, p.y, t));\n' +
    ' return vec2(a-b, d-c)/(2.0*e); }\n';

  /* Velocity: advect by itself, decay, and re-inject the curl field. */
  var FRAG_VEL =
    '#version 300 es\nprecision highp float;\n' +
    'in vec2 vUv; out vec4 outColor;\n' +
    'uniform sampler2D uVel; uniform vec2 uTexel; uniform float uDt, uTime, uDecay, uCurl, uScale;\n' +
    NOISE +
    'void main(){\n' +
    '  vec2 v = texture(uVel, vUv).xy;\n' +
    '  vec2 src = vUv - v*uDt*uTexel;\n' +
    '  vec2 sv = texture(uVel, src).xy;\n' +
    '  vec2 f = curl(vUv*uScale, uTime*0.05);\n' +
    '  outColor = vec4(sv*uDecay + f*uCurl*uDt, 0.0, 1.0);\n' +
    '}\n';

  /* Dye: semi-lagrangian advection with gentle dissipation. */
  var FRAG_ADV =
    '#version 300 es\nprecision highp float;\n' +
    'in vec2 vUv; out vec4 outColor;\n' +
    'uniform sampler2D uSrc, uVel; uniform vec2 uTexel; uniform float uDt, uDiss;\n' +
    'void main(){\n' +
    '  vec2 v = texture(uVel, vUv).xy;\n' +
    '  vec2 src = vUv - v*uDt*uTexel;\n' +
    '  outColor = texture(uSrc, src) * uDiss;\n' +
    '}\n';

  /* Additive injection. Blending is ONE/ONE so this only emits the increment. */
  var FRAG_SPLAT =
    '#version 300 es\nprecision highp float;\n' +
    'in vec2 vUv; out vec4 outColor;\n' +
    'uniform vec2 uPoint, uAspect; uniform vec3 uValue; uniform float uRadius, uRing, uThick;\n' +
    'void main(){\n' +
    '  vec2 d = (vUv - uPoint) * uAspect;\n' +
    '  float r = length(d);\n' +
    '  float f;\n' +
    '  if (uRing > 0.5) {\n' +
    '    float o = (r - uRadius) / max(uThick, 1e-4);\n' +
    '    f = exp(-o*o);\n' +
    '    vec2 dir = r > 1e-5 ? d/r : vec2(0.0);\n' +
    '    outColor = vec4(dir*f*uValue.x, 0.0, 0.0);\n' +
    '  } else {\n' +
    '    float o = r / max(uRadius, 1e-4);\n' +
    '    f = exp(-o*o);\n' +
    '    outColor = vec4(uValue*f, 0.0);\n' +
    '  }\n' +
    '}\n';

  /* Ground: the slow domain-warped gradient. It is very low frequency and the
     only expensive maths in the whole chain, so it is rendered into a tiny
     texture a few times a second and sampled back bilinearly. Evaluating fbm
     per screen pixel cost more than everything else combined. */
  var FRAG_GROUND =
    '#version 300 es\nprecision highp float;\n' +
    'in vec2 vUv; out vec4 outColor;\n' +
    'uniform float uTime, uWarp;\n' +
    NOISE +
    'void main(){\n' +
    '  vec2 wp = vUv*1.7 + vec2(fbm(vec3(vUv*2.1, uTime*0.011)),\n' +
    '                           fbm(vec3(vUv*2.1+7.3, uTime*0.013)))*uWarp*0.55;\n' +
    '  outColor = vec4(vec3(fbm(vec3(wp, uTime*0.008))), 1.0);\n' +
    '}\n';

  /* Composite: the visual identity. Kept deliberately cheap — texture taps,
     one hash, no noise. */
  var FRAG_COMP =
    '#version 300 es\nprecision highp float;\n' +
    'in vec2 vUv; out vec4 outColor;\n' +
    'uniform sampler2D uDye, uGround; uniform vec2 uTexel;\n' +
    'uniform vec3 uBg; uniform float uTime, uGrain, uHaze, uStrength, uAtmo;\n' +
    'float hash2(vec2 p){ p = fract(p*vec2(443.897,441.423)); p += dot(p, p+19.19); return fract(p.x*p.y); }\n' +
    'void main(){\n' +
    // 9-tap lift: a cheap bloom that keeps everything soft-edged
    '  vec3 dye = texture(uDye, vUv).rgb;\n' +
    '  vec3 blur = vec3(0.0); float wsum = 0.0;\n' +
    '  for (int i=-1;i<=1;i++) for (int j=-1;j<=1;j++) {\n' +
    '    float w = (i==0&&j==0) ? 4.0 : 1.0;\n' +
    '    blur += texture(uDye, vUv + vec2(float(i),float(j))*uTexel*3.5).rgb * w; wsum += w; }\n' +
    '  blur /= wsum;\n' +
    '  vec3 c = dye + blur * uHaze * 2.2;\n' +
    '  c *= uStrength * (0.55 + 0.16*uAtmo);\n' +
    '  c += uBg * (0.72 + 0.85*texture(uGround, vUv).r);\n' +
    '  c = c / (1.0 + c*0.78);\n' +          // soft clip, no hard highlights
    '  c += (hash2(gl_FragCoord.xy + floor(uTime*24.0)) - 0.5) * uGrain * 0.055;\n' +
    '  outColor = vec4(max(c, 0.0), 1.0);\n' +
    '}\n';

  var gl = null, canvas = null;
  var progVel = null, progAdv = null, progSplat = null, progComp = null, progGround = null;
  var vao = null, quad = null;
  var vel = null, dye = null, ground = null;   // ping-pong pairs + ground scratch
  var GROUND_W = 96, GROUND_H = 64, groundAge = 1e9;
  var simW = 1, simH = 1, dispW = 1, dispH = 1, dpr = 1;
  var time = 0, ok = false;

  function compile(vsSrc, fsSrc) {
    function sh(type, src) {
      var s = gl.createShader(type);
      gl.shaderSource(s, src); gl.compileShader(s);
      if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) { gl.deleteShader(s); return null; }
      return s;
    }
    var v = sh(gl.VERTEX_SHADER, vsSrc), f = sh(gl.FRAGMENT_SHADER, fsSrc);
    if (!v || !f) return null;
    var p = gl.createProgram();
    gl.attachShader(p, v); gl.attachShader(p, f);
    gl.bindAttribLocation(p, 0, 'aPos');
    gl.linkProgram(p);
    gl.deleteShader(v); gl.deleteShader(f);
    if (!gl.getProgramParameter(p, gl.LINK_STATUS)) { gl.deleteProgram(p); return null; }
    var u = {}, n = gl.getProgramParameter(p, gl.ACTIVE_UNIFORMS);
    for (var i = 0; i < n; i++) {
      var info = gl.getActiveUniform(p, i);
      u[info.name] = gl.getUniformLocation(p, info.name);
    }
    return { p: p, u: u };
  }

  function makeTarget(w, h, internal, format, type) {
    var tex = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, tex);
    gl.texImage2D(gl.TEXTURE_2D, 0, internal, w, h, 0, format, type, null);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    var fb = gl.createFramebuffer();
    gl.bindFramebuffer(gl.FRAMEBUFFER, fb);
    gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, tex, 0);
    var complete = gl.checkFramebufferStatus(gl.FRAMEBUFFER) === gl.FRAMEBUFFER_COMPLETE;
    gl.clearColor(0, 0, 0, 1); gl.clear(gl.COLOR_BUFFER_BIT);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    if (!complete) return null;
    return { tex: tex, fb: fb, w: w, h: h };
  }

  function makePair(w, h, internal, format, type) {
    var a = makeTarget(w, h, internal, format, type);
    var b = makeTarget(w, h, internal, format, type);
    if (!a || !b) return null;
    return { read: a, write: b, swap: function () { var t = this.read; this.read = this.write; this.write = t; } };
  }

  function destroyPair(p) {
    if (!p) return;
    [p.read, p.write].forEach(function (t) {
      if (!t) return;
      gl.deleteTexture(t.tex); gl.deleteFramebuffer(t.fb);
    });
  }

  function drawQuad() { gl.bindVertexArray(vao); gl.drawArrays(gl.TRIANGLES, 0, 3); }

  function bindTex(unit, tex) { gl.activeTexture(gl.TEXTURE0 + unit); gl.bindTexture(gl.TEXTURE_2D, tex); }

  function allocate() {
    destroyPair(vel); destroyPair(dye);
    vel = makePair(simW, simH, gl.RGBA16F, gl.RGBA, gl.HALF_FLOAT);
    dye = makePair(simW, simH, gl.RGBA16F, gl.RGBA, gl.HALF_FLOAT);
    if (!ground) ground = makeTarget(GROUND_W, GROUND_H, gl.RGBA8, gl.RGBA, gl.UNSIGNED_BYTE);
    groundAge = 1e9;
    return !!(vel && dye && ground);
  }

  function init(target) {
    if (ok) return true;
    try {
      canvas = target;
      gl = canvas.getContext('webgl2', {
        alpha: true, antialias: false, depth: false, stencil: false,
        premultipliedAlpha: false, preserveDrawingBuffer: false, powerPreference: 'high-performance'
      });
      if (!gl) return false;
      // RGBA16F is only colour-renderable with this extension.
      if (!gl.getExtension('EXT_color_buffer_float') && !gl.getExtension('EXT_color_buffer_half_float')) {
        gl = null; return false;
      }
      progVel = compile(VERT, FRAG_VEL);
      progAdv = compile(VERT, FRAG_ADV);
      progSplat = compile(VERT, FRAG_SPLAT);
      progComp = compile(VERT, FRAG_COMP);
      progGround = compile(VERT, FRAG_GROUND);
      if (!progVel || !progAdv || !progSplat || !progComp || !progGround) { gl = null; return false; }

      quad = gl.createBuffer();
      gl.bindBuffer(gl.ARRAY_BUFFER, quad);
      gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 3, -1, -1, 3]), gl.STATIC_DRAW);
      vao = gl.createVertexArray();
      gl.bindVertexArray(vao);
      gl.enableVertexAttribArray(0);
      gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
      gl.bindVertexArray(null);

      gl.disable(gl.DEPTH_TEST); gl.disable(gl.CULL_FACE);
      resize(canvas.clientWidth || 1, canvas.clientHeight || 1, global.devicePixelRatio || 1);
      if (!vel || !dye) { gl = null; return false; }
      ok = true;
      api.available = true;
      return true;
    } catch (err) {
      gl = null; ok = false; api.available = false;
      return false;
    }
  }

  function resize(cssW, cssH, ratio) {
    if (!gl) return;
    dpr = Math.min(ratio || 1, 2);
    dispW = Math.max(1, Math.round(cssW * dpr));
    dispH = Math.max(1, Math.round(cssH * dpr));
    canvas.width = dispW; canvas.height = dispH;
    // Half resolution, capped; quarter on small screens. This is the budget.
    var cap = cssW < 700 ? 256 : 512;
    var long = Math.max(cssW, cssH);
    var scale = Math.min(0.5, cap / Math.max(1, long));
    var nw = Math.max(16, Math.round(cssW * scale));
    var nh = Math.max(16, Math.round(cssH * scale));
    if (nw !== simW || nh !== simH || !vel || !dye) {
      simW = nw; simH = nh;
      allocate();
    }
  }

  /* One additive quad into the given pair's read target. */
  function inject(pair, isRing, px, py, value, radius, thickness) {
    if (!gl || !pair) return;
    var t = pair.read;
    gl.bindFramebuffer(gl.FRAMEBUFFER, t.fb);
    gl.viewport(0, 0, t.w, t.h);
    gl.enable(gl.BLEND); gl.blendFunc(gl.ONE, gl.ONE);
    gl.useProgram(progSplat.p);
    var u = progSplat.u;
    gl.uniform2f(u.uPoint, px, py);
    gl.uniform2f(u.uAspect, 1.0, t.h / t.w);
    gl.uniform3f(u.uValue, value[0], value[1], value[2]);
    gl.uniform1f(u.uRadius, radius);
    gl.uniform1f(u.uRing, isRing ? 1.0 : 0.0);
    gl.uniform1f(u.uThick, thickness || 0.02);
    drawQuad();
    gl.disable(gl.BLEND);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
  }

  function splat(x, y, dx, dy, rgb, radius, strength) {
    if (!ok || !gl) return;
    var w = dispW / dpr, h = dispH / dpr;
    if (w <= 0 || h <= 0) return;
    var px = x / w, py = 1.0 - y / h;
    var r = Math.max(0.004, (radius || 20) / w);
    var s = Math.max(0, strength || 0);
    // velocity in sim px/s; the shader consumes texel-space offsets
    var k = 0.55 * s * (simW / Math.max(1, w));
    inject(vel, false, px, py, [dx * k, -dy * k, 0], r, 0);
    inject(dye, false, px, py, [rgb[0] * s, rgb[1] * s, rgb[2] * s], r, 0);
  }

  function ring(x, y, radius, strength) {
    if (!ok || !gl) return;
    var w = dispW / dpr, h = dispH / dpr;
    if (w <= 0 || h <= 0) return;
    var px = x / w, py = 1.0 - y / h;
    var r = Math.max(0.004, (radius || 20) / w);
    var s = Math.max(0, strength || 0) * 90.0 * (simW / Math.max(1, w));
    inject(vel, true, px, py, [s, s, 0], r, Math.max(0.012, r * 0.5));
  }

  function step(dt) {
    if (!ok || !gl) return;
    var d = Math.min(Math.max(dt || 0, 0), 1 / 30);
    if (d <= 0) return;
    time += d;
    var texel = [1 / simW, 1 / simH];

    // 1) velocity: self-advect, decay, re-inject curl
    gl.bindFramebuffer(gl.FRAMEBUFFER, vel.write.fb);
    gl.viewport(0, 0, simW, simH);
    gl.useProgram(progVel.p);
    bindTex(0, vel.read.tex);
    gl.uniform1i(progVel.u.uVel, 0);
    gl.uniform2f(progVel.u.uTexel, texel[0], texel[1]);
    gl.uniform1f(progVel.u.uDt, d);
    gl.uniform1f(progVel.u.uTime, time);
    gl.uniform1f(progVel.u.uDecay, Math.exp(-0.85 * d));
    gl.uniform1f(progVel.u.uCurl, 44.0);
    gl.uniform1f(progVel.u.uScale, 3.2);
    drawQuad();
    vel.swap();

    // 2) dye: advect through the velocity field
    gl.bindFramebuffer(gl.FRAMEBUFFER, dye.write.fb);
    gl.viewport(0, 0, simW, simH);
    gl.useProgram(progAdv.p);
    bindTex(0, dye.read.tex); bindTex(1, vel.read.tex);
    gl.uniform1i(progAdv.u.uSrc, 0);
    gl.uniform1i(progAdv.u.uVel, 1);
    gl.uniform2f(progAdv.u.uTexel, texel[0], texel[1]);
    gl.uniform1f(progAdv.u.uDt, d);
    gl.uniform1f(progAdv.u.uDiss, Math.exp(-1.45 * d));
    drawQuad();
    dye.swap();

    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
  }

  function render(p) {
    if (!ok || !gl) return;
    p = p || {};
    var bg = p.background || [0.13, 0.07, 0.01];

    // The ground drifts on a ~100s cycle; 6 Hz is far more than enough.
    groundAge += 1;
    if (groundAge > 10) {
      groundAge = 0;
      gl.bindFramebuffer(gl.FRAMEBUFFER, ground.fb);
      gl.viewport(0, 0, GROUND_W, GROUND_H);
      gl.useProgram(progGround.p);
      gl.uniform1f(progGround.u.uTime, time);
      gl.uniform1f(progGround.u.uWarp, p.warp === undefined ? 0.6 : p.warp);
      drawQuad();
    }

    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    gl.viewport(0, 0, dispW, dispH);
    gl.useProgram(progComp.p);
    bindTex(0, dye.read.tex); bindTex(1, ground.tex);
    var u = progComp.u;
    gl.uniform1i(u.uDye, 0);
    gl.uniform1i(u.uGround, 1);
    gl.uniform2f(u.uTexel, 1 / simW, 1 / simH);
    gl.uniform3f(u.uBg, bg[0], bg[1], bg[2]);
    gl.uniform1f(u.uTime, time);
    gl.uniform1f(u.uGrain, p.grain === undefined ? 0.5 : p.grain);
    gl.uniform1f(u.uHaze, p.haze === undefined ? 0.25 : p.haze);
    gl.uniform1f(u.uStrength, p.strength === undefined ? 0.85 : p.strength);
    gl.uniform1f(u.uAtmo, p.atmosphere === undefined ? 3.4 : p.atmosphere);
    drawQuad();
  }

  function dispose() {
    if (!gl) { ok = false; return; }
    destroyPair(vel); destroyPair(dye);
    if (ground) { gl.deleteTexture(ground.tex); gl.deleteFramebuffer(ground.fb); ground = null; }
    [progVel, progAdv, progSplat, progComp, progGround].forEach(function (pr) { if (pr) gl.deleteProgram(pr.p); });
    if (quad) gl.deleteBuffer(quad);
    if (vao) gl.deleteVertexArray(vao);
    vel = dye = null; progVel = progAdv = progSplat = progComp = progGround = null;
    gl = null; ok = false; api.available = false;
  }

  var api = {
    available: false,
    init: init,
    resize: resize,
    splat: splat,
    ring: ring,
    step: step,
    render: render,
    dispose: dispose
  };
  global.MonadFluid = api;

}(window));
