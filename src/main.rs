use std::env;
use std::error::Error;

use arpeggiator::MultiArpeggiator;
use settings::{ReceiveProgramChanges, VariableVelocity};

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
const CLOCK_PEDAL: &str = "clock-pedal";
const MULTI: &str = "multi";
const MODES: [&str; 7] = [
    REPEAT,
    PEDAL,
    CLOCK,
    CLOCK_DOWN,
    CLOCK_UP,
    CLOCK_PEDAL,
    MULTI
];

//TODO (CONFIG) headless auto config
// - by opening all midi devices for read, waiting for first to send a note on, then second to send a note on
// - first becomes in, second becomes out
// - to confirm connection, play the 2 notes used as first note ons to the output one after another

//TODO (STATUS) make StatusSignal trait
// - basic implementation std out, later implement physical LED
// - indicate beats at tempo, number of steps, direction
// - ideally show if an arp is playing/stopping
// - allow arpeggiators to send a "start beat" signal, which syncs the clock beat to start at the next midi tick (for example PedalRecorder will mark the start of the beat when the pedal is pressed down)

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let mode = args.next().ok_or(format!("The first argument should be the arpeggiator mode: {}", MODES.join(", ")))?;
    let midi_in = args.next().ok_or("The second argument should be the MIDI IN device file")?;
    let midi_out = args.next().ok_or("The third argument should be the MIDI OUT device file")?;
    let mut midi_clock = || args.next().ok_or("The fourth argument should be the MIDI CLOCK device file");
    match mode.as_str() {
        REPEAT => timed::RepeatRecorder::new(
            &OutputDevice::open(&midi_out)?
        ).listen(
            InputDevice::open(&midi_in, false)?,
            VariableVelocity(StopArpeggio::WhenFinished)
        ),
        PEDAL => timed::PedalRecorder::new(
            &OutputDevice::open(&midi_out)?,
        ).listen(
            InputDevice::open(&midi_in, false)?,
            VariableVelocity(StopArpeggio::Immediately)
        ),
        CLOCK => synced::MutatingHold::new(
            &OutputDevice::open(&midi_out)?,
        ).listen(
            InputDevice::open_with_external_clock(&midi_in, &midi_clock()?)?,
            VariableVelocity(StopArpeggio::WhenFinished)
        ),
        CLOCK_DOWN => synced::PressHold::new(
            &OutputDevice::open(&midi_out)?
        ).listen(
            InputDevice::open_with_external_clock(&midi_in, &midi_clock()?)?,
            FixedNotesPerStep(1, Pattern::Down, None, StopArpeggio::WhenFinished)
        ),
        CLOCK_UP => synced::PressHold::new(
            &OutputDevice::open(&midi_out)?
        ).listen(
            InputDevice::open_with_external_clock(&midi_in, &midi_clock()?)?,
            FixedNotesPerStep(1, Pattern::Up, None, StopArpeggio::WhenFinished)
        ),
        CLOCK_PEDAL => synced::PedalRecorder::new(
            &OutputDevice::open(&midi_out)?,
        ).listen(
            InputDevice::open_with_external_clock(&midi_in, &midi_clock()?)?,
            FixedNotesPerStep(1, Pattern::Up, None, StopArpeggio::WhenFinished)
        ),
        MULTI => MultiArpeggiator::new(
            &OutputDevice::open(&midi_out)?,
        ).listen(
            InputDevice::open_with_external_clock(&midi_in, &midi_clock()?)?,
            ReceiveProgramChanges::new()
        ),
        _ => return Err(format!("Invalid arpeggiator mode: {}", mode).into())
    }
}