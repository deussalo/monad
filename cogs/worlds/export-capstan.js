// Turns a render from render-capstan.js into a finished file: an 8 s fade at the end, then 16-bit WAV (TPDF dither) and
// Ogg Opus at 192 kbps through the browser's own encoder. node export-capstan.js <in.f32> <out-stem>
const { chromium } = require(process.env.COGS_PW || '/srv/rig/rig/webapp/node_modules/.pnpm/playwright-core@1.64.0/node_modules/playwright-core');
const fs = require('fs');
const [inp, stem] = process.argv.slice(2), FS = 48000, FADE = 8 * FS;
const b = fs.readFileSync(inp), x = new Float32Array(b.buffer, b.byteOffset, b.length >> 2), N = x.length >> 1;
for (let i = 0; i < FADE; i++) { const g = Math.cos((i / FADE) * Math.PI / 2) ** 2, s = N - FADE + i; x[2 * s] *= g; x[2 * s + 1] *= g; }
// WAV
const pcm = Buffer.alloc(N * 4); let k = 0;
for (let i = 0; i < x.length; i++) { const d = (Math.random() - Math.random()) / 32768, v = Math.max(-1, Math.min(32767 / 32768, x[i] + d)); pcm.writeInt16LE(Math.round(v * 32768), k); k += 2; }
const h = Buffer.alloc(44); h.write('RIFF', 0); h.writeUInt32LE(36 + pcm.length, 4); h.write('WAVEfmt ', 8); h.writeUInt32LE(16, 16); h.writeUInt16LE(1, 20); h.writeUInt16LE(2, 22); h.writeUInt32LE(FS, 24); h.writeUInt32LE(FS * 4, 28); h.writeUInt16LE(4, 32); h.writeUInt16LE(16, 34); h.write('data', 36); h.writeUInt32LE(pcm.length, 40);
fs.writeFileSync(stem + '.wav', Buffer.concat([h, pcm]));
// Ogg
const T = Array.from({ length: 256 }, (_, i) => { let r = i << 24; for (let j = 0; j < 8; j++) r = r & 0x80000000 ? (r << 1) ^ 0x04c11db7 : r << 1; return r >>> 0; });
const crc = u => { let c = 0; for (const v of u) c = ((c << 8) ^ T[((c >>> 24) ^ v) & 255]) >>> 0; return c; };
let seq = 0; const SERIAL = 0x43415053;
function page(pkts, gran, flags) {
  const lace = []; for (const p of pkts) { let n = p.length; while (n >= 255) { lace.push(255); n -= 255; } lace.push(n); }
  const o = Buffer.alloc(27 + lace.length); o.write('OggS', 0); o[5] = flags; o.writeBigInt64LE(BigInt(gran), 6); o.writeUInt32LE(SERIAL, 14); o.writeUInt32LE(seq++, 18); o[26] = lace.length; Buffer.from(lace).copy(o, 27);
  const r = Buffer.concat([o, ...pkts]); r.writeUInt32LE(crc(r), 22); return r;
}
(async () => {
  const br = await chromium.launch({ executablePath: process.env.MONAD_CHROME || '/srv/rig/.cache/ms-playwright/chromium-1248/chrome-linux64/chrome', args: ['--no-sandbox'] });
  const p = await br.newPage(); const pk = [];
  // AudioEncoder needs a secure context, and a file: page is one
  fs.writeFileSync(stem + '.enc.html', '<!doctype html>');
  await p.goto('file://' + require('path').resolve(stem + '.enc.html'));
  fs.unlinkSync(stem + '.enc.html');
  await p.exposeFunction('__pkt', (s) => { pk.push(Buffer.from(s, 'base64')); });
  await p.exposeFunction('__pcm', (at, n) => { const u = Buffer.from(x.buffer, x.byteOffset + at * 8, n * 8); return u.toString('base64'); });
  const ok = await p.evaluate(async ({ N, FS }) => {
    if (typeof AudioEncoder === 'undefined') return 'no AudioEncoder (needs a secure context)';
    const cfg = { codec: 'opus', sampleRate: FS, numberOfChannels: 2, bitrate: 192000 };
    const sup = await AudioEncoder.isConfigSupported(cfg); if (!sup.supported) return 'opus unsupported';
    const e = new AudioEncoder({ output: ch => { const u = new Uint8Array(ch.byteLength); ch.copyTo(u); let t = ''; for (const v of u) t += String.fromCharCode(v); __pkt(btoa(t)); }, error: er => { throw er; } });
    e.configure(cfg);
    const CH = FS * 5;
    for (let at = 0; at < N; at += CH) { const n = Math.min(CH, N - at), s = atob(await __pcm(at, n)), u = new Uint8Array(s.length); for (let i = 0; i < s.length; i++) u[i] = s.charCodeAt(i);
      e.encode(new AudioData({ format: 'f32', sampleRate: FS, numberOfFrames: n, numberOfChannels: 2, timestamp: Math.round(at / FS * 1e6), data: u })); while (e.encodeQueueSize > 8) await new Promise(r => setTimeout(r, 20)); }
    await e.flush(); e.close(); return 'ok';
  }, { N, FS });
  await new Promise(r => setTimeout(r, 500));
  await br.close();
  if (ok !== 'ok') { console.log('opus skipped:', ok); return; }
  const head = Buffer.alloc(19); head.write('OpusHead', 0); head[8] = 1; head[9] = 2; head.writeUInt16LE(312, 10); head.writeUInt32LE(FS, 12);
  const vend = 'cogs', title = 'TITLE=Capstan', tags = Buffer.alloc(8 + 4 + vend.length + 4 + 4 + title.length); tags.write('OpusTags', 0); tags.writeUInt32LE(vend.length, 8); tags.write(vend, 12); tags.writeUInt32LE(1, 16 + vend.length); tags.writeUInt32LE(title.length, 20 + vend.length); tags.write(title, 24 + vend.length);
  const parts = [page([head], 0, 2), page([tags], 0, 0)]; const FRAME = 960; let g = 0, cur = [], size = 0;   // 20 ms packets
  pk.forEach((q, i) => { cur.push(q); size += q.length; g += FRAME; const last = i === pk.length - 1;
    if (cur.length >= 50 || last) { parts.push(page(cur, last ? Math.min(g, N + 312) : g, last ? 4 : 0)); cur = []; size = 0; } });
  fs.writeFileSync(stem + '.ogg', Buffer.concat(parts));
  console.log('opus packets', pk.length, 'covers', (pk.length * FRAME / FS).toFixed(1), 's of', (N / FS).toFixed(1));
})().catch(e => { console.error(e); process.exit(1); });
