use std::any::Any;
use std::collections::HashMap;
use std::time::Instant;
use std::sync::{mpsc, Arc, atomic::{AtomicBool, Ordering}};
use std::thread::{self, JoinHandle};
use std::error::Error;
use std::fmt;
use wmidi::{Note, MidiMessage};
use crate::midi;

pub struct Arpeggiator {
    midi_in: midi::InputDevice,
    midi_out: midi::OutputDevice,
    arpeggios: HashMap<Note, Arc<bool>>,
    held_notes: HashMap<Note, Instant>
}

impl Arpeggiator {
    pub fn new(midi_in: midi::InputDevice, midi_out: midi::OutputDevice) -> Self {
        Self {
            midi_in,
            midi_out,
            arpeggios: HashMap::new(),
            held_notes: HashMap::new()
        }
    }

    pub fn listen(&mut self) {
        for received in &self.midi_in.receiver {
            todo!();
        }
    }
}

pub struct Player {
    thread: JoinHandle<()>,
    should_stop: Arc<AtomicBool>
}

impl Player {
    fn start(arpeggio: Arpeggio, midi_out: &midi::OutputDevice) -> Result<Self, Box<dyn Error>> {
        let sender_cloned = midi_out.sender.clone();
        let should_stop = Arc::new(AtomicBool::new(false));
        let should_stop_cloned = Arc::clone(&should_stop);
        let thread = thread::Builder::new().name(format!("arp:{}", arpeggio)).spawn(move || arpeggio.play(sender_cloned, should_stop_cloned))?;
        Ok(Self {
            thread,
            should_stop
        })
    }

    fn stop(&mut self) {
        self.should_stop.store(true, Ordering::Relaxed);
    }

    fn ensure_stopped(mut self) -> Result<(), Box<dyn Any + Send>> {
        self.stop();
        self.thread.join()
    }
}

pub struct Arpeggio {
    name: String,
    trigger: Note,
    steps: Vec<Step>
}

impl fmt::Display for Arpeggio {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let notes: Vec<String> = self.steps.iter().map(|s| format!("{}", s)).collect(); //TODO avoid this allocation
        write!(f, "{}@{}bpm", notes.join(","), self.bpm())
    }
}

impl Arpeggio {
    fn play(&self, midi_out: mpsc::Sender<MidiMessage>, should_stop: Arc<AtomicBool>) {
        todo!();
    }

    fn bpm(&self) -> f64 {
        todo!()
    }
}

pub struct Step {
    offset: f64,
    notes: Vec<Note>
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.notes.len() == 1 {
            write!(f, "{}", self.notes[0])
        } else {
            let notes: Vec<String> = self.notes.iter().map(|n| format!("{}", n)).collect(); //TODO avoid this allocation
            write!(f, "[{}]", notes.join(","))
        }
    }
}

impl Step {
    pub fn send_on(&self, tx: &mpsc::Sender<MidiMessage>) {
        todo!();
    }

    pub fn send_off(&self, tx: &mpsc::Sender<MidiMessage>) {
        todo!();
    }
}