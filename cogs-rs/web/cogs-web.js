// The page's glue for the web build: loads the app (cogs.js + cogs_bg.wasm, wasm-bindgen no-modules) into the canvas,
// starts the sound on the first tap (an AudioWorklet running worklet.wasm), keeps the sound's clock for the app's
// heard time, and brings MIDI in: Web MIDI, and Web Bluetooth straight to a BLE-MIDI keyboard (cogs/src/midi.js).
// A single-file build carries the two wasm files and worklet.js inline (window.COGS_INLINE); a folder fetches them.
(() => {
  const BLE_MIDI_SERVICE = '03b80e5a-ede8-4b33-a751-6ce34ec4c700', BLE_MIDI_CHAR = '7772e5db-3868-4112-a1a9-f2669d106bf3';
  const enc = new TextEncoder(), dec = new TextDecoder();
  const inline = window.COGS_INLINE || null;
  const b64 = s => Uint8Array.from(atob(s), c => c.charCodeAt(0));
  const bytes = async name => inline ? b64(inline[name]) : new Uint8Array(await (await fetch(name)).arrayBuffer());
  const cogs = window.cogs = {
    ctx: null, node: null, st: null, inbox: [], live: false, starting: null, err: '', midiMsg: '', devices: new Map(), keys: 0,
    send(s) { if (this.live) this.node.port.postMessage(enc.encode(s)); },
    poll() { return this.inbox.length ? this.inbox.shift() : undefined; },
    ready() { return this.live && !!this.st; },
    heard() {
      const c = this.ctx, s = this.st; if (!c || !s || !this.live) return -1;
      let a = c.currentTime - (c.outputLatency || 0) - (c.baseLatency || 0);
      const ts = c.getOutputTimestamp && c.getOutputTimestamp();
      if (ts && ts.contextTime > 0 && ts.performanceTime > 0) a = ts.contextTime + Math.max(0, performance.now() - ts.performanceTime) / 1000;
      return Math.min(s.t, s.t - (s.ct - a));
    },
    status() {
      const c = this.ctx, names = [...this.devices.values()];
      const audio = this.err ? 'sound failed: ' + this.err : !c ? 'no sound yet: tap anywhere' :
        `sound ${c.sampleRate} Hz · ${(((c.outputLatency || 0) + (c.baseLatency || 0)) * 1000).toFixed(0)} ms out · ${c.state}`;
      return audio + (names.length ? ' · keyboard: ' + names.join(', ') + ` (${this.keys} keys)` : this.midiMsg ? ' · MIDI: ' + this.midiMsg : '');
    },
    buzz(ms) { try { navigator.vibrate && navigator.vibrate(ms); } catch (e) {} },
    async start() {
      if (this.starting) return this.starting;
      return this.starting = (async () => {
        try {
          const c = this.ctx = new (window.AudioContext || window.webkitAudioContext)({ latencyHint: 'interactive' });
          const src = inline ? 'data:text/javascript;charset=utf-8,' + encodeURIComponent(inline['worklet.js']) : 'worklet.js';
          await c.audioWorklet.addModule(src);
          const wasm = await bytes('worklet.wasm');
          const n = this.node = new AudioWorkletNode(c, 'cogs', { numberOfInputs: 0, numberOfOutputs: 1, outputChannelCount: [2], processorOptions: { wasm } });
          n.port.onmessage = e => {
            const s = dec.decode(e.data);
            if (s.startsWith('{"type":"ready"')) { this.live = true; return; }
            try { const m = JSON.parse(s); this.st = { t: m.t, ct: m.ct }; this.last = m; } catch (err) {}
            this.inbox.push(s); if (this.inbox.length > 64) this.inbox.splice(0, this.inbox.length - 64);
          };
          n.onprocessorerror = () => { this.err = 'the sound stopped (processor error)'; this.live = false; };
          n.connect(c.destination);
          await c.resume();
        } catch (e) { this.err = String(e && e.message || e); }
      })();
    },
    midi(st, d1, d2) { if ((st & 0xf0) === 0x90 && d2 > 0) this.keys++; this.send(`{"type":"midi","d":[${st},${d1},${d2}]}`); },
    // BLE-MIDI packets: a header byte, then timestamped messages; running status and several messages per packet allowed
    bleMidi(d) {
      let i = 1, status = 0;
      while (i < d.length) {
        if (d[i] & 0x80) { i++; if (i < d.length && d[i] & 0x80) { status = d[i]; i++; } }
        if (status === 0xf0) { while (i < d.length && d[i] !== 0xf7) i++; i++; status = 0; continue; }
        if (!status || status >= 0xf0) { i++; continue; }
        const n = (status & 0xe0) === 0xc0 ? 1 : 2; if (i + n > d.length) break;
        this.midi(status, d[i], n > 1 ? d[i + 1] : 0); i += n;
      }
    },
    async connectBluetooth() {
      if (!navigator.bluetooth) { this.midiMsg = 'Bluetooth MIDI is not available in this browser'; return; }
      try {
        const dev = await navigator.bluetooth.requestDevice({ filters: [{ services: [BLE_MIDI_SERVICE] }] });
        if (this.devices.has('ble:' + dev.id)) return;
        this.midiMsg = 'connecting ' + (dev.name || 'device') + '…';
        const server = await dev.gatt.connect(), svc = await server.getPrimaryService(BLE_MIDI_SERVICE), ch = await svc.getCharacteristic(BLE_MIDI_CHAR);
        ch.addEventListener('characteristicvaluechanged', e => this.bleMidi(new Uint8Array(e.target.value.buffer, e.target.value.byteOffset, e.target.value.byteLength)));
        await ch.startNotifications();
        this.devices.set('ble:' + dev.id, dev.name || 'Bluetooth MIDI');
        dev.addEventListener('gattserverdisconnected', () => { this.devices.delete('ble:' + dev.id); this.send('{"type":"midi","d":[176,123,0]}'); this.midiMsg = (dev.name || 'device') + ' disconnected'; });
      } catch (e) { this.midiMsg = e.name === 'NotFoundError' ? 'no device chosen' : 'Bluetooth failed: ' + e.message; }
    },
    async connectWebMidi() {
      if (!navigator.requestMIDIAccess) { this.midiMsg = 'Web MIDI is not available in this browser'; return; }
      try {
        const acc = this.access = this.access || await navigator.requestMIDIAccess();
        const hook = () => {
          for (const k of [...this.devices.keys()]) if (k.startsWith('midi:')) this.devices.delete(k);
          for (const inp of acc.inputs.values()) { if (inp.state !== 'connected') continue; this.devices.set('midi:' + inp.id, inp.name || 'MIDI input');
            inp.onmidimessage = e => { const d = e.data; if (d.length) this.midi(d[0], d[1] || 0, d[2] || 0); }; }
        };
        acc.onstatechange = hook; hook();
        if (![...this.devices.keys()].some(k => k.startsWith('midi:'))) this.midiMsg = 'no MIDI inputs yet';
      } catch (e) { this.midiMsg = 'MIDI is blocked here'; }
    },
    // One button: a phone's keyboard is a Bluetooth one; on a computer, what is plugged in, else Bluetooth.
    async connectMidi() {
      this.start();
      if (navigator.bluetooth && matchMedia('(pointer:coarse)').matches) return this.connectBluetooth();
      await this.connectWebMidi();
      if (navigator.bluetooth && ![...this.devices.keys()].some(k => k.startsWith('midi:'))) this.connectBluetooth();
    },
  };
  // the sound may only start from a gesture
  for (const ev of ['pointerdown', 'keydown', 'touchend']) addEventListener(ev, () => cogs.start(), { capture: true });
  (async () => {
    await wasm_bindgen(inline ? { module_or_path: b64(inline['cogs_bg.wasm']) } : { module_or_path: 'cogs_bg.wasm' });
    wasm_bindgen.start('cogs');
  })();
})();
