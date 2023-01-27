use std::env;
use std::error::Error;
use crate::midi::{InputDevice, OutputDevice};
use crate::arpeggiator::{Pattern, Arpeggiator, timed, synced};

mod midi;
mod arpeggio;
mod arpeggiator;

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let mode = args.next().expect("The first argument should be the arpeggiator mode: repeat, pedal");
    let midi_in = args.next().expect("The second argument should be the MIDI IN device file");
    let midi_out = args.next().expect("The third argument should be the MIDI OUT device file");
    let mut midi_clock = || args.next().expect("The fourth argument should be the MIDI CLOCK device file");
    let mut arp: Box<dyn Arpeggiator> = match mode.as_str() {
        "repeat" => Box::new(timed::RepeatRecorder::new(
            InputDevice::open(&midi_in, false)?,
            OutputDevice::open(&midi_out)?
        )),
        "pedal" => Box::new(timed::PedalRecorder::new(
            InputDevice::open(&midi_in, false)?,
            OutputDevice::open(&midi_out)?
        )),
        "clock-down" => Box::new(synced::PressHold::new(
            InputDevice::open_with_external_clock(&midi_in, &midi_clock())?,
            OutputDevice::open(&midi_out)?,
            Pattern::Down,
            true
        )),
        "clock-up" => Box::new(synced::PressHold::new(
            InputDevice::open_with_external_clock(&midi_in, &midi_clock())?,
            OutputDevice::open(&midi_out)?,
            Pattern::Up,
            true
        )),
        _ => panic!("Invalid arpeggiator mode: {}", mode)
    };
    arp.listen();//TODO make this stop on ESC pressed (or any key?)
    Ok(())
}