use std::env;
use std::error::Error;

use crate::settings::{StopArpeggio, FixedNotesPerStep};
use crate::midi::{InputDevice, OutputDevice};
use crate::arpeggiator::{Pattern, Arpeggiator, timed, synced};

mod midi;
mod arpeggio;
mod arpeggiator;
mod settings;

const REPEAT:&str = "repeat";
const PEDAL: &str = "pedal";
const CLOCK_DOWN: &str = "clock-down";
const CLOCK_UP: &str = "clock-up";
const CLOCK: &str = "clock";
const MODES: [&str; 5] = [
    REPEAT,
    PEDAL,
    CLOCK,
    CLOCK_DOWN,
    CLOCK_UP
];

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let mode = args.next().ok_or(format!("The first argument should be the arpeggiator mode: {}", MODES.join(", ")))?;
    let midi_in = args.next().ok_or("The second argument should be the MIDI IN device file")?;
    let midi_out = args.next().ok_or("The third argument should be the MIDI OUT device file")?;
    let mut midi_clock = || args.next().ok_or("The fourth argument should be the MIDI CLOCK device file");
    let (input_device, mut arp): (midi::InputDevice, Box<dyn Arpeggiator<_>>) = match mode.as_str() {
        REPEAT => (InputDevice::open(&midi_in, false)?,
        Box::new(timed::RepeatRecorder::new(
            &OutputDevice::open(&midi_out)?,
            &StopArpeggio::WhenFinished
        ))),
        PEDAL => (InputDevice::open(&midi_in, false)?,
        Box::new(timed::PedalRecorder::new(
            &OutputDevice::open(&midi_out)?,
            &StopArpeggio::Immediately
        ))),
        CLOCK => (InputDevice::open_with_external_clock(&midi_in, &midi_clock()?)?,
        Box::new(synced::MutatingHold::new(
            &OutputDevice::open(&midi_out)?,
            &StopArpeggio::WhenFinished
        ))),
        CLOCK_DOWN => (InputDevice::open_with_external_clock(&midi_in, &midi_clock()?)?,
        Box::new(synced::PressHold::new(
            &OutputDevice::open(&midi_out)?,
            &FixedNotesPerStep(1, Pattern::Down, StopArpeggio::WhenFinished)
        ))),
        CLOCK_UP => (InputDevice::open_with_external_clock(&midi_in, &midi_clock()?)?,
        Box::new(synced::PressHold::new(
            &OutputDevice::open(&midi_out)?,
            &FixedNotesPerStep(1, Pattern::Up, StopArpeggio::WhenFinished)
        ))),
        _ => return Err(format!("Invalid arpeggiator mode: {}", mode).into())
    };
    arp.listen(input_device)?; //TODO make this stop on ESC pressed (or any key?)
    Ok(())
}