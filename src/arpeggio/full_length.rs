use std::{sync::mpsc, error::Error};
use std::fmt;
use wmidi::{Channel, MidiMessage, Note, U7};
use crate::midi::{self, TICKS_PER_BEAT};

const NOTE_MAX: usize = 127;

pub struct Arpeggio {
    notes: [bool; NOTE_MAX],
    ticks_per_step: usize
}

impl fmt::Display for Arpeggio {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        //TODO
        // match self.steps.len() {
        //     0 => write!(f, "-")?,
        //     len => {
        //         write!(f, "{}", self.steps[0])?;
        //         for i in 1..len {
        //             write!(f, ",{}", self.steps[i])?;
        //         }
        //     }
        // }
        // write!(f, "@{}ticks/step", self.ticks_per_step)
        write!(f, "TODO")
    }
}

impl Arpeggio {
    pub fn from(note: Note) -> Self {
        let mut arp = Self {
            notes: [false; NOTE_MAX],
            ticks_per_step: TICKS_PER_BEAT / 16 // semiquavers
        };
        arp.note_on(note);
        arp
    }

    pub fn note_on(&mut self, note: Note) {
        self.notes[note as u8 as usize] = true;
    }

    pub fn note_off(&mut self, note: Note) {
        self.notes[note as u8 as usize] = false;
    }
}

pub struct Player {
    midi_out: mpsc::Sender<MidiMessage<'static>>,
    arpeggio: Arpeggio,
    last_note: usize,
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
            last_note: NOTE_MAX,
            wait_ticks: 0,
            should_stop: false,
            midi_out: midi_out.clone_sender()
        }
    }

    pub fn note_on(&mut self, note: Note) {
        self.arpeggio.note_on(note)
    }

    pub fn note_off(&mut self, note: Note) {
        self.arpeggio.note_off(note)
    }

    pub fn play_tick(&mut self) -> Result<bool, mpsc::SendError<MidiMessage<'static>>>  {
        if self.should_stop {
            self.last_note_off()?;
            return Ok(false);
        }
        if self.wait_ticks == 0 {
            let mut next_note = self.last_note;
            loop {
                next_note = if next_note == NOTE_MAX {
                    0
                } else {
                    next_note + 1
                };
                if self.arpeggio.notes[next_note] {
                    break; // found the next note
                }
                if next_note == self.last_note {
                    return Ok(false); // no notes are on
                }
            }
            self.last_note_off()?;
            self.next_note_on(next_note)?;
            self.wait_ticks = self.arpeggio.ticks_per_step;
        }
        self.wait_ticks -= 1;
        Ok(true)
    }

    fn last_note_off(&self) -> Result<(), mpsc::SendError<MidiMessage<'static>>> {
        let message = MidiMessage::NoteOn(Channel::Ch1, Note::from_u8_lossy(self.last_note as u8), U7::from_u8_lossy(100));
        self.midi_out.send(message)
    }

    fn next_note_on(&mut self, next_note: usize) -> Result<(), mpsc::SendError<MidiMessage<'static>>> {
        self.last_note = next_note;
        let message = MidiMessage::NoteOn(Channel::Ch1, Note::from_u8_lossy(next_note as u8), U7::from_u8_lossy(100));
        self.midi_out.send(message)
    }

    pub fn stop(&mut self) {
        self.should_stop = true;
    }

    pub fn force_stop(&mut self) -> Result<(), Box<dyn Error>> {
        self.wait_ticks = 0;
        self.should_stop = true;
        if self.play_tick()? {
            Err(format!("Failed to force stop arpeggio").into())
        } else {
            Ok(())
        }
    }
}
