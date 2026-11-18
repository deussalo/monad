package help.techmate.cogs

/** The Rust side (libcogs.so, crate cogs-app): every MIDI message goes straight to its audio thread. */
object Native {
    @JvmStatic external fun midi(status: Int, d1: Int, d2: Int)
    @JvmStatic external fun status(text: String, connected: Boolean)
    /** The activity came to the front. */
    @JvmStatic external fun resumed()
    /** A file the user picked to open, as text (null: cancelled or unreadable, with why). */
    @JvmStatic external fun fileText(text: String?, error: String?)
}

/** Hands each parsed message to Rust: one JNI call, no allocation. */
class NativeSink : MidiSink {
    override fun noteOn(ch: Int, note: Int, velocity: Int) = Native.midi(0x90 or ch, note, velocity)
    override fun noteOff(ch: Int, note: Int) = Native.midi(0x80 or ch, note, 0)
    override fun controlChange(ch: Int, cc: Int, value: Int) = Native.midi(0xB0 or ch, cc, value)
    override fun pitchBend(ch: Int, value14: Int) = Native.midi(0xE0 or ch, value14 and 0x7F, value14 shr 7)
}
