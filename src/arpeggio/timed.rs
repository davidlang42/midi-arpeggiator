use std::error::Error;
use std::time::{Duration, Instant};
use std::sync::{mpsc, Arc, atomic::{AtomicBool, Ordering}};
use std::thread::{self, JoinHandle};
use std::fmt;
use wmidi::{Note, MidiMessage};
use crate::midi;

use super::{Step, NoteDetails};

pub struct Arpeggio {
    steps: Vec<(Duration, Step)>,
    period: Duration,
    finish_steps: bool
}

impl fmt::Display for Arpeggio {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.steps.len() {
            0 => write!(f, "-")?,
            len => {
                write!(f, "{}", self.steps[0].1)?;
                for i in 1..len {
                    write!(f, ",{}", self.steps[i].1)?;
                }
            }
        }
        write!(f, "@{:0.0}bpm", self.bpm())
    }
}

impl Arpeggio {
    fn play(&self, midi_out: mpsc::Sender<MidiMessage<'static>>, should_stop: Arc<AtomicBool>) -> Result<(), mpsc::SendError<MidiMessage<'static>>> {
        let mut i = 0;
        while !should_stop.load(Ordering::Relaxed) || (self.finish_steps && i != 0) {
            let step = &self.steps[i].1;
            step.send_on(&midi_out)?;
            if i == self.steps.len() - 1 {
                i = 0;
            } else {
                i += 1;
            }
            thread::sleep(self.steps[i].0);
            step.send_off(&midi_out)?;
        }
        Ok(())
    }
    
    fn bpm(&self) -> f64 {
        let beats = self.steps.len() as f64;
        let seconds = self.period.as_secs_f64();
        beats / seconds * 60.0
    }

    pub fn first_note(&self) -> Note {
        for (_, step) in &self.steps {
            if let Some(note) = step.highest_note() {
                return note;
            }
        }
        panic!("Arpeggio did not contain any notes");
    }

    pub fn from(notes: Vec<(Instant, NoteDetails)>, finish: Instant, finish_steps: bool) -> Self {
        if notes.len() == 0 {
            panic!("Cannot construct an Arpeggio without any notes");
        }
        let start = notes[0].0;
        let period = finish - start;
        let mut steps = Vec::new();
        let mut prev_i = start;
        for (instant, note) in notes {
            steps.push((instant - prev_i, Step::note(note)));
            prev_i = instant;
        }
        steps[0].0 = finish - prev_i;
        Self { steps, period, finish_steps }
    }

    pub fn transpose(&self, from: Note, to: Note) -> Self {
        let from_u8: u8 = from.into();
        let to_u8: u8 = to.into();
        let half_steps = to_u8 as i8 - from_u8 as i8;
        Self {
            period: self.period,
            steps: self.steps.iter().map(|(d, s)| (*d, s.transpose(half_steps))).collect(),
            finish_steps: self.finish_steps
        }
    }
}


pub struct Player{
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

    pub fn ensure_stopped(mut self) -> Result<(), Box<dyn Error>> {
        self.stop();
        match self.thread.join() {
            Ok(Err(inner)) => Err(Box::new(inner)),
            Err(_) => Err(format!("Thread paniced").into()),
            Ok(Ok(ok)) => Ok(ok)
        }
    }
}
