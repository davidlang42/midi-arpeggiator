use std::any::Any;
use std::collections::HashMap;
use std::time::{Instant, Duration};
use std::sync::{mpsc, Arc, atomic::{AtomicBool, Ordering}};
use std::thread::{self, JoinHandle};
use std::error::Error;
use std::fmt;
use wmidi::{Note, MidiMessage, Velocity, Channel};
use crate::midi;

pub struct Arpeggiator {
    midi_in: midi::InputDevice,
    midi_out: midi::OutputDevice,
    arpeggios: HashMap<Note, Player>,
    held_notes: HashMap<Note, (Instant, Channel, Velocity)>,
    last_note_off: Option<(Note, Instant, Channel, Velocity)>
}

impl Arpeggiator {
    pub fn new(midi_in: midi::InputDevice, midi_out: midi::OutputDevice) -> Self {
        Self {
            midi_in,
            midi_out,
            arpeggios: HashMap::new(),
            held_notes: HashMap::new(),
            last_note_off: None
        }
    }

    pub fn listen(&mut self) {
        for received in &self.midi_in.receiver {
            match received {
                MidiMessage::NoteOn(c, n, v) => {
                    match self.last_note_off {
                        Some((first_n, first_i, first_c, first_v)) if first_n == n => {
                            let now = Instant::now();
                            let period = first_i - now;
                            //TODO add check that there wasn't a long gap between last note off and this note on
                            let mut steps = vec![Step {
                                wait: Duration::from_secs(0),
                                notes: vec![(first_c, first_n, first_v)]
                            }];
                            let mut prev_i = first_i;
                            let mut notes: Vec<(Note, (Instant, Channel, Velocity))> = self.held_notes.drain().collect();
                            notes.sort_by(|(_, (a, _, _)), (_, (b, _, _))| a.cmp(b));
                            for (n, (i, c, v)) in notes {
                                steps.push(Step {
                                    wait: i - prev_i,
                                    notes: vec![(c, n, v)] //TODO handle multiple notes in one step
                                });
                                prev_i = i;
                            }
                            steps[0].wait = now - prev_i;
                            let arp = Arpeggio { steps, period };
                            println!("Starting: {}", arp);
                            self.arpeggios.insert(n, Player::start(arp, &self.midi_out).unwrap()); //TODO handle error
                        },
                        _ => {
                            self.held_notes.insert(n, (Instant::now(), c, v));
                        }
                    }
                },
                MidiMessage::NoteOff(_, n, _) => {
                    if let Some(mut player) = self.arpeggios.remove(&n) {
                        println!("Stopping: {}", n);
                        player.stop();
                    } else if let Some((i, c, v)) = self.held_notes.remove(&n) {
                        self.last_note_off = Some((n, i, c, v));
                    } else {
                        self.last_note_off = None;
                    }
                },
                MidiMessage::Reset => {
                    self.held_notes.clear();
                    self.last_note_off = None;
                    for (_, player) in self.arpeggios.drain() {
                        player.ensure_stopped().unwrap(); //TODO handle error
                    }
                },
                _ => {}
            }
        }
    }
}

pub struct Player {
    thread: JoinHandle<Result<(), mpsc::SendError<MidiMessage<'static>>>>,
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
        let notes: Vec<String> = self.steps.iter().map(|s| format!("{}", s)).collect(); //TODO avoid this allocation
        write!(f, "{}@{:0.0}bpm", notes.join(","), self.bpm())
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
    wait: Duration,
    notes: Vec<(Channel, Note, Velocity)>
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.notes.len() == 1 {
            write!(f, "{}", self.notes[0].1)
        } else {
            let notes: Vec<String> = self.notes.iter().map(|n| format!("{}", n.1)).collect(); //TODO avoid this allocation
            write!(f, "[{}]", notes.join(","))
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