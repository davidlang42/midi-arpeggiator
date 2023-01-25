use std::collections::HashMap;
use std::time::{Instant, Duration};
use wmidi::{Note, MidiMessage, Velocity, Channel};
use crate::midi;
use crate::arpeggio::{Arpeggio, Player, Step};

pub trait Arpeggiator {
    fn listen(&mut self);
}

pub struct RepeatRecorder {
    midi_in: midi::InputDevice,
    midi_out: midi::OutputDevice,
    arpeggios: HashMap<Note, Player>,
    held_notes: HashMap<Note, (Instant, Channel, Velocity)>,
    last_note_off: Option<(Note, Instant, Channel, Velocity)>
}

impl RepeatRecorder {
    pub fn new(midi_in: midi::InputDevice, midi_out: midi::OutputDevice) -> Self {
        Self {
            midi_in,
            midi_out,
            arpeggios: HashMap::new(),
            held_notes: HashMap::new(),
            last_note_off: None
        }
    }
}

impl Arpeggiator for RepeatRecorder {
    fn listen(&mut self) {
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
