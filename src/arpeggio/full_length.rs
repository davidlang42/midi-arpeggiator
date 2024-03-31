use std::{sync::mpsc, error::Error};
use std::fmt;
use wmidi::{Channel, MidiMessage, Note, Velocity, U7};
use crate::arpeggiator::Pattern;
use crate::midi::{self, TICKS_PER_BEAT};

const NOTE_MAX: usize = 127;

pub struct Arpeggio {
    notes: [Option<Velocity>; NOTE_MAX],
    ticks_per_step: usize,
    pattern: Pattern
}

impl fmt::Display for Arpeggio {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for i in 0..NOTE_MAX {
            if self.notes[i].is_some() {
                write!(f, "{} ", Note::from_u8_lossy(i as u8))?;
            }
        }
        write!(f, "@{}ticks/step", self.ticks_per_step)
    }
}

impl Arpeggio {
    pub fn from(n: Note, v: Velocity, notes_per_beat: usize, pattern: Pattern) -> Self {
        let mut arp = Self {
            notes: [None; NOTE_MAX],
            ticks_per_step: TICKS_PER_BEAT / notes_per_beat,
            pattern
        };
        arp.note_on(n, v);
        arp
    }

    pub fn note_on(&mut self, n: Note, v: Velocity) {
        self.notes[n as u8 as usize] = Some(v);
    }

    pub fn note_off(&mut self, n: Note) {
        self.notes[n as u8 as usize] = None;
    }
}

pub struct Player {
    midi_out: mpsc::Sender<MidiMessage<'static>>,
    arpeggio: Arpeggio,
    last_note: usize,
    wait_ticks: usize,
    pub should_stop: bool
}

impl Player {
    pub fn init(arpeggio: Arpeggio, midi_out: &midi::OutputDevice) -> Self {
        Self {
            arpeggio,
            last_note: NOTE_MAX - 1,
            wait_ticks: 0,
            should_stop: false,
            midi_out: midi_out.clone_sender()
        }
    }

    pub fn note_on(&mut self, n: Note, v: Velocity) {
        self.arpeggio.note_on(n, v)
    }

    pub fn note_off(&mut self, n: Note) {
        self.arpeggio.note_off(n)
    }

    pub fn play_tick(&mut self) -> Result<bool, mpsc::SendError<MidiMessage<'static>>>  {
        if self.should_stop {
            self.last_note_off()?;
            return Ok(false);
        }
        if self.wait_ticks == 0 {
            let mut next_note = self.last_note;
            loop {
                next_note = match self.arpeggio.pattern {
                    Pattern::Up => if next_note == NOTE_MAX - 1 {
                        0
                    } else {
                        next_note + 1
                    },
                    Pattern::Down => if next_note == 0 {
                        NOTE_MAX - 1
                    } else {
                        next_note - 1
                    }
                };
                if let Some(v) = &self.arpeggio.notes[next_note] {
                    // found the next note
                    self.last_note_off()?;
                    self.next_note_on(next_note, *v)?;
                    break;
                }
                if next_note == self.last_note {
                    // no notes are on
                    self.last_note_off()?;
                    return Ok(false);
                }
            }
            self.wait_ticks = self.arpeggio.ticks_per_step;
        }
        self.wait_ticks -= 1;
        Ok(true)
    }

    fn last_note_off(&self) -> Result<(), mpsc::SendError<MidiMessage<'static>>> {
        let message = MidiMessage::NoteOff(Channel::Ch1, Note::from_u8_lossy(self.last_note as u8), U7::MIN);
        self.midi_out.send(message)
    }

    fn next_note_on(&mut self, next_note: usize, v: Velocity) -> Result<(), mpsc::SendError<MidiMessage<'static>>> {
        self.last_note = next_note;
        let message = MidiMessage::NoteOn(Channel::Ch1, Note::from_u8_lossy(next_note as u8), v);
        self.midi_out.send(message)
    }

    pub fn stop(&mut self) {
        self.should_stop = true;
    }

    pub fn force_stop(&mut self) -> Result<(), Box<dyn Error>> {
        self.stop();
        self.wait_ticks = 0;
        if self.play_tick()? {
            Err(format!("Failed to force stop arpeggio").into())
        } else {
            Ok(())
        }
    }
}
