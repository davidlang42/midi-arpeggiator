use std::any::Any;
use std::time::{Duration, Instant};
use std::sync::{mpsc, Arc, atomic::{AtomicBool, Ordering}};
use std::thread::{self, JoinHandle};
use std::error::Error;
use std::fmt;
use wmidi::{Note, MidiMessage, Velocity, Channel};
use crate::midi;

pub struct Player {
    thread: JoinHandle<Result<(), mpsc::SendError<MidiMessage<'static>>>>,
    should_stop: Arc<AtomicBool>
}

impl Player {
    pub fn start(arpeggio: Arpeggio, midi_out: &midi::OutputDevice) -> Result<Self, Box<dyn Error>> {
        let sender_cloned = midi_out.sender.clone();
        let should_stop = Arc::new(AtomicBool::new(false));
        let should_stop_cloned = Arc::clone(&should_stop);
        let thread = thread::Builder::new().name(format!("arp:{}", arpeggio)).spawn(move || arpeggio.play(sender_cloned, should_stop_cloned))?;
        Ok(Self {
            thread,
            should_stop
        })
    }

    pub fn stop(&mut self) {
        self.should_stop.store(true, Ordering::Relaxed);
    }

    pub fn ensure_stopped(mut self) -> Result<(), Box<dyn Any + Send>> {
        self.stop();
        self.thread.join()?.unwrap(); //TODO handle error
        Ok(())
    }
}

pub struct Arpeggio {
    steps: Vec<Step>,
    period: Duration
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
        write!(f, "@{:0.0}bpm", self.bpm())
    }
}

impl Arpeggio {
    fn play(&self, midi_out: mpsc::Sender<MidiMessage<'static>>, should_stop: Arc<AtomicBool>) -> Result<(), mpsc::SendError<MidiMessage<'static>>> {
        let mut i = 0;
        println!("Playing: {}", self);
        while !should_stop.load(Ordering::Relaxed) {
            self.steps[i].send_on(&midi_out)?;
            let last_step = &self.steps[i];
            if i == self.steps.len() - 1 {
                i = 0;
            } else {
                i += 1;
            }
            thread::sleep(self.steps[i].wait);
            last_step.send_off(&midi_out)?;
        }
        println!("Stopped: {}", self);
        Ok(())
    }

    fn bpm(&self) -> f64 {
        let beats = self.steps.len() as f64;
        let seconds = self.period.as_secs_f64();
        beats / seconds * 60.0
    }

    pub fn first_note(&self) -> Note {
        if self.steps.len() == 0 {
            panic!("Arpeggios must have at least 1 step");
        }
        self.steps[0].highest_note()
    }

    pub fn from(notes: Vec<(Instant, NoteDetails)>, finish: Instant) -> Self {
        if notes.len() == 0 {
            panic!("Cannot construct an Arpeggio without any notes");
        }
        let start = notes[0].0;
        let period = finish - start;
        let mut steps = Vec::new();
        let mut prev_i = start;
        for (instant, note) in notes {
            steps.push(Step {
                wait: instant - prev_i,
                notes: vec![note]
            });
            prev_i = instant;
        }
        steps[0].wait = finish - prev_i;
        Arpeggio { steps, period }
    }

    pub fn transpose(&self, from: Note, to: Note) -> Arpeggio {
        let from_u8: u8 = from.into();
        let to_u8: u8 = to.into();
        let half_steps = to_u8 as i8 - from_u8 as i8;
        Self {
            period: self.period,
            steps: self.steps.iter().map(|s| s.transpose(half_steps)).collect()
        }
    }
}

pub struct Step {
    wait: Duration,
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
            wait: self.wait,
            notes
        }
    }
}

#[derive(Copy, Clone)]
pub struct NoteDetails {
    pub c: Channel,
    pub n: Note,
    pub v: Velocity
}
