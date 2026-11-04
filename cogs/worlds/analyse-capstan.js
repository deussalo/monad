// Reads a render from render-capstan.js: level and band energy per block of bars, clipping, width, the events by source,
// and a spectrogram PNG. node analyse-capstan.js <file.f32> <fromBar> <barsPerRow> [bpm] [out.png]
const fs = require('fs'), zlib = require('zlib');
const [file, from_ = '0', per_ = '8', bpm_ = '119', png] = process.argv.slice(2);
const b = fs.readFileSync(file), x = new Float32Array(b.buffer, b.byteOffset, b.length >> 2), N = x.length >> 1, FS = 48000, from = +from_, per = +per_, bpm = +bpm_;
const barS = Math.round(4 * 60 / bpm * FS), db = v => v > 1e-9 ? (10 * Math.log10(v)).toFixed(1) : '-inf';
// one-pole band splits: <90, 90-300, 300-2k, 2k+
const co = f => 1 - Math.exp(-2 * Math.PI * f / FS);
const c90 = co(90), c300 = co(300), c2k = co(2000); let l1 = 0, l2 = 0, l3 = 0, peak = 0, clip = 0, tot = 0;
console.log('bars        rms    <90   -300   -2k    2k+   peak   side/mid');
let e = [0, 0, 0, 0, 0], pk = 0, S = 0, M = 0;
const line = i => { const n = barS * per; console.log(`${String(from + (i / barS) - per).padStart(3)}-${String(from + i / barS).padEnd(5)}`, ...e.map(v => db(v / n).padStart(6)), (20 * Math.log10(pk + 1e-9)).toFixed(1).padStart(6), (M > 0 ? Math.sqrt(S / M) : 0).toFixed(2).padStart(6)); e = [0, 0, 0, 0, 0]; pk = 0; S = 0; M = 0; };
for (let i = 0; i < N; i++) {
  const L = x[2 * i], R = x[2 * i + 1], m = (L + R) / 2, s = (L - R) / 2; const a = Math.max(Math.abs(L), Math.abs(R)); if (a > peak) peak = a; if (a > pk) pk = a; if (a >= 1) clip++;
  tot += m * m + s * s; l1 += c90 * (m - l1); l2 += c300 * (m - l2); l3 += c2k * (m - l3);
  const b0 = l1, b1 = l2 - l1, b2 = l3 - l2, b3 = m - l3; e[0] += m * m + s * s; e[1] += b0 * b0; e[2] += b1 * b1; e[3] += b2 * b2; e[4] += b3 * b3; M += m * m; S += s * s;
  if ((i + 1) % (barS * per) === 0) line(i + 1);
}
console.log('secs', (N / FS).toFixed(1), 'peak', (20 * Math.log10(peak)).toFixed(2), 'dBFS, overall rms', db(tot / N), 'dB; samples at or past 0 dBFS:', clip);
try { const ev = JSON.parse(fs.readFileSync(file + '.events.json')).events, by = {};
  const name = s => s.replace(/cog(\d+)\.ring\d+>/, '').replace(/arm\d+>/, '').replace(/ball\d+\.\d+/, 'rain');
  for (const [t, s] of ev) (by[name(s)] = by[name(s)] || []).push(t);
  console.log('events by part:', Object.entries(by).map(([k, v]) => `${k}:${v.length}`).join(' '));
} catch (e) { console.log('no events'); }
if (png) { // spectrogram: log frequency 30 Hz-16 kHz, 240 rows, 1 column per 1/8 bar at most 1600
  const W_ = Math.min(1600, Math.floor(N / 2048)), H = 240, hop = Math.floor(N / W_), F = 4096, win = Float64Array.from({ length: F }, (_, i) => .5 - .5 * Math.cos(2 * Math.PI * i / F)), img = Buffer.alloc((W_ * 3 + 1) * H);
  const re = new Float64Array(F), im = new Float64Array(F);
  const fft = () => { for (let i = 0, j = 0; i < F; i++) { if (i < j) { let t = re[i]; re[i] = re[j]; re[j] = t; t = im[i]; im[i] = im[j]; im[j] = t; } let m = F >> 1; while (m >= 1 && j & m) { j ^= m; m >>= 1; } j |= m; }
    for (let L = 2; L <= F; L <<= 1) { const a = -2 * Math.PI / L, wr = Math.cos(a), wi = Math.sin(a); for (let i = 0; i < F; i += L) { let cr = 1, ci = 0; for (let k = 0; k < L / 2; k++) { const u = i + k, v = u + L / 2, tr = re[v] * cr - im[v] * ci, ti = re[v] * ci + im[v] * cr; re[v] = re[u] - tr; im[v] = im[u] - ti; re[u] += tr; im[u] += ti; const n = cr * wr - ci * wi; ci = cr * wi + ci * wr; cr = n; } } } };
  for (let c = 0; c < W_; c++) { const s0 = Math.min(c * hop, N - F); for (let i = 0; i < F; i++) { re[i] = (x[2 * (s0 + i)] + x[2 * (s0 + i) + 1]) * .5 * win[i]; im[i] = 0; } fft();
    for (let r = 0; r < H; r++) { const f = 30 * Math.pow(16000 / 30, 1 - r / (H - 1)), bin = Math.max(1, Math.round(f * F / FS)), hw = Math.max(0, Math.round(bin * .02)); let m = 0; for (let q = bin - hw; q <= bin + hw; q++) m = Math.max(m, re[q] * re[q] + im[q] * im[q]);
      const v = Math.max(0, Math.min(1, (10 * Math.log10(m / (F * F) + 1e-14) + 95) / 75)), off = r * (W_ * 3 + 1) + 1 + c * 3; img[off] = Math.floor(255 * Math.min(1, v * 1.8)); img[off + 1] = Math.floor(255 * Math.max(0, v * 1.8 - .6) / 1.2); img[off + 2] = Math.floor(255 * Math.max(0, .6 - Math.abs(v - .3)) * .9); } }
  for (let r = 0; r < H; r++) img[r * (W_ * 3 + 1)] = 0;
  // a tick above each 16 bars
  const crcT = Array.from({ length: 256 }, (_, n) => { let c = n; for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1; return c >>> 0; }), crc = bf => { let c = ~0; for (const v of bf) c = crcT[(c ^ v) & 255] ^ (c >>> 8); return (~c) >>> 0; };
  const chunk = (t, d) => { const l = Buffer.alloc(4); l.writeUInt32BE(d.length); const td = Buffer.concat([Buffer.from(t), d]), cr = Buffer.alloc(4); cr.writeUInt32BE(crc(td)); return Buffer.concat([l, td, cr]); };
  const ih = Buffer.alloc(13); ih.writeUInt32BE(W_, 0); ih.writeUInt32BE(H, 4); ih[8] = 8; ih[9] = 2;
  fs.writeFileSync(png, Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk('IHDR', ih), chunk('IDAT', zlib.deflateSync(img)), chunk('IEND', Buffer.alloc(0))]));
  console.log('wrote', png, W_, H);
}
