use std::sync::mpsc;
use std::fmt;
use wmidi::{Channel, MidiMessage, Note, Velocity};

use crate::midi::MidiOutput;

pub mod timed;
pub mod synced;
pub mod full_length;

#[derive(Copy, Clone)]
pub struct NoteDetails {
    pub n: Note,
    pub v: Velocity
}

impl NoteDetails  {
    pub fn new(n: Note, mut v: Velocity, fixed_velocity: Option<u8>) -> Self {
        if let Some(fixed) = fixed_velocity {
            v = if fixed >= u8::from(Velocity::MAX) {
                Velocity::MAX
            } else {
                fixed.try_into().unwrap()
            };
        }
        Self { n, v }
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
    pub fn send_on(&self, tx: &MidiOutput, channel: Channel) -> Result<(), mpsc::SendError<MidiMessage<'static>>> {
        for note in &self.notes {
            let message = MidiMessage::NoteOn(channel, note.n, note.v);
            tx.send(message)?;
        }
        Ok(())
    }

    pub fn send_off(&self, tx: &MidiOutput, channel: Channel) -> Result<(), mpsc::SendError<MidiMessage<'static>>> {
        for note in &self.notes {
            let message = MidiMessage::NoteOff(channel, note.n, note.v);
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
