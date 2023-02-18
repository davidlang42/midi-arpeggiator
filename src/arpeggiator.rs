use std::error::Error;
use wmidi::MidiMessage;

use strum_macros::EnumIter;

use crate::{arpeggio::{NoteDetails, Step}, midi, settings::{FinishSettings, PatternSettings, MidiReceiver, ModeSettings}};

pub mod timed;
pub mod synced;

#[derive(Clone, EnumIter)]
pub enum Pattern {
    Down,
    Up
    //TODO more patterns: Random, Out, In
}

impl Pattern {
    pub fn of(&self, mut notes: Vec<NoteDetails>, steps: usize) -> Vec<Step> {
        // put the notes in order based on the pattern type
        match self {
            Pattern::Down => notes.sort_by(|a, b| a.n.cmp(&b.n)),
            Pattern::Up => notes.sort_by(|a, b| b.n.cmp(&a.n)),
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

pub trait Arpeggiator<S: MidiReceiver> {
    fn process(&mut self, message: MidiMessage<'static>) -> Result<(), Box<dyn Error>>;
    fn stop_arpeggios(&mut self) -> Result<(), Box<dyn Error>>;
    fn settings(&mut self) -> &mut S;

    fn listen(&mut self, midi_in: midi::InputDevice) -> Result<(), Box<dyn Error>> {
        for message in &midi_in.receiver {
            if let Some(passed_thru) = self.settings().passthrough_midi(message) {
                self.process(passed_thru)?;
            }
            //TODO handle abort message
        }
        Ok(())
    }
}

#[derive(PartialEq, EnumIter, Copy, Clone)]
pub enum ArpeggiatorMode {
    RepeatRecorder,
    TimedPedalRecorder,
    PressHold,
    MutatingHold,
    SyncedPedalRecorder
}

impl ArpeggiatorMode {
    fn create<'a, S: FinishSettings + PatternSettings>(&self, midi_out: &'a midi::OutputDevice, settings: &'a mut S) -> Box<dyn Arpeggiator<S> + 'a> {
        match self {
            Self::MutatingHold => Box::new(synced::MutatingHold::new(midi_out, settings)),
            Self::PressHold => Box::new(synced::PressHold::new(midi_out, settings)),
            Self::TimedPedalRecorder => Box::new(timed::PedalRecorder::new(midi_out, settings)),
            Self::RepeatRecorder => Box::new(timed::RepeatRecorder::new(midi_out, settings)),
            Self::SyncedPedalRecorder => Box::new(synced::PedalRecorder::new(midi_out, settings))
        }
    }
}

pub struct MultiArpeggiator<'a, S: FinishSettings + PatternSettings + ModeSettings> {
    current: Box<dyn Arpeggiator<S> + 'a>,
    mode: ArpeggiatorMode,
    midi_out: &'a midi::OutputDevice,
    settings: &'a mut S
}

impl<'a, S: FinishSettings + PatternSettings + ModeSettings> MultiArpeggiator<'a, S> {
    pub fn new(midi_out: &'a midi::OutputDevice, settings: &'a mut S) -> Self {
        let mode = settings.get_mode();
        Self {
            mode,
            midi_out,
            settings,
            current: mode.create(midi_out, settings)
        }
    }
}

impl<'a, S: FinishSettings + PatternSettings + ModeSettings> Arpeggiator<S> for MultiArpeggiator<'a, S> {
    fn process(&mut self, message: MidiMessage<'static>) -> Result<(), Box<dyn Error>> {
        let new_mode = self.settings.get_mode();
        if new_mode != self.mode {
            self.mode = new_mode;
            self.current.stop_arpeggios();
            self.current = self.mode.create(self.midi_out, self.settings);
        }
        self.current.process(message)
    }

    fn stop_arpeggios(&mut self) -> Result<(), Box<dyn Error>> {
        self.current.stop_arpeggios()
    }

    fn settings(&mut self) -> &'a mut S {
        &mut self.settings
    }
}