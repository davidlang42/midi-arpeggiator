use std::sync::mpsc;
use std::fmt;
use wmidi::{Note, MidiMessage, Velocity, Channel};

use crate::settings::Settings;

pub mod timed;
pub mod synced;

#[derive(Copy, Clone)]
pub struct NoteDetails {
    pub c: Channel,
    pub n: Note,
    pub v: Velocity
}

impl NoteDetails  {
    pub fn change_velocity(mut self, settings: &Settings) -> Self {
        if let Some(fixed) = settings.fixed_velocity {
            self.v = if fixed >= u8::from(Velocity::MAX) {
                Velocity::MAX
            } else {
                fixed.try_into().unwrap()
            }
        }
        self
    }
}

#[derive(Clone)]
pub struct Step {
    notes: Vec<NoteDetails>
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.notes.len() {
            0 => write!(f, "[]"),
            1 => write!(f, "{}", self.notes[0].n),
            len => {
                write!(f, "[{}", self.notes[0].n)?;
                for i in 1..len {
                    write!(f, ",{}", self.notes[i].n)?;
                }
                write!(f, "]")
            }
        }
    }
}

impl Step {
    pub fn send_on(&self, tx: &mpsc::Sender<MidiMessage<'static>>) -> Result<(), mpsc::SendError<MidiMessage<'static>>> {
        for note in &self.notes {
            let message = MidiMessage::NoteOn(note.c, note.n, note.v);
            tx.send(message)?;
        }
        Ok(())
    }

    pub fn send_off(&self, tx: &mpsc::Sender<MidiMessage<'static>>) -> Result<(), mpsc::SendError<MidiMessage<'static>>> {
        for note in &self.notes {
            let message = MidiMessage::NoteOff(note.c, note.n, note.v);
            tx.send(message)?;
        }
        Ok(())
    }

    fn highest_note(&self) -> Option<Note> {
        self.notes.iter().map(|d| d.n).max()
    }

    fn transpose(&self, half_steps: i8) -> Step {
        let mut notes = Vec::new();
        for d in &self.notes {
            if let Ok(new_n) = d.n.step(half_steps) {
                notes.push(NoteDetails {
                    c: d.c,
                    n: new_n,
                    v: d.v
                });
            }
        }
        Self {
            notes
        }
    }

    pub fn note(note: NoteDetails) -> Self {
        Self {
            notes: vec![note]
        }
    }

    pub fn notes(notes: Vec<NoteDetails>) -> Self {
        Self { notes }
    }
}
