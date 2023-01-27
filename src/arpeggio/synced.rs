use std::sync::mpsc;
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

    pub fn from(notes: Vec<NoteDetails>, finish_steps: bool) -> Self {
        const TICKS_PER_QUARTER_NOTE: usize = 24;
        if notes.len() == 0 {
            panic!("Cannot construct an Arpeggio without any notes");
        }
        let ticks_per_step = if notes.len() >= 24 {
            1
        } else {
            TICKS_PER_QUARTER_NOTE / notes.len()
        };
        let steps = notes.into_iter().map(|n| Step::note(n)).collect();
        Self { steps, ticks_per_step, finish_steps }
    }

    pub fn _transpose(self, from: Note, to: Note) -> Self {
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
    last_step: Option<usize>,
    wait_ticks: usize,
    should_stop: bool
}

impl Player {
    pub fn init(arpeggio: Arpeggio, midi_out: &midi::OutputDevice) -> Self {
        Self {
            arpeggio,
            step: 0,
            wait_ticks: 0,
            should_stop: false,
            last_step: None,
            midi_out: midi_out.sender.clone()
        }
    }

    pub fn play_tick(&mut self) -> Result<bool, mpsc::SendError<MidiMessage<'static>>>  {
        if self.should_stop && !self.arpeggio.finish_steps {
            if let Some(last_step) = self.last_step {
                self.arpeggio.steps[last_step].send_off(&self.midi_out)?;
            }
            return Ok(false);
        }
        if self.wait_ticks == 0 {
            if let Some(last_step) = self.last_step {
                self.arpeggio.steps[last_step].send_off(&self.midi_out)?;
            }
            if self.should_stop && self.step == 0 {
                return Ok(false);
            }
            self.arpeggio.steps[self.step].send_on(&self.midi_out)?;
            self.last_step = Some(self.step);
            if self.step == self.arpeggio.steps.len() - 1 {
                self.step = 0;
            } else {
                self.step += 1;
            }
            self.wait_ticks = self.arpeggio.ticks_per_step;
        } else {
            self.wait_ticks -= 1;
        }
        Ok(true)
    }

    pub fn stop(&mut self) {
        self.should_stop = true;
    }

    pub fn force_stop(&mut self) {
        self.step = 0;
        self.wait_ticks = 0;
        self.should_stop = true;
        if self.play_tick().unwrap() {
            panic!("Failed to force stop arpeggio");
        }
    }
}
