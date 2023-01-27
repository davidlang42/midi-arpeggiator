use std::sync::{mpsc, Arc, atomic::{AtomicBool, Ordering}};
use std::fmt;
use wmidi::{Note, MidiMessage};
use crate::midi;
use super::{Step, NoteDetails};

pub struct Arpeggio {
    steps: Vec<Step>,
    ticks_per_step: usize,
    finish_steps: bool,
    clock: midi::ClockDevice
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

impl super::Arpeggio for Arpeggio {
    fn play(&self, midi_out: mpsc::Sender<MidiMessage<'static>>, should_stop: Arc<AtomicBool>) -> Result<(), mpsc::SendError<MidiMessage<'static>>> {
        let mut i = 0;
        let mut wait_ticks = 0;
        println!("Playing: {}", self);
        let mut step = &Step::EMPTY;
        loop {
            self.clock.wait_for_tick();//TODO handle error
            if should_stop.load(Ordering::Relaxed) {
                if !self.finish_steps || i == 0 {
                    break;
                }
            }
            if wait_ticks == 0 {
                step.send_off(&midi_out)?;
                step = &self.steps[i];
                step.send_on(&midi_out)?;
                if i == self.steps.len() - 1 {
                    i = 0;
                } else {
                    i += 1;
                }
                wait_ticks = self.ticks_per_step;
            } else {
                wait_ticks -= 1;
            }
        }
        step.send_off(&midi_out)?;
        println!("Stopped: {}", self);
        Ok(())
    }
}

impl Arpeggio {
    pub fn first_note(&self) -> Note {
        if self.steps.len() == 0 {
            panic!("Arpeggios must have at least 1 step");
        }
        self.steps[0].highest_note()
    }

    pub fn from(notes: Vec<NoteDetails>, finish_steps: bool, clock: midi::ClockDevice) -> Self {
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
        Self { steps, ticks_per_step, finish_steps, clock }
    }

    pub fn transpose(self, from: Note, to: Note) -> Self {
        let from_u8: u8 = from.into();
        let to_u8: u8 = to.into();
        let half_steps = to_u8 as i8 - from_u8 as i8;
        Self {
            ticks_per_step: self.ticks_per_step,
            steps: self.steps.iter().map(|s| s.transpose(half_steps)).collect(),
            finish_steps: self.finish_steps,
            clock: self.clock
        }
    }
}
