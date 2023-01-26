use std::env;
use std::error::Error;
use crate::arpeggiator::Arpeggiator;

mod midi;
mod arpeggio;
mod arpeggiator;

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let mode = args.next().expect("The first argument should be the arpeggiator mode: repeat, pedal");
    let midi_in = midi::InputDevice::open(&args.next().expect("The second argument should be the MIDI IN device file"))?;
    let midi_out = midi::OutputDevice::open(&args.next().expect("The third argument should be the MIDI OUT device file"))?;
    let mut arp: Box<dyn Arpeggiator> = match mode.as_str() {
        "repeat" => Box::new(arpeggiator::RepeatRecorder::new(midi_in, midi_out)),
        "pedal" => Box::new(arpeggiator::PedalRecorder::new(midi_in, midi_out)),
        _ => panic!("Invalid arpeggiator mode: {}", mode)
    };
    arp.listen();
    Ok(())
}