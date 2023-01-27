use std::env;
use std::error::Error;
use crate::midi::{InputDevice, OutputDevice};
use crate::arpeggiator::{Pattern, Arpeggiator, timed, synced};

mod midi;
mod arpeggio;
mod arpeggiator;

const REPEAT:&str = "repeat";
const PEDAL: &str = "pedal";
const CLOCK_DOWN: &str = "clock-down";
const CLOCK_UP: &str = "clock-up";
const MODES: [&str; 4] = [
    REPEAT,
    PEDAL,
    CLOCK_DOWN,
    CLOCK_UP
];

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let mode = args.next().ok_or(format!("The first argument should be the arpeggiator mode: {}", MODES.join(", ")))?;
    let midi_in = args.next().ok_or("The second argument should be the MIDI IN device file")?;
    let midi_out = args.next().ok_or("The third argument should be the MIDI OUT device file")?;
    let mut midi_clock = || args.next().ok_or("The fourth argument should be the MIDI CLOCK device file");
    let mut arp: Box<dyn Arpeggiator> = match mode.as_str() {
        REPEAT => Box::new(timed::RepeatRecorder::new(
            InputDevice::open(&midi_in, false)?,
            OutputDevice::open(&midi_out)?
        )),
        PEDAL => Box::new(timed::PedalRecorder::new(
            InputDevice::open(&midi_in, false)?,
            OutputDevice::open(&midi_out)?
        )),
        CLOCK_DOWN => Box::new(synced::PressHold::new(
            InputDevice::open_with_external_clock(&midi_in, &midi_clock()?)?,
            OutputDevice::open(&midi_out)?,
            Pattern::Down,
            true
        )),
        CLOCK_UP => Box::new(synced::PressHold::new(
            InputDevice::open_with_external_clock(&midi_in, &midi_clock()?)?,
            OutputDevice::open(&midi_out)?,
            Pattern::Up,
            true
        )),
        _ => panic!("Invalid arpeggiator mode: {}", mode)
    };
    arp.listen();//TODO make this stop on ESC pressed (or any key?)
    Ok(())
}