use std::error::Error;
use wmidi::MidiMessage;

use strum_macros::EnumIter;

use crate::arpeggio::{NoteDetails, Step};
use crate::status::StatusSignal;
use crate::midi::{MidiReceiver, OutputDevice, InputDevice};
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
    EvenMutator
}

impl ArpeggiatorMode {
    fn create<'a>(&self, midi_out: &'a OutputDevice) -> Box<dyn Arpeggiator + 'a> {
        match self {
            Self::Passthrough => Box::new(Passthrough(midi_out)),
            Self::MutatingHold => Box::new(synced::MutatingHold::new(midi_out)),
            Self::PressHold => Box::new(synced::PressHold::new(midi_out)),
            Self::TimedPedalRecorder => Box::new(timed::PedalRecorder::new(midi_out)),
            Self::RepeatRecorder => Box::new(timed::RepeatRecorder::new(midi_out)),
            Self::SyncedPedalRecorder => Box::new(synced::PedalRecorder::new(midi_out)),
            Self::EvenMutator => Box::new(full_length::EvenMutator::new(midi_out))
        }
    }
}

pub struct MultiArpeggiator<SG: SettingsGetter, SS: StatusSignal> {
    pub midi_in: InputDevice,
    pub midi_out: OutputDevice,
    pub settings: SG,
    pub status: SS
}

impl<SS: StatusSignal, SG: SettingsGetter> MultiArpeggiator<SG, SS> {
    pub fn listen(self) -> Result<(), Box<dyn Error>> {
        self.listen_with_midi_receivers(Vec::new())
    }

    pub fn listen_with_midi_receivers(mut self, mut extra_midi_receivers: Vec<&mut dyn MidiReceiver>) -> Result<(), Box<dyn Error>> {
        let mut mode = self.settings.get().mode;
        let mut current: Box<dyn Arpeggiator> = mode.create(&self.midi_out);
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
            let new_mode = self.settings.get().mode;
            if new_mode != mode {
                mode = new_mode;
                current.stop_arpeggios()?;
                current = new_mode.create(&self.midi_out);
                self.status.update_count(current.count_arpeggios());
            }
            // pass message through status
            if m.is_none() { continue; }
            m = self.status.passthrough_midi(m.unwrap());
            // process message in arp
            if m.is_none() { continue; }
            current.process(m.unwrap(), self.settings.get(), &mut self.status)?;
            self.status.update_count(current.count_arpeggios());
        }
    }
}

struct Passthrough<'a>(&'a OutputDevice);

impl<'a> Arpeggiator for Passthrough<'a> {
    fn process(&mut self, message: MidiMessage<'static>, _settings: &Settings, _signal: &mut dyn StatusSignal) -> Result<(), Box<dyn Error>> {
        self.0.send(message)?;
        Ok(())
    }

    fn stop_arpeggios(&mut self) -> Result<(), Box<dyn Error>> {
        Ok(())
    }

    fn count_arpeggios(&self) -> usize {
        1//TODO what shoudl this look like?
    }
}