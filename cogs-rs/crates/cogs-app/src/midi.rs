//! MIDI bytes from any keyboard, parsed into note and controller events for the audio thread.
//! The platform side (Kotlin on Android) pushes raw messages; the audio callback drains them every block.

#[derive(Clone, Copy, Debug)]
pub enum Midi {
    On { note: u8, vel: u8 },
    Off { note: u8 },
    Cc { cc: u8, value: u8 },
    Bend { value: i16 },
}

/// One complete message (status, data1, data2) into an event; None for what we don't play.
pub fn decode(st: u8, d1: u8, d2: u8) -> Option<Midi> {
    match st & 0xf0 {
        0x90 if d2 > 0 => Some(Midi::On { note: d1, vel: d2 }),
        0x80 | 0x90 => Some(Midi::Off { note: d1 }),
        0xb0 => Some(Midi::Cc { cc: d1, value: d2 }),
        0xe0 => Some(Midi::Bend { value: ((d1 as i16) | ((d2 as i16) << 7)) - 8192 }),
        _ => None,
    }
}
