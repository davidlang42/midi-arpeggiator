use std::time::Duration;
use std::{env, fs, thread};
use std::error::Error;

use arpeggiator::MultiArpeggiator;
use settings::{Settings, SettingsWithProgramInfo, SpecificProgramChanges};
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
    let settings_list = SettingsWithProgramInfo::load(args.next().unwrap_or(DEFAULT_SETTINGS_FILE.to_owned()))?;
    let mut status = LedStatus::<8>::new(18); //TextStatus::_new(std::io::stdout())
    if let Some(midi_in) = args.next() {
        if let Some(midi_out) = args.next() {
            run(&midi_in, &midi_out, &settings_list, &mut status)
        } else {
            run(&midi_in, &midi_in, &settings_list, &mut status)
        }
    } else {
        loop {
            let mut devices = list_files("/dev", "midi")?;
            while devices.len() != 2 {
                if devices.len() < 2 {
                    status.waiting_for_midi_connect();
                } else {
                    status.waiting_for_midi_disconnect();
                }
                thread::sleep(Duration::from_millis(500));
                devices = list_files("/dev", "midi")?;
            }
            status.waiting_for_midi_clock();
            if ClockDevice::init(&devices[0]).is_ok() {
                run_and_print(&devices[1], &devices[0], &settings_list, &mut status);
            } else if ClockDevice::init(&devices[1]).is_ok() {
                run_and_print(&devices[1], &devices[0], &settings_list, &mut status);
            }
        }
    }
}

fn run_and_print<SS: StatusSignal>(midi_in: &str, midi_out: &str, settings_list: &Vec<SettingsWithProgramInfo>, status: &mut SS) {
    match run(midi_in, midi_out, settings_list, status) {
        Ok(()) => println!("Arpeggiator disconnected OK"),
        Err(e) => println!("Arpeggiator disconnected with error: {}", e)
    }
}

fn run<SS: StatusSignal>(midi_in: &str, midi_out: &str, settings_list: &Vec<SettingsWithProgramInfo>, status: &mut SS) -> Result<(), Box<dyn Error>> {
    println!("Starting arpeggiator with MIDI-IN: {}, MIDI-OUT: {}", midi_in, midi_out);
    let default_settings = Settings::passthrough();
    MultiArpeggiator {
        midi_in: InputDevice::open_with_external_clock(&midi_in, &midi_out)?,
        midi_out: OutputDevice::open(&midi_out)?,
        settings: SpecificProgramChanges::new(settings_list, &default_settings),
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