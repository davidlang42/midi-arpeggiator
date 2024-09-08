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
mod notename;
mod presets;

#[macro_use] extern crate serde_derive;

const DEFAULT_SETTINGS_FILE: &str = "settings.json";

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let settings_list = SettingsWithProgramInfo::load(args.next().unwrap_or(DEFAULT_SETTINGS_FILE.to_owned()))?;
    let mut status = LedStatus::<8>::new(18); //TextStatus::_new(std::io::stdout());
    if let Some(midi_or_required_devices) = args.next() {
        if let Ok(required_devices) = midi_or_required_devices.parse::<usize>() {
            wait_for_midi_devices(required_devices, status, settings_list)
        } else {
            let midi_in = midi_or_required_devices;
            if let Some(midi_out) = args.next() {
                run(&midi_in, &midi_out, &settings_list, &mut status)
            } else {
                run(&midi_in, &midi_in, &settings_list, &mut status)
            }
        }
    } else {
        wait_for_midi_devices(2, status, settings_list)
    }
}

fn wait_for_midi_devices<S: StatusSignal>(required_devices: usize, mut status: S, settings_list: Vec<SettingsWithProgramInfo>) -> Result<(), Box<dyn Error>> {
    if required_devices < 1 || required_devices > 2 {
        panic!("required_devices out of range 1-2")
    }
    loop {
        let mut devices = list_files("/dev", "midi")?;
        while devices.len() != required_devices {
            if devices.len() < required_devices {
                status.waiting_for_midi_connect();
            } else {
                status.waiting_for_midi_disconnect();
            }
            thread::sleep(Duration::from_millis(500));
            devices = list_files("/dev", "midi")?;
        }
        status.waiting_for_midi_clock();
        if devices.len() == 1 {
            run_and_print(&devices[0], &devices[0], &settings_list, &mut status);
        } else if ClockDevice::init(&devices[0]).is_ok() {
            run_and_print(&devices[1], &devices[0], &settings_list, &mut status);
        } else if ClockDevice::init(&devices[1]).is_ok() {
            run_and_print(&devices[0], &devices[1], &settings_list, &mut status);
        }
    }
}

fn run_and_print<SS: StatusSignal>(midi_in: &str, midi_out_with_clock: &str, settings_list: &Vec<SettingsWithProgramInfo>, status: &mut SS) {
    match run(midi_in, midi_out_with_clock, settings_list, status) {
        Ok(()) => println!("Arpeggiator disconnected OK"),
        Err(e) => println!("Arpeggiator disconnected with error: {}", e)
    }
}

fn run<SS: StatusSignal>(midi_in: &str, midi_out: &str, settings_list: &Vec<SettingsWithProgramInfo>, status: &mut SS) -> Result<(), Box<dyn Error>> {
    println!("Starting arpeggiator with MIDI-IN: {}, MIDI-OUT: {}", midi_in, midi_out);
    let default_settings = Settings::passthrough();
    MultiArpeggiator {
        midi_in: if midi_in == midi_out {
            InputDevice::open(&midi_in, true)?
        } else {
            InputDevice::open_with_external_clock(&midi_in, &midi_out, true)?
        },
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