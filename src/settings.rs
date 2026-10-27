use std::collections::HashMap;
use std::error::Error;
use std::fs;

use wmidi::{MidiMessage, ControlFunction};

use crate::arpeggio::{NoteDetails, Step};
use crate::arpeggiator::{Pattern, ArpeggiatorMode};
use crate::midi::MidiReceiver;
use crate::presets::Preset;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Settings {
    pub finish_pattern: bool,
    pub fixed_velocity: Option<u8>,
    pub mode: ArpeggiatorMode,
    pub fixed_steps: Option<usize>, // assumed in 1 beat
    pub fixed_notes_per_step: Option<usize>,
    pub pattern: Pattern,
    pub double_notes: Option<Vec<i8>>,
    pub presets: Option<Vec<Preset>>,
    pub max_repeats: Option<usize>,
    pub use_pedal: Option<bool>
}

impl Settings {
    pub fn passthrough() -> Self {
        Self {
            mode: ArpeggiatorMode::Passthrough,
            finish_pattern: false,
            fixed_velocity: None,
            fixed_steps: None,
            fixed_notes_per_step: None,
            pattern: Pattern::Up,
            double_notes: None,
            presets: None,
            max_repeats: None,
            use_pedal: None
        }
    }

    pub fn generate_steps(fixed_steps: Option<usize>, pattern: Pattern, fixed_notes_per_step: Option<usize>, notes: Vec<NoteDetails>) -> Vec<Step> {
        if let Some(steps) = fixed_steps {
            pattern.of(notes, steps)
        } else if let Some(notes_per_step) = fixed_notes_per_step {
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
            pattern.of(notes, steps)
        } else {
            let notes_len = notes.len();
            pattern.of(notes, notes_len)
        }
    }

    pub fn _load(file: String) -> Result<Vec<Self>, Box<dyn Error>> {
        let json = fs::read_to_string(&file).map_err(|e| format!("Cannot read from '{}': {}", file, e))?;
        let settings: Vec<Settings> = serde_json::from_str(&format!("[{}]", json)).map_err(|e| format!("Cannot parse settigs from '{}': {}", file, e))?;
        Ok(settings)
    }
}

pub trait SettingsChanger: MidiReceiver + Clone {
    fn get_change(&mut self) -> Option<&Settings>;

    fn get_default(&self) -> &Settings;
}

#[derive(Deserialize)]
pub struct SettingsWithProgramInfo {
    #[serde(flatten)]
    pub program: ProgramSet,
    #[serde(flatten)]
    pub settings: Settings
}

impl SettingsWithProgramInfo {
    pub fn load_list(file: String) -> Result<Vec<Self>, Box<dyn Error>> {
        let mut json = fs::read_to_string(&file).map_err(|e| format!("Cannot read from '{}': {}", file, e))?;
        if !json.trim_start().starts_with('[') && !json.trim_end().ends_with(']') {
            json.insert(0, '[');
            json.push(']');
        }
        let settings: Vec<SettingsWithProgramInfo> = serde_json::from_str(&json).map_err(|e| format!("Cannot parse settigs from '{}': {}", file, e))?;
        Ok(settings)
    }

    pub fn load_map(file: String) -> Result<HashMap<ProgramSet, Settings>, Box<dyn Error>> {
        let mut map = HashMap::new();
        for s in Self::load_list(file)? {
            map.insert(s.program, s.settings);
        }
        Ok(map)
    }
}

#[derive(Deserialize, PartialEq, Copy, Clone, Eq, Hash)]
pub struct ProgramSet {
    msb: u8, // 0-127
    lsb: u8, // 0-127
    pc: u8 // 1-128
}

impl ProgramSet {
    pub fn unset() -> Self {
        Self {
            msb: 0,
            lsb: 0,
            pc: 0
        }
    }
}

#[derive(Clone)]
pub struct ProgramChanger<'a> {
    default_settings: Settings,
    settings_map: &'a HashMap<ProgramSet, Settings>,
    existing: ProgramSet,
    new: ProgramSet,
    change: bool
}

impl<'a> ProgramChanger<'a> {
    pub fn new(default_settings: Settings, settings_map: &'a HashMap<ProgramSet, Settings>) -> Self {
        // technically starting at (0,0,0) is an invalid state, but thats important so that any incoming PC will trigger the first change
        // eg. if we started at (0,0,1) and a PC of 1 came in, this would not register a change, but it should
        Self {
            default_settings,
            settings_map,
            existing: ProgramSet::unset(),
            new: ProgramSet::unset(),
            change: false
        }
    }
}

impl<'a> SettingsChanger for ProgramChanger<'a> {
    fn get_default(&self) -> &Settings {
        &self.default_settings
    }

    fn get_change(&mut self) -> Option<&Settings> {
        if self.change {
            self.change = false;
            Some(self.settings_map.get(&self.existing).unwrap_or(&self.default_settings))
        } else {
            None
        }
    }
}

impl<'a> MidiReceiver for ProgramChanger<'a> {
    fn passthrough_midi(&mut self, message: MidiMessage<'static>) -> Option<MidiMessage<'static>> {
        match message {
            MidiMessage::ControlChange(_, ControlFunction::BANK_SELECT, msb) => {
                self.new.msb = msb.into();
                None
            },
            MidiMessage::ControlChange(_, ControlFunction::BANK_SELECT_LSB, lsb) => {
                self.new.lsb = lsb.into();
                None
            },
            MidiMessage::ProgramChange(_, pc) => {
                self.new.pc = u8::from(pc) + 1; // pc is 1-based by MIDI specification, but value comes in as 0-based U7
                if self.new != self.existing {
                    self.change = true;
                    self.existing = self.new;
                }
                None
            },
            _ => Some(message)
        }
    }
}