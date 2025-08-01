use std::error::Error;
use wmidi::{ControlFunction, MidiMessage, U7};

use strum_macros::EnumIter;

use crate::arpeggio::{NoteDetails, Step};
use crate::presets::Preset;
use crate::status::StatusSignal;
use crate::midi::{InputDevice, MidiReceiver, OutputDevice};
use crate::settings::{Settings, SettingsGetter};

pub mod timed;
pub mod synced;
pub mod full_length;

#[derive(Copy, Clone, EnumIter, Debug, Serialize, Deserialize, PartialEq)]
pub enum Pattern {
    Up,
    Down
}

impl Pattern {
    pub fn of(&self, mut notes: Vec<NoteDetails>, steps: usize) -> Vec<Step> {
        if steps == 0 {
            panic!("Cannot generate Pattern in 0 steps");
        }
        // put the notes in order based on the pattern type
        match self {
            Pattern::Up => notes.sort_by(|a, b| a.n.cmp(&b.n)),
            Pattern::Down => notes.sort_by(|a, b| b.n.cmp(&a.n)),
        }
        // expand notes until there are at least enough notes for 1 note per step
        while notes.len() < steps {
            Self::expand(&mut notes);
        }
        // calculate how many notes in each step (prioritising earlier steps)
        let minimum_notes_per_step = notes.len() / steps;
        let mut notes_per_step = [minimum_notes_per_step].repeat(steps);
        let mut notes_remaining = notes.len() % steps;
        for i in 0..steps {
            if notes_remaining == 0 {
                break;
            } else {
                notes_per_step[i] += 1;
                notes_remaining -= 1;
            }
        }
        // generate steps
        let mut steps = Vec::new();
        let mut iter = notes.into_iter();
        for notes_in_this_step in notes_per_step {
            steps.push(Step::notes((&mut iter).take(notes_in_this_step).collect()));
        }
        steps
    }

    fn expand(notes: &mut Vec<NoteDetails>) {
        // create extra notes by repeating the existing notes in reverse
        let range = match notes.len() {
            0 => panic!("Cannot generate Pattern of 0 notes"),
            1 => 0..1, // if there is only 1 note, repeat it
            2 => 0..2, // if there are only 2 notes, repeat them both
            _ => 1..(notes.len() - 1) // otherwise repeat all except first and last notes
        };
        for i in range.rev() {
            notes.push(notes[i].clone())
        }
    }
}

pub trait Arpeggiator {
    fn process(&mut self, message: MidiMessage<'static>, settings: &Settings, signal: &mut dyn StatusSignal) -> Result<(), Box<dyn Error>>;
    fn stop_arpeggios(&mut self) -> Result<(), Box<dyn Error>>;
    fn count_arpeggios(&self) -> usize;
}

#[derive(PartialEq, EnumIter, Copy, Clone, Debug, Serialize, Deserialize)]
pub enum ArpeggiatorMode {
    Passthrough,
    RepeatRecorder,
    TimedPedalRecorder,
    PressHold,
    MutatingHold,
    SyncedPedalRecorder,
    EvenMutator,
    PrerecordedSets,
    TriggeredChords,
}

impl ArpeggiatorMode {
    fn create<'a>(&self, midi_out: &'a OutputDevice, presets: &Option<Vec<Preset>>) -> Box<dyn Arpeggiator + 'a> {
        match self {
            Self::Passthrough => Box::new(Passthrough(midi_out)),
            Self::MutatingHold => Box::new(synced::MutatingHold::new(midi_out)),
            Self::PressHold => Box::new(synced::PressHold::new(midi_out)),
            Self::TimedPedalRecorder => Box::new(timed::PedalRecorder::new(midi_out)),
            Self::RepeatRecorder => Box::new(timed::RepeatRecorder::new(midi_out)),
            Self::SyncedPedalRecorder => Box::new(synced::PedalRecorder::new(midi_out)),
            Self::EvenMutator => Box::new(full_length::EvenMutator::new(midi_out)),
            Self::PrerecordedSets => {
                if let Some(actual_presets) = presets {
                    Box::new(synced::PrerecordedSets::new(midi_out, actual_presets.clone()))
                } else {
                    // not very useful, but better not to crash
                    Box::new(synced::PrerecordedSets::new(midi_out, Vec::new()))
                }
            },
            Self::TriggeredChords => {
                if let Some(actual_presets) = presets {
                    Box::new(full_length::TriggeredChords::new(midi_out, actual_presets.clone()))
                } else {
                    // not very useful, but better not to crash
                    Box::new(full_length::TriggeredChords::new(midi_out, Vec::new()))
                }
            }
        }
    }
}

