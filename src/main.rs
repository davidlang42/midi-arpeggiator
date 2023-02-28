use std::{env, fs};
use std::error::Error;

use arpeggiator::MultiArpeggiator;
use settings::ReceiveProgramChanges;
use midi::{InputDevice, OutputDevice, ClockDevice};

mod midi;
mod arpeggio;
mod arpeggiator;
mod settings;

const DEFAULT_SETTINGS_FILE: &str = "settings.json";

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
    let _settings = args.next().unwrap_or(DEFAULT_SETTINGS_FILE.to_owned());
    //TODO load settings file (Vec<Settings>) and give it to the arpeggiator
    let devices = list_files("/dev", "midi")?;
    match devices.len() {
        0 => Err(format!("No MIDI devices found").into()),
        1 => run(&devices[0], &devices[0]),
        2 if ClockDevice::init(&devices[0]).is_ok() => run(&devices[1], &devices[0]),
        2 if ClockDevice::init(&devices[1]).is_ok() => run(&devices[0], &devices[1]),
        _ => Err(format!("More than 2 MIDI devices found").into())
    }
}

fn run(midi_in: &str, midi_out: &str) -> Result<(), Box<dyn Error>> {
    println!("Starting arpeggiator with MIDI-IN: {}, MIDI-OUT: {}", midi_in, midi_out);
    MultiArpeggiator::new(
        &OutputDevice::open(&midi_out)?,
    ).listen(
        InputDevice::open_with_external_clock(&midi_in, &midi_out)?,
        ReceiveProgramChanges::new()
    )
}

fn list_files(root: &str, prefix: &str) -> Result<Vec<String>, Box<dyn Error>> {
    let md = fs::metadata(root)?;
    if md.is_dir() {
        let mut files = Vec::new();
        for entry in fs::read_dir(root)? {
            let path = entry?.path();
            if !path.is_dir() && path.file_name().unwrap().to_string_lossy().starts_with(prefix) {
                files.push(path.display().to_string());
            }
        }
        files.sort();
        Ok(files)
    } else {
        Ok(vec![root.to_string()])
    }
}