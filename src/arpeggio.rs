use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::{Ordering, AtomicBool};
use std::thread::JoinHandle;
use std::{sync::mpsc, any::Any};
use std::{fmt, thread};
use crate::midi;
use wmidi::{Note, MidiMessage, Velocity, Channel};

pub mod timed;
pub mod synced;

pub trait Arpeggio: Send + fmt::Display + 'static {
    fn play(&self, midi_out: mpsc::Sender<MidiMessage<'static>>, should_stop: Arc<AtomicBool>) -> Result<(), mpsc::SendError<MidiMessage<'static>>>;
}

#[derive(Copy, Clone)]
pub struct NoteDetails {
    pub c: Channel,
    pub n: Note,
    pub v: Velocity
}

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
    pub const EMPTY: Step = Step { notes: Vec::new() };

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

    fn highest_note(&self) -> Note {
        self.notes.iter().map(|d| d.n).max().expect("Steps must have at least 1 note")
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

    fn note(note: NoteDetails) -> Self {
        Self {
            notes: vec![note]
        }
    }

    fn interval(notes: Vec<NoteDetails>) -> Self {
        Self {
            notes
        }
    }

pub struct Player<A> {
    thread: JoinHandle<Result<A, mpsc::SendError<MidiMessage<'static>>>>,
    should_stop: Arc<AtomicBool>
}

impl<A: Arpeggio> Player<A> {
    pub fn start(arpeggio: A, midi_out: &midi::OutputDevice) -> Result<Self, Box<dyn Error>> {
        let sender_cloned = midi_out.sender.clone();
        let should_stop = Arc::new(AtomicBool::new(false));
        let should_stop_cloned = Arc::clone(&should_stop);
        let thread = thread::Builder::new().name(format!("arp:{}", arpeggio)).spawn(move || {
            arpeggio.play(sender_cloned, should_stop_cloned)?;
            Ok(arpeggio)
        })?;
        Ok(Self {
            thread,
            should_stop
        })
    }

    pub fn stop(&mut self) {
        self.should_stop.store(true, Ordering::Relaxed);
    }

    pub fn ensure_stopped(mut self) -> Result<A, Box<dyn Any + Send>> {
        self.stop();
        Ok(self.thread.join().unwrap().unwrap()) //TODO handle errors
    }
}
