use std::time::Duration;
use std::{env, fs, thread};
use std::error::Error;

use arpeggiator::MultiArpeggiator;
use settings::SettingsWithProgramInfo;
use midi::ClockDevice;
use status::StatusSignal;

use crate::settings::{ProgramChanger, Settings, SettingsChanger};
#[cfg(feature = "led")]
use crate::status::LedStatus;

#[cfg(feature = "text")]
use crate::status::TextStatus;

#[cfg(all(feature = "led", feature = "text"))]
compile_error!("Cannot enable both 'led' and 'text' features at the same time. Choose one.");

#[cfg(not(any(feature = "led", feature = "text")))]
compile_error!("You must specify a status backend. Enable either the 'led' or 'text' feature.");

mod midi;
mod arpeggio;
mod arpeggiator;
mod settings;
mod status;
mod notename;
mod presets;

#[cfg(not(feature = "unix"))]
mod nonblock_win;

#[cfg(not(feature = "led"))]
mod ws281x_rpi_stub;

#[macro_use] extern crate serde_derive;

const DEFAULT_SETTINGS_FILE: &str = "settings.json";

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let settings_map = SettingsWithProgramInfo::load_map(args.next().unwrap_or(DEFAULT_SETTINGS_FILE.to_owned()))?;
    let settings_changer = ProgramChanger::new(Settings::passthrough(), &settings_map);

    #[cfg(feature = "led")]
    let mut status = LedStatus::<8>::new(18);

    #[cfg(feature = "text")]
    let mut status = TextStatus::new(std::io::stdout(), settings_changer.get_default());

    if let Some(midi_or_required_devices) = args.next() {
        if let Ok(required_devices) = midi_or_required_devices.parse::<usize>() {
            wait_for_midi_devices(required_devices, settings_changer, status)
        } else {
            let midi_in = midi_or_required_devices;
            if let Some(midi_out) = args.next() {
                run(&midi_in, &midi_out, settings_changer, &mut status)
            } else {
                run(&midi_in, &midi_in, settings_changer, &mut status)
            }
        }
    } else {
        wait_for_midi_devices(2, settings_changer, status)
    }
}

fn wait_for_midi_devices<SC: SettingsChanger, SS: StatusSignal>(required_devices: usize, settings_changer: SC, mut status: SS) -> Result<(), Box<dyn Error>> {
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
            run_and_print(&devices[0], &devices[0], settings_changer.clone(), &mut status); //TODO avoid clone
        } else if ClockDevice::init(&devices[0]).is_ok() {
            run_and_print(&devices[1], &devices[0], settings_changer.clone(), &mut status); //TODO avoid clone
        } else if ClockDevice::init(&devices[1]).is_ok() {
            run_and_print(&devices[0], &devices[1], settings_changer.clone(), &mut status); //TODO avoid clone
        }
    }
}

fn run_and_print<SC: SettingsChanger, SS: StatusSignal>(midi_in: &str, midi_out_with_clock: &str, settings_changer: SC, status: &mut SS) {
    match run(midi_in, midi_out_with_clock, settings_changer, status) {
        Ok(()) => println!("Arpeggiator disconnected OK"),
        Err(e) => println!("Arpeggiator disconnected with error: {}", e)
    }
}

fn run<SC: SettingsChanger, SS: StatusSignal>(midi_in: &str, midi_out: &str, settings_changer: SC, status: &mut SS) -> Result<(), Box<dyn Error>> {
    println!("Starting arpeggiator with MIDI-IN: {}, MIDI-OUT: {}", midi_in, midi_out);
    MultiArpeggiator::new(midi_in, midi_out, settings_changer, status)?.listen()
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