use std::error::Error;
use std::fs;

use wmidi::{MidiMessage, ControlFunction, U7};

use crate::arpeggio::{NoteDetails, Step};
use crate::arpeggiator::{Pattern, ArpeggiatorMode};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Settings {
    pub finish_pattern: bool,
    pub fixed_velocity: Option<u8>,
    pub mode: ArpeggiatorMode,
    fixed_steps: Option<usize>,
    //TODO (SETTINGS) fixed steps per beat, fixed_beats?
    fixed_notes_per_step: Option<usize>,
    pattern: Pattern
}

impl Settings {
    pub fn generate_steps(&self, notes: Vec<NoteDetails>) -> Vec<Step> {
        if let Some(steps) = self.fixed_steps {
            self.pattern.of(notes, steps)
        } else if let Some(notes_per_step) = self.fixed_notes_per_step {
            let mut steps = 0;
            let mut notes_remaining = notes.len();
            while notes_remaining > 0 {
                steps += 1;
                if notes_remaining <= notes_per_step {
                    notes_remaining = 0;
                } else {
                    notes_remaining -= notes_per_step;
                }
            }
            self.pattern.of(notes, steps)
        } else {
            let notes_len = notes.len();
            self.pattern.of(notes, notes_len)
        }
    }

    pub fn load(file: String) -> Result<Vec<Self>, Box<dyn Error>> {
        let json = fs::read_to_string(&file).map_err(|e| format!("Cannot read from '{}': {}", file, e))?;
        let settings: Vec<Settings> = serde_json::from_str(&format!("[{}]", json)).map_err(|e| format!("Cannot parse settigs from '{}': {}", file, e))?;
        Ok(settings)
    }
}

//TODO (SETTINGS) implement rhythm follower settings getter
// FIRST: make sure this provides value for the types of arp I need, if it doesn't turn it into a github issue for future reference
// ** set keyboard rhythm volume to 0, midi out to ch10, pattern to *something* and turn it on
// ** handle any note-on for ch10 as triggers for arpeggio steps (rather than clock ticks)
// ** "learn" pattern in first beat (24 ticks) by determining steps based on there being any notes on during a tick (how we do know where the start of the beat is? only matters on non-even rhythms)
// ** this determines the number and duration of each step, then when notes are played, they are divided evenly between the steps, with extra notes on earlier steps as required
// ** this requires reading more note-on from midi_out (which currently just reads clock)

pub trait SettingsGetter {
    fn passthrough_midi(&mut self, message: MidiMessage<'static>) -> Option<MidiMessage<'static>>;
    fn get(&self) -> &Settings;
}

pub struct PredefinedProgramChanges {
    predefined: Vec<Settings>,
    index: usize,
    msb: u8,
    lsb: u8,
    pc: u8
}


impl SettingsGetter for PredefinedProgramChanges {
    fn passthrough_midi(&mut self, message: MidiMessage<'static>) -> Option<MidiMessage<'static>> {
        match message {
            MidiMessage::ControlChange(_, ControlFunction::BANK_SELECT, msb) => {
                self.msb = msb.into();
                None
            },
            MidiMessage::ControlChange(_, ControlFunction::BANK_SELECT_LSB, lsb) => {
                self.lsb = lsb.into();
                None
            },
            MidiMessage::ProgramChange(_, pc) => {
                self.pc = pc.into();
                self.index = ((self.msb as usize * u8::from(U7::MAX) as usize + self.lsb as usize) * u8::from(U7::MAX) as usize + self.pc as usize) % self.predefined.len();
                None
            },
            _ => Some(message)
        }
    }

    fn get(&self) -> &Settings {
        &self.predefined[self.index]
    }
}

impl PredefinedProgramChanges {
    pub fn new(predefined: Vec<Settings>) -> Self {
        if predefined.len() > u8::from(U7::MAX) as usize * u8::from(U7::MAX) as usize * u8::from(U7::MAX) as usize {
            panic!("Too many predefined program changes for 3 U7s");
        }
        Self {
            predefined,
            msb: 0,
            lsb: 0,
            pc: 0,
            index: 0
        }
    }
}
