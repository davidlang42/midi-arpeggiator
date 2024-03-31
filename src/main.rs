use std::{env, fs};
use std::error::Error;

use arpeggiator::MultiArpeggiator;
use settings::{PredefinedProgramChanges, Settings};
use midi::{InputDevice, OutputDevice, ClockDevice};
use status::LedStatus;
//use crate::status::TextStatus;

mod midi;
mod arpeggio;
mod arpeggiator;
mod settings;
mod status;

#[macro_use] extern crate serde_derive;

const DEFAULT_SETTINGS_FILE: &str = "settings.json";

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let predefined = Settings::load(args.next().unwrap_or(DEFAULT_SETTINGS_FILE.to_owned()))?;
    let devices = list_files("/dev", "midi")?;
    match devices.len() {
        0 => Err(format!("No MIDI devices found").into()),
        1 => run(&devices[0], &devices[0], predefined),
        2 if ClockDevice::init(&devices[0]).is_ok() => run(&devices[1], &devices[0], predefined),
        2 if ClockDevice::init(&devices[1]).is_ok() => run(&devices[0], &devices[1], predefined),
        _ => Err(format!("More than 2 MIDI devices found").into())
    }
}

fn run(midi_in: &str, midi_out: &str, predefined: Vec<Settings>) -> Result<(), Box<dyn Error>> {
    println!("Starting arpeggiator with MIDI-IN: {}, MIDI-OUT: {}", midi_in, midi_out);
    MultiArpeggiator {
        midi_in: InputDevice::open_with_external_clock(&midi_in, &midi_out)?,
        midi_out: OutputDevice::open(&midi_out)?,
        settings: PredefinedProgramChanges::new(predefined),
        status: LedStatus::<8>::new(18) //TextStatus::_new(std::io::stdout())
    }.listen()
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