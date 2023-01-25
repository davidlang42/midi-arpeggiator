use std::env;
use std::error::Error;

mod midi;
mod arpeggio;
mod arpeggiator;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() == 3 {
        let midi_in = midi::InputDevice::open(&args[1])?;
        let midi_out = midi::OutputDevice::open(&args[2])?;
        let mut arp = arpeggiator::Arpeggiator::new(midi_in, midi_out);
        arp.listen();
    } else {
        println!("Requires exactly 2 arguments: MIDI_IN, MIDI_OUT");
    }
    Ok(())
}