use std::{sync::mpsc, error::Error};
use std::fmt;
use wmidi::{Note, MidiMessage};
use crate::midi::{self, MidiOutput};
use crate::presets::Preset;
use super::{NoteDetails, Step};

pub struct Arpeggio {
    steps: Vec<Step>,
    ticks_per_step: usize,
    finish_steps: bool
}

impl fmt::Display for Arpeggio {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.steps.len() {
            0 => write!(f, "-")?,
            len => {
                write!(f, "{}", self.steps[0])?;
                for i in 1..len {
                    write!(f, ",{}", self.steps[i])?;
                }
            }
        }
        write!(f, "@{}ticks/step", self.ticks_per_step)
    }
}

impl Arpeggio {
    pub fn first_note(&self) -> Note {
        for step in &self.steps {
            if let Some(note) = step.highest_note() {
                return note;
            }
        }
        panic!("Arpeggio did not contain any notes");
    }

    pub fn from(steps: Vec<Step>, total_beats: usize, finish_steps: bool) -> Self {
        if steps.len() == 0 {
            panic!("Cannot construct an Arpeggio without any steps");
        }
        let total_ticks = total_beats * midi::TICKS_PER_BEAT;
        let ticks_per_step = if steps.len() >= total_ticks {
            1
        } else {
            total_ticks / steps.len()
        };
        Self { steps, ticks_per_step, finish_steps }
    }

    pub fn from_preset(preset: &Preset, c: wmidi::Channel, v: wmidi::Velocity, finish_steps: bool) -> Self {
        let mut steps = Vec::new();
        for n in &preset.steps {
            steps.push(Step::note(NoteDetails { c, n: n.into(), v }));
        }
        Self {
            steps,
            ticks_per_step: preset.ticks_per_step,
            finish_steps
        }
    }

    pub fn transpose(&self, from: Note, to: Note) -> Self {
        let from_u8: u8 = from.into();
        let to_u8: u8 = to.into();
        let half_steps = to_u8 as i8 - from_u8 as i8;
        Self {
            ticks_per_step: self.ticks_per_step,
            steps: self.steps.iter().map(|s| s.transpose(half_steps)).collect(),
            finish_steps: self.finish_steps
        }
    }
}

pub struct Player {
    midi_out: MidiOutput,
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
    pub fn init(arpeggio: Arpeggio, midi_out: &midi::OutputDevice, doubling: &Option<Vec<i8>>) -> Self {
        Self {
            arpeggio,
            step: 0,
            wait_ticks: 0,
            should_stop: false,
            last_step: OptionIndex::None,
            midi_out: midi_out.with_doubling(doubling)
        }
    }

    fn last_step_off(&self) -> Result<(), mpsc::SendError<MidiMessage<'static>>> {
        match &self.last_step {
            OptionIndex::SomeIndex(index) => self.arpeggio.steps[*index].send_off(&self.midi_out),
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
            self.arpeggio.steps[self.step].send_on(&self.midi_out)?;
            self.last_step = OptionIndex::SomeIndex(self.step);
            if self.step == self.arpeggio.steps.len() - 1 {
                self.step = 0;
            } else {
                self.step += 1;
            }
            self.wait_ticks = self.arpeggio.ticks_per_step;
        }
        self.wait_ticks -= 1;
        Ok(true)
    }

    pub fn change_arpeggio(&mut self, arpeggio: Arpeggio) -> Result<(), mpsc::SendError<MidiMessage<'static>>> {
        if let OptionIndex::SomeIndex(index) = self.last_step {
            self.last_step = OptionIndex::Some(self.arpeggio.steps[index].clone());
        }
        let steps_since_start = if self.step == 0 {
            self.arpeggio.steps.len()
        } else {
            self.step
        };
        let ticks_since_start = steps_since_start * self.arpeggio.ticks_per_step - self.wait_ticks;
        let ticks_since_start_minus_1 = if ticks_since_start == 0 {
            0
        } else {
            ticks_since_start - 1
        };
        self.arpeggio = arpeggio;
        self.step = (ticks_since_start_minus_1 / self.arpeggio.ticks_per_step + 1) % self.arpeggio.steps.len();
        self.wait_ticks = self.arpeggio.ticks_per_step - (ticks_since_start_minus_1.rem_euclid(self.arpeggio.ticks_per_step) + 1);
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
