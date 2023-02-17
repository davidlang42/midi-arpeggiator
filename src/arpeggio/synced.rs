use std::{sync::mpsc, error::Error};
use std::fmt;
use wmidi::{Note, MidiMessage};
use crate::midi;
use super::{Step, NoteDetails};

pub struct Arpeggio {
    steps: Vec<(usize, Step)>,
    total_ticks: usize,
    finish_steps: bool
}

impl fmt::Display for Arpeggio {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.steps.len() {
            0 => write!(f, "-")?,
            len => {
                write!(f, "{}", self.steps[0].1)?;
                for i in 1..len {
                    write!(f, ",{}", self.steps[i].1)?;
                }
            }
        }
        write!(f, "@{}ticks/step", self.total_ticks / self.steps.len())
    }
}

impl Arpeggio {
    pub fn first_note(&self) -> Note {
        for (_, step) in self.steps {
            if let Some(note) = step.highest_note() {
                return note;
            }
        }
        panic!("Arpeggio did not contain any notes");
    }

    pub fn from_steps(steps: Vec<Step>, finish_steps: bool) -> Self {
        if steps.len() == 0 {
            panic!("Cannot construct an Arpeggio without any steps");
        }
        let ticks_per_step = if steps.len() >= midi::TICKS_PER_BEAT {
            1
        } else {
            midi::TICKS_PER_BEAT / steps.len()
        };
        let total_ticks = ticks_per_step * steps.len();
        Self {
            steps: steps.into_iter().map(|s| (ticks_per_step, s)).collect(),
            total_ticks,
            finish_steps
        }
    }

    pub fn from_notes(notes: Vec<(usize, NoteDetails)>, ticks_after_last_note: usize, finish_steps: bool) -> Self {
        if notes.len() == 0 {
            panic!("Cannot construct an Arpeggio without any notes");
        }
        let steps = Vec::with_capacity(notes.len() + 1);
        let mut next_step = Step::empty();
        let mut total_ticks = ticks_after_last_note;
        for (ticks_since_last_note, note) in notes {
            steps.push((ticks_since_last_note, next_step));
            next_step = Step::note(note);
            total_ticks += ticks_since_last_note;
        }
        steps.push((ticks_after_last_note, next_step));
        Self {
            steps,
            total_ticks,
            finish_steps
        }
    }

    pub fn transpose(&self, from: Note, to: Note) -> Self {
        let from_u8: u8 = from.into();
        let to_u8: u8 = to.into();
        let half_steps = to_u8 as i8 - from_u8 as i8;
        Self {
            total_ticks: self.total_ticks,
            steps: self.steps.iter().map(|(t, s)| (*t, s.transpose(half_steps))).collect(),
            finish_steps: self.finish_steps
        }
    }
}

pub struct Player {
    midi_out: mpsc::Sender<MidiMessage<'static>>,
    arpeggio: Arpeggio,
    step: usize,
    last_step: OptionIndex<Step>,
    wait_ticks: usize,
    pub should_stop: bool
}

enum OptionIndex<T> {
    None,
    Some(T),
    SomeIndex(usize)
}

impl Player {
    pub fn init(arpeggio: Arpeggio, midi_out: &midi::OutputDevice) -> Self {
        Self {
            arpeggio,
            step: 0,
            wait_ticks: 0,
            should_stop: false,
            last_step: OptionIndex::None,
            midi_out: midi_out.sender.clone()
        }
    }

    fn last_step_off(&self) -> Result<(), mpsc::SendError<MidiMessage<'static>>> {
        match &self.last_step {
            OptionIndex::SomeIndex(index) => self.arpeggio.steps[*index].1.send_off(&self.midi_out),
            OptionIndex::Some(step) => step.send_off(&self.midi_out),
            OptionIndex::None => Ok(())
        }
    }

    pub fn play_tick(&mut self) -> Result<bool, mpsc::SendError<MidiMessage<'static>>>  {
        if self.arpeggio.steps.len() == 0 {
            return Ok(false);
        }
        if self.should_stop && !self.arpeggio.finish_steps {
            self.last_step_off()?;
            return Ok(false);
        }
        if self.wait_ticks == 0 {
            self.last_step_off()?;
            if self.should_stop && self.step == 0 {
                return Ok(false);
            }
            let (wait_ticks, step) = self.arpeggio.steps[self.step];
            step.send_on(&self.midi_out)?;
            self.last_step = OptionIndex::SomeIndex(self.step);
            if self.step == self.arpeggio.steps.len() - 1 {
                self.step = 0;
            } else {
                self.step += 1;
            }
            self.wait_ticks = wait_ticks;
        }
        self.wait_ticks -= 1;
        Ok(true)
    }

    pub fn change_arpeggio(&mut self, arpeggio: Arpeggio) -> Result<(), mpsc::SendError<MidiMessage<'static>>> {
        if let OptionIndex::SomeIndex(index) = self.last_step {
            self.last_step = OptionIndex::Some(self.arpeggio.steps[index].1.clone());
        }
        let mut ticks_since_start;
        if self.step == 0 {
            ticks_since_start = self.arpeggio.total_ticks;
        } else {
            ticks_since_start = 0;
            for i in 0..self.step {
                ticks_since_start += self.arpeggio.steps[i].0;
            }
        }
        ticks_since_start -= self.wait_ticks;
        self.arpeggio = arpeggio;
        self.step = 0;
        while ticks_since_start > self.arpeggio.steps[self.step].0 {
            ticks_since_start -= self.arpeggio.steps[self.step].0;
            if self.step == self.arpeggio.steps.len() - 1 {
                self.step = 0;
            } else {
                self.step += 1;
            }
        }
        self.wait_ticks = self.arpeggio.steps[self.step].0 - ticks_since_start;
        Ok(())
    }

    pub fn stop(&mut self) {
        self.should_stop = true;
    }

    pub fn force_stop(&mut self) -> Result<(), Box<dyn Error>> {
        self.step = 0;
        self.wait_ticks = 0;
        self.should_stop = true;
        if self.play_tick()? {
            Err(format!("Failed to force stop arpeggio").into())
        } else {
            Ok(())
        }
    }
}
