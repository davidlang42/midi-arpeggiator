use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::time::Instant;

use wmidi::{MidiMessage, ControlFunction, U7, Channel};

use crate::arpeggio::{NoteDetails, Step};
use crate::arpeggiator::{Pattern, ArpeggiatorMode};
use crate::midi::{MidiReceiver, self};
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
    pub presets: Option<Vec<Preset>>
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
            presets: None
        }
    }

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

    pub fn _load(file: String) -> Result<Vec<Self>, Box<dyn Error>> {
        let json = fs::read_to_string(&file).map_err(|e| format!("Cannot read from '{}': {}", file, e))?;
        let settings: Vec<Settings> = serde_json::from_str(&format!("[{}]", json)).map_err(|e| format!("Cannot parse settigs from '{}': {}", file, e))?;
        Ok(settings)
    }
}

pub trait SettingsGetter: MidiReceiver {
    fn get(&self) -> &Settings;
}

pub struct WraparoundProgramChanges<'a> {
    predefined: &'a Vec<Settings>,
    index: usize,
    msb: u8,
    lsb: u8,
    pc: u8
}

impl<'a> MidiReceiver for WraparoundProgramChanges<'a> {
    fn passthrough_midi(&mut self, message: MidiMessage<'static>) -> Option<MidiMessage<'static>> {
        match message {
            MidiMessage::ControlChange(Self::RECEIVE_CHANNEL, ControlFunction::BANK_SELECT, msb) => {
                self.msb = msb.into();
                None
            },
            MidiMessage::ControlChange(Self::RECEIVE_CHANNEL, ControlFunction::BANK_SELECT_LSB, lsb) => {
                self.lsb = lsb.into();
                None
            },
            MidiMessage::ProgramChange(Self::RECEIVE_CHANNEL, pc) => {
                self.pc = pc.into();
                self.index = ((self.msb as usize * u8::from(U7::MAX) as usize + self.lsb as usize) * u8::from(U7::MAX) as usize + self.pc as usize) % self.predefined.len();
                None
            },
            _ => Some(message)
        }
    }
}

impl<'a> SettingsGetter for WraparoundProgramChanges<'a> {
    fn get(&self) -> &Settings {
        &self.predefined[self.index]
    }
}

impl<'a> WraparoundProgramChanges<'a> {
    const RECEIVE_CHANNEL: Channel = Channel::Ch1;

    pub fn _new(predefined: &'a Vec<Settings>) -> Self {
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

#[derive(Deserialize)]
pub struct SettingsWithProgramInfo {
    pub lsb: u8, // 0-127
    pub msb: u8, // 0-127
    pub pc: u8, // 1-128
    #[serde(flatten)]
    pub settings: Settings
}

impl SettingsWithProgramInfo {
    pub fn load(file: String) -> Result<Vec<Self>, Box<dyn Error>> {
        let json = fs::read_to_string(&file).map_err(|e| format!("Cannot read from '{}': {}", file, e))?;
        let settings: Vec<SettingsWithProgramInfo> = serde_json::from_str(&format!("[{}]", json)).map_err(|e| format!("Cannot parse settigs from '{}': {}", file, e))?;
        Ok(settings)
    }
}

pub struct SpecificProgramChanges<'a> {
    predefined: HashMap<(u8, u8, u8), &'a Settings>,
    default: &'a Settings,
    current: &'a Settings,
    msb: u8,
    lsb: u8,
    pc: u8
}

impl<'a> MidiReceiver for SpecificProgramChanges<'a> {
    fn passthrough_midi(&mut self, message: MidiMessage<'static>) -> Option<MidiMessage<'static>> {
        match message {
            MidiMessage::ControlChange(Self::RECEIVE_CHANNEL, ControlFunction::BANK_SELECT, msb) => {
                self.msb = msb.into();
                None
            },
            MidiMessage::ControlChange(Self::RECEIVE_CHANNEL, ControlFunction::BANK_SELECT_LSB, lsb) => {
                self.lsb = lsb.into();
                None
            },
            MidiMessage::ProgramChange(Self::RECEIVE_CHANNEL, pc) => {
                self.pc = u8::from(pc) + 1; // pc is 1 based
                if let Some(specific) = self.predefined.get(&(self.msb, self.lsb, self.pc)) {
                    self.current = specific;
                } else {
                    self.current = self.default;
                }
                None
            },
            _ => Some(message)
        }
    }
}

impl<'a> SettingsGetter for SpecificProgramChanges<'a> {
    fn get(&self) -> &Settings {
        &self.current
    }
}

impl<'a> SpecificProgramChanges<'a> {
    const RECEIVE_CHANNEL: Channel = Channel::Ch1;

    pub fn new(settings_with_program_info: &'a Vec<SettingsWithProgramInfo>, default_settings: &'a Settings) -> Self {
        let mut predefined = HashMap::new();
        for s in settings_with_program_info {
            predefined.insert((s.msb, s.lsb, s.pc), &s.settings);
        }
        Self {
            predefined,
            msb: 0,
            lsb: 0,
            pc: 0,
            current: default_settings,
            default: default_settings
        }
    }
}

pub struct BpmDetector {
    ticks: usize,
    last_beat: Instant,
    last_bpm: usize
}

impl BpmDetector {
    pub fn _new() -> Self {
        Self {
            ticks: 0,
            last_beat: Instant::now(),
            last_bpm: 0
        }
    }

    pub fn _get(&self) -> usize {
        self.last_bpm
    }
}

impl MidiReceiver for BpmDetector {
    fn passthrough_midi(&mut self, message: MidiMessage<'static>) -> Option<MidiMessage<'static>> {
        if let MidiMessage::TimingClock = message {
            self.ticks += 1;
            if self.ticks == 24 {
                self.ticks = 0;
                let now = Instant::now();
                let ns = now.duration_since(self.last_beat).as_nanos();
                self.last_beat = now;
                let bpm = (60000000000.0 / ns as f64).round() as usize;
                if bpm != self.last_bpm {
                    self.last_bpm = bpm;
                }
            }
        }
        Some(message)
    }
}

pub struct NoteCounter {
    midi_channel: Channel,
    notes: [usize; Self::COUNT_PERIOD],
    ticks: usize,
    last_note_count: usize
}

impl NoteCounter {
    const COUNT_PERIOD: usize = midi::TICKS_PER_BEAT; // 1 quarter note

    pub fn _new(midi_channel: Channel) -> Self {
        Self {
            midi_channel,
            ticks: 0,
            notes: [0; Self::COUNT_PERIOD],
            last_note_count: 0,
        }
    }

    pub fn _get(&self) -> usize {
        self.last_note_count
    }
}

impl MidiReceiver for NoteCounter {
    fn passthrough_midi(&mut self, message: MidiMessage<'static>) -> Option<MidiMessage<'static>> {
        match message {
            MidiMessage::TimingClock => {
                self.ticks += 1;
                if self.ticks == self.notes.len() {
                    self.ticks = 0;
                    let note_count = self.notes.iter().filter(|&&c| c > 0).count();
                    if note_count != self.last_note_count {
                        self.last_note_count = note_count;
                    }
                    for i in 0..Self::COUNT_PERIOD {
                        self.notes[i] = 0;
                    }
                }
                Some(message)
            },
            MidiMessage::NoteOn(c, _, _) if c == self.midi_channel => {
                self.notes[self.ticks] += 1;
                None // don't forward notes on this channel
            },
            _ => Some(message)
        }
    }
}