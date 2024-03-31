use std::time::Duration;
use std::{env, fs, thread};
use std::error::Error;

use arpeggiator::MultiArpeggiator;
use settings::{PredefinedProgramChanges, Settings};
use midi::{InputDevice, OutputDevice, ClockDevice};
use status::{LedStatus, StatusSignal};
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
    let settings_list = Settings::load(args.next().unwrap_or(DEFAULT_SETTINGS_FILE.to_owned()))?;
    let status = LedStatus::<8>::new(18); //TextStatus::_new(std::io::stdout())
    if let Some(midi_in) = args.next() {
        if let Some(midi_out) = args.next() {
            run(&midi_in, &midi_out, settings_list, status)
        } else {
            run(&midi_in, &midi_in, settings_list, status)
        }
    } else {
        loop {
            let mut devices = list_files("/dev", "midi")?;
            while devices.len() != 2 {
                //TODO show loading if < 2, loading_error if > 2
                thread::sleep(Duration::from_millis(500));
                devices = list_files("/dev", "midi")?;
            }
            let mut result = None;
            while result.is_none() {
                //TODO show orange count up
                result = if ClockDevice::init(&devices[0]).is_ok() {
                    Some(run(&devices[1], &devices[0], settings_list, status))
                } else if ClockDevice::init(&devices[1]).is_ok() {
                    Some(run(&devices[1], &devices[0], settings_list, status))
                } else {
                    None
                }
            }
            match result.unwrap() {
                Ok(()) => println!("Arpeggiator disconnected OK"),
                Err(e) => println!("Arpeggiator disconnected with error: {}", e)
            }
        }
    }
}

fn run<SS: StatusSignal>(midi_in: &str, midi_out: &str, settings_list: Vec<Settings>, status: SS) -> Result<(), Box<dyn Error>> {
    println!("Starting arpeggiator with MIDI-IN: {}, MIDI-OUT: {}", midi_in, midi_out);
    MultiArpeggiator {
        midi_in: InputDevice::open_with_external_clock(&midi_in, &midi_out)?,
        midi_out: OutputDevice::open(&midi_out)?,
        settings: PredefinedProgramChanges::new(settings_list),
        status
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