pub struct MultiArpeggiator<'a, SG: SettingsGetter, SS: StatusSignal> {
    pub midi_in: InputDevice,
    pub midi_out: OutputDevice,
    pub settings: SG,
    pub status: &'a mut SS
}

impl<'a, SS: StatusSignal, SG: SettingsGetter> MultiArpeggiator<'a, SG, SS> {
    pub fn listen(self) -> Result<(), Box<dyn Error>> {
        self.listen_with_midi_receivers(Vec::new())
    }

    pub fn listen_with_midi_receivers(mut self, mut extra_midi_receivers: Vec<&mut dyn MidiReceiver>) -> Result<(), Box<dyn Error>> {
        let mut existing_settings = self.settings.get().clone();
        let mut arpeggiator: Box<dyn Arpeggiator> = existing_settings.mode.create(&self.midi_out, &self.settings.get().presets);
        loop {
            let mut m = Some(self.midi_in.read()?);
            // pass message through extra receivers
            for midi_receiver in extra_midi_receivers.iter_mut() {
                m = midi_receiver.passthrough_midi(m.unwrap());
                if m.is_none() { break; }
            }
            // pass message through settings
            if m.is_none() { continue; }
            m = self.settings.passthrough_midi(m.unwrap());
            // handle settings changes
            self.status.update_settings(self.settings.get());
            let new_settings = self.settings.get().clone();
            if new_settings != existing_settings {
                existing_settings = new_settings;
                arpeggiator.stop_arpeggios()?;
                arpeggiator = existing_settings.mode.create(&self.midi_out, &self.settings.get().presets);
                self.status.update_count(arpeggiator.count_arpeggios());
            }
            // pass message through status
            if m.is_none() { continue; }
            m = self.status.passthrough_midi(m.unwrap());
            // process message in arp
            if m.is_none() { continue; }
            arpeggiator.process(m.unwrap(), self.settings.get(), self.status)?;
            self.status.update_count(arpeggiator.count_arpeggios());
        }
    }
}

struct Passthrough<'a>(&'a OutputDevice);

impl<'a> Passthrough<'a> {
    fn should_passthrough(message: &MidiMessage) -> bool {
        match message {
            // dont send patch changes
            MidiMessage::ProgramChange(_, _) => false,
            MidiMessage::ControlChange(_, ControlFunction::BANK_SELECT, _) => false,
            MidiMessage::ControlChange(_, ControlFunction::BANK_SELECT_LSB, _) => false,
            // do send notes and expression
            MidiMessage::NoteOff(_, _, _) => true,
            MidiMessage::NoteOn(_, _, _) => true,
            MidiMessage::PolyphonicKeyPressure(_, _, _) => true,
            MidiMessage::ControlChange(_, _, _) => true,
            MidiMessage::ChannelPressure(_, _) => true,
            MidiMessage::PitchBendChange(_, _) => true,
            // dont send other weirdness
            MidiMessage::SysEx(_) => false,
            MidiMessage::OwnedSysEx(_) => false,
            MidiMessage::MidiTimeCode(_) => false,
            MidiMessage::SongPositionPointer(_) => false,
            MidiMessage::SongSelect(_) => false,
            MidiMessage::Reserved(_) => false,
            MidiMessage::TuneRequest => false,
            MidiMessage::TimingClock => false,
            MidiMessage::Start => false,
            MidiMessage::Continue => false,
            MidiMessage::Stop => false,
            MidiMessage::ActiveSensing => false,
            MidiMessage::Reset => false
        }
    }
}

impl<'a> Arpeggiator for Passthrough<'a> {
    fn process(&mut self, mut message: MidiMessage<'static>, settings: &Settings, _signal: &mut dyn StatusSignal) -> Result<(), Box<dyn Error>> {
        if Self::should_passthrough(&message) {
            if let Some(fixed) = settings.fixed_velocity {
                message = match message {
                    MidiMessage::NoteOff(c, n, _) => MidiMessage::NoteOff(c, n, U7::from_u8_lossy(fixed)),
                    MidiMessage::NoteOn(c, n, _) => MidiMessage::NoteOn(c, n, U7::from_u8_lossy(fixed)),
                    MidiMessage::PolyphonicKeyPressure(c, n, _) => MidiMessage::PolyphonicKeyPressure(c, n, U7::from_u8_lossy(fixed)),
                    _ => message
                };
            }
            if let Some(doubling) = &settings.double_notes {
                self.0.send_with_doubling(message, doubling.iter())?;
            } else {
                self.0.send(message)?;
            }
        }
        Ok(())
    }

    fn stop_arpeggios(&mut self) -> Result<(), Box<dyn Error>> {
        Ok(())
    }

    fn count_arpeggios(&self) -> usize {
        1
    }
}
