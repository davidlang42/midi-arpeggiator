use std::any::Any;
use std::time::Duration;
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
    pub steps: Vec<Step>,
    pub period: Duration
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
        Ok(())
    }

    fn bpm(&self) -> f64 {
        let beats = self.steps.len() as f64;
        let seconds = self.period.as_secs_f64();
        beats / seconds * 60.0
    }
}

pub struct Step {
    pub wait: Duration,
    pub notes: Vec<(Channel, Note, Velocity)>
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.notes.len() {
            0 => write!(f, "[]"),
            1 => write!(f, "{}", self.notes[0].1),
            len => {
                write!(f, "[{}", self.notes[0].1)?;
                for i in 1..len {
                    write!(f, ",{}", self.notes[i].1)?;
                }
                write!(f, "]")
            }
        }
    }
}

impl Step {
    pub fn send_on(&self, tx: &mpsc::Sender<MidiMessage<'static>>) -> Result<(), mpsc::SendError<MidiMessage<'static>>> {
        for (channel, note, velocity) in &self.notes {
            let message = MidiMessage::NoteOn(*channel, *note, *velocity);
            tx.send(message.drop_unowned_sysex().unwrap())?;
        }
        Ok(())
    }

    pub fn send_off(&self, tx: &mpsc::Sender<MidiMessage<'static>>) -> Result<(), mpsc::SendError<MidiMessage<'static>>> {
        for (channel, note, velocity) in &self.notes {
            let message = MidiMessage::NoteOff(*channel, *note, *velocity);
            tx.send(message.drop_unowned_sysex().unwrap())?;
        }
        Ok(())
    }
}
