package help.techmate.cogs

/** What the parser hands on. Channel is 0-15; the app listens on all channels (omni). */
interface MidiSink {
    fun noteOn(ch: Int, note: Int, velocity: Int)
    fun noteOff(ch: Int, note: Int)
    fun controlChange(ch: Int, cc: Int, value: Int)
    fun pitchBend(ch: Int, value14: Int)
}

/**
 * A byte-stream MIDI 1.0 parser: running status, realtime bytes interleaved anywhere
 * (BLE MIDI keyboards send clock and active sensing), SysEx skipped, system-common
 * messages clearing running status. Android may split or merge messages across
 * onSend() calls, so state carries over between [feed] calls. One parser per port.
 */
class MidiParser(private val sink: MidiSink) {
    private var status = 0
    private var need = 0
    private var idx = 0
    private val data = IntArray(2)
    private var inSysex = false

    fun feed(bytes: ByteArray, offset: Int, count: Int) { for (i in offset until offset + count) feed(bytes[i].toInt() and 0xFF) }

    fun feed(b: Int) {
        if (b >= 0xF8) return // realtime: clock, start/stop, active sensing, reset
        if (b >= 0x80) {
            when {
                b == 0xF0 -> { inSysex = true; status = 0 }
                b == 0xF7 -> { inSysex = false; status = 0 }
                b >= 0xF1 -> { inSysex = false; status = 0 } // system common: its data bytes are dropped
                else -> {
                    inSysex = false; status = b; idx = 0
                    need = when (b and 0xF0) { 0xC0, 0xD0 -> 1; else -> 2 }
                }
            }
            return
        }
        if (inSysex || status == 0) return
        data[idx++] = b
        if (idx < need) return
        idx = 0 // keep status: running status
        val ch = status and 0x0F
        when (status and 0xF0) {
            0x90 -> if (data[1] == 0) sink.noteOff(ch, data[0]) else sink.noteOn(ch, data[0], data[1])
            0x80 -> sink.noteOff(ch, data[0])
            0xB0 -> sink.controlChange(ch, data[0], data[1])
            0xE0 -> sink.pitchBend(ch, data[0] or (data[1] shl 7))
        }
    }
}

