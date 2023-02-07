use std::{sync::mpsc, error::Error};
use std::fmt;
use wmidi::{Note, MidiMessage};
use crate::midi;

use super::{Step, NoteDetails};

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
    pub fn _first_note(&self) -> Note {
        if self.steps.len() == 0 {
            panic!("Arpeggios must have at least 1 step");
        }
        self.steps[0].highest_note()
    }

    pub fn from(notes: &Vec<NoteDetails>, finish_steps: bool) -> Self {
        if notes.len() == 0 {
            panic!("Cannot construct an Arpeggio without any notes");
        }
        let ticks_per_step = if notes.len() >= midi::TICKS_PER_BEAT {
            1
        } else {
            midi::TICKS_PER_BEAT / notes.len()
        };
        let steps = notes.iter().map(|n| Step::note(*n)).collect();
        Self { steps, ticks_per_step, finish_steps }
    }

    pub fn _transpose(&self, from: Note, to: Note) -> Self {
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
    midi_out: mpsc::Sender<MidiMessage<'static>>,
    arpeggio: Arpeggio,
    step: usize,
    last_index: Option<usize>,
    last_step: Option<Step>,
    wait_ticks: usize,
    pub should_stop: bool
}

impl Player {
    pub fn init(arpeggio: Arpeggio, midi_out: &midi::OutputDevice) -> Self {
        Self {
            arpeggio,
            step: 0,
            wait_ticks: 0,
            should_stop: false,
            last_index: None,
            last_step: None,
            midi_out: midi_out.sender.clone()
        }
    }

    pub fn play_tick(&mut self) -> Result<bool, mpsc::SendError<MidiMessage<'static>>>  {
        if self.arpeggio.steps.len() == 0 {
            return Ok(false);
        }
        if self.should_stop && !self.arpeggio.finish_steps {
            if let Some(last_index) = self.last_index {
                self.arpeggio.steps[last_index].send_off(&self.midi_out)?;
            } else if let Some(last_step) = &self.last_step {
                last_step.send_off(&self.midi_out)?;
                self.last_step = None;
            }
            return Ok(false);
        }
        if self.wait_ticks == 0 {
            if let Some(last_index) = self.last_index {
                self.arpeggio.steps[last_index].send_off(&self.midi_out)?;
            } else if let Some(last_step) = &self.last_step {
                last_step.send_off(&self.midi_out)?;
                self.last_step = None;
            }
            if self.should_stop && self.step == 0 {
                return Ok(false);
            }
            self.arpeggio.steps[self.step].send_on(&self.midi_out)?;
            self.last_index = Some(self.step);
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

    pub fn change_arpeggio(&mut self, arpeggio: Arpeggio) -> Result<(), mpsc::SendError<MidiMessage<'static>>>  {
        if let Some(last_index) = self.last_index {
            self.last_step = Some(self.arpeggio.steps.remove(last_index));
            self.last_index = None;
        }
        let steps_since_start = if self.step == 0 {
            self.arpeggio.steps.len()
        } else {
            self.step
        };
        let ticks_since_start = if steps_since_start * self.arpeggio.ticks_per_step <= self.wait_ticks {
            //TODO fix this properly
            //println!("Would have overflowed: {} * {} - {}", steps_since_start, self.arpeggio.ticks_per_step, self.wait_ticks);
            1
        } else {
            steps_since_start * self.arpeggio.ticks_per_step - self.wait_ticks
        };
        self.arpeggio = arpeggio;
        self.step = ((ticks_since_start - 1) / self.arpeggio.ticks_per_step + 1) % self.arpeggio.steps.len();
        self.wait_ticks = self.arpeggio.ticks_per_step - ((ticks_since_start - 1).rem_euclid(self.arpeggio.ticks_per_step) + 1);
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
