// The web build's sound: an AudioWorkletProcessor running cogs_worklet.wasm (crates/cogs-worklet), which owns the
// machine and the engine (link.rs). The wasm bytes come in processorOptions.wasm; messages from the page are UTF-8
// bytes (the worklet scope may have no TextEncoder), state goes back the same way, one message every 8 quanta.
class CogsProcessor extends AudioWorkletProcessor {
  constructor(o) {
    super();
    const x = this.x = new WebAssembly.Instance(new WebAssembly.Module(o.processorOptions.wasm), {}).exports;
    x.cogs_init(sampleRate);
    this.port.onmessage = e => {
      const b = e.data; if (!(b instanceof Uint8Array)) return;
      const p = x.cogs_inbox(b.length); new Uint8Array(x.memory.buffer, p, b.length).set(b); x.cogs_message(b.length);
    };
    this.port.postMessage(new Uint8Array([123, 34, 116, 121, 112, 101, 34, 58, 34, 114, 101, 97, 100, 121, 34, 125])); // {"type":"ready"}
  }
  process(inputs, outputs) {
    const o = outputs[0], x = this.x;
    const pl = x.cogs_block(currentTime), pr = x.cogs_right(), m = x.memory.buffer;
    o[0].set(new Float32Array(m, pl, 128)); if (o[1]) o[1].set(new Float32Array(m, pr, 128));
    const n = x.cogs_outbox_len();
    if (n) { this.port.postMessage(new Uint8Array(x.memory.buffer, x.cogs_outbox_ptr(), n).slice()); x.cogs_outbox_clear(); }
    return true;
  }
}
registerProcessor('cogs', CogsProcessor);
