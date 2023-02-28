use std::error::Error;
use wmidi::MidiMessage;

use strum_macros::EnumIter;

use crate::arpeggio::{NoteDetails, Step};
use crate::midi;
use crate::settings::{Settings, SettingsGetter};

pub mod timed;
pub mod synced;

#[derive(Clone, EnumIter, Debug)]
pub enum Pattern {
    Up,
    Down
}

impl Pattern {
    pub fn of(&self, mut notes: Vec<NoteDetails>, steps: usize) -> Vec<Step> {
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
        let range = if notes.len() == 2 {
            // if there are only 2 notes, repeat them both
            (0..2).rev()
        } else {
            // otherwise repeat all except first and last notes
            (1..(notes.len() - 1)).rev()
        };
        for i in range {
            notes.push(notes[i].clone())
        }
    }
}

pub trait Arpeggiator {
    fn process(&mut self, message: MidiMessage<'static>, settings: &Settings) -> Result<(), Box<dyn Error>>;
    fn stop_arpeggios(&mut self) -> Result<(), Box<dyn Error>>;
}

#[derive(PartialEq, EnumIter, Copy, Clone, Debug)]
pub enum ArpeggiatorMode {
    RepeatRecorder,
    TimedPedalRecorder,
    PressHold,
    MutatingHold,
    SyncedPedalRecorder
}

impl ArpeggiatorMode {
    fn create<'a>(&self, midi_out: &'a midi::OutputDevice) -> Box<dyn Arpeggiator + 'a> {
        match self {
            Self::MutatingHold => Box::new(synced::MutatingHold::new(midi_out)),
            Self::PressHold => Box::new(synced::PressHold::new(midi_out)),
            Self::TimedPedalRecorder => Box::new(timed::PedalRecorder::new(midi_out)),
            Self::RepeatRecorder => Box::new(timed::RepeatRecorder::new(midi_out)),
            Self::SyncedPedalRecorder => Box::new(synced::PedalRecorder::new(midi_out))
        }
    }
}

pub struct MultiArpeggiator<'a> {
    midi_out: &'a midi::OutputDevice
}

impl<'a> MultiArpeggiator<'a> {
    pub fn new(midi_out: &'a midi::OutputDevice) -> Self {
        Self {
            midi_out
        }
    }

    pub fn listen<S: SettingsGetter>(self, midi_in: midi::InputDevice, mut settings: S) -> Result<(), Box<dyn Error>> {
        let mut mode = settings.get().mode;
        let mut current: Box<dyn Arpeggiator<S>> = mode.create(self.midi_out);
        for message in &midi_in.receiver {
            let pass_through = settings.passthrough_midi(message);
            let new_mode = settings.get().mode;
            if new_mode != mode {
                mode = new_mode;
                current.stop_arpeggios()?;
                current = new_mode.create(self.midi_out);
            }
            if let Some(passed_through) = pass_through {
                current.process(passed_through, settings.get())?;
            }
        }
        Ok(())
    }
}