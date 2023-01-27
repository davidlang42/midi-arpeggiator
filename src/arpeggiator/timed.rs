use std::collections::HashMap;
use std::mem;
use std::time::Instant;
use wmidi::{Note, MidiMessage, ControlFunction};
use crate::midi;
use crate::arpeggio::NoteDetails;
use crate::arpeggio::timed::{Arpeggio, Player};
use super::Arpeggiator;

pub struct RepeatRecorder {
    midi_in: midi::InputDevice,
    midi_out: midi::OutputDevice,
    held_notes: HashMap<Note, (Instant, NoteDetails)>,
    last_note_off: Option<(Instant, NoteDetails)>,
    arpeggios: HashMap<Note, Player>,
}

impl RepeatRecorder {
    pub fn new(midi_in: midi::InputDevice, midi_out: midi::OutputDevice) -> Self {
        Self {
            midi_in,
            midi_out,
            held_notes: HashMap::new(),
            last_note_off: None,
            arpeggios: HashMap::new()
        }
    }
}

impl Arpeggiator for RepeatRecorder {
    fn listen(&mut self) {
        for received in &self.midi_in.receiver {
            match received {
                MidiMessage::NoteOn(c, n, v) => {
                    match &self.last_note_off {
                        Some((first_i, first)) if first.n == n => {
                            let finish = Instant::now();
                            //TODO add check that there wasn't a long gap between last note off and this note on
                            //TODO handle multiple notes in one step
                            let mut notes: Vec<(Instant, NoteDetails)> = self.held_notes.drain().map(|(_, v)| v).collect();
                            notes.push((*first_i, *first));
                            notes.sort_by(|(a, _), (b, _)| a.cmp(&b));
                            let arp = Arpeggio::from(notes, finish);
                            self.arpeggios.insert(n, Player::start(arp, &self.midi_out).unwrap()); //TODO handle error
                        },
                        _ => {
                            self.held_notes.insert(n, (Instant::now(), NoteDetails { c, n, v }));
                        }
                    }
                },
                MidiMessage::NoteOff(_, n, _) => {
                    if let Some(mut player) = self.arpeggios.remove(&n) {
                        player.stop();
                    } else if let Some(value) = self.held_notes.remove(&n) {
                        self.last_note_off = Some(value);
                    } else {
                        self.last_note_off = None;
                    }
                },
                MidiMessage::Reset => {
                    self.held_notes.clear();
                    self.last_note_off = None;
                    drain_and_wait_for_stop(&mut self.arpeggios);
                },
                _ => {}
            }
        }
    }

    fn stop_arpeggios(&mut self) {
        drain_and_wait_for_stop(&mut self.arpeggios);
    }
}

pub struct PedalRecorder {
    midi_in: midi::InputDevice,
    midi_out: midi::OutputDevice,
    notes: Vec<(Instant, NoteDetails)>,
    thru_notes: HashMap<Note, NoteDetails>,
    pedal: bool,
    arpeggios: HashMap<Note, Player>,
    recorded: Option<Arpeggio>
}

impl PedalRecorder {
    pub fn new(midi_in: midi::InputDevice, midi_out: midi::OutputDevice) -> Self {
        Self {
            midi_in,
            midi_out,
            notes: Vec::new(),
            thru_notes: HashMap::new(),
            pedal: false,
            arpeggios: HashMap::new(),
            recorded: None
        }
    }
}

impl Arpeggiator for PedalRecorder {
    fn listen(&mut self) {
        for received in &self.midi_in.receiver {
            match received {
                MidiMessage::ControlChange(_, ControlFunction::DAMPER_PEDAL, value) => {
                    if u8::from(value) >= 64 {
                        self.pedal = true;
                        self.recorded = None;
                        drain_and_stop(&mut self.arpeggios);
                    } else {
                        self.pedal = false;
                        for (_, thru_note) in self.thru_notes.drain() {
                            if self.midi_out.sender.send(MidiMessage::NoteOff(thru_note.c, thru_note.n, thru_note.v)).is_err() {
                                panic!("Unable to send to output queue");
                            }
                        }
                        if self.notes.len() > 0 {
                            // save recorded arpeggio
                            let finish = Instant::now();
                            let notes = mem::replace(&mut self.notes, Vec::new());
                            self.recorded = Some(Arpeggio::from(notes, finish));
                            // start play in original key
                            let arp = self.recorded.as_ref().unwrap();
                            let original = arp.first_note();
                            let new_arp = arp.transpose(original, original);
                            self.arpeggios.insert(original, Player::start(new_arp, &self.midi_out).unwrap()); //TODO handle error
                        }
                    }
                },
                MidiMessage::NoteOn(c, n, v) => {
                    if self.pedal {
                        if self.midi_out.sender.send(received).is_err() {
                            panic!("Unable to forward to output queue");
                        }
                        let d = NoteDetails { c, n, v };
                        self.thru_notes.insert(n, d);
                        self.notes.push((Instant::now(), d));
                    } else if self.arpeggios.contains_key(&n) {
                        // already playing, do nothing
                    } else if let Some(arp) = &self.recorded {
                        let original = arp.first_note();
                        let new_arp = arp.transpose(original, n);
                        self.arpeggios.insert(n, Player::start(new_arp, &self.midi_out).unwrap()); //TODO handle error
                    }
                },
                MidiMessage::NoteOff(_, n, _) => {
                    if self.pedal {
                        if self.midi_out.sender.send(received).is_err() {
                            panic!("Unable to forward to output queue");
                        }
                        self.thru_notes.remove(&n);
                    } else if let Some(mut player) = self.arpeggios.remove(&n) {
                        player.stop();
                    }
                },
                MidiMessage::Reset => {
                    self.notes.clear();
                    self.pedal = false;
                    drain_and_wait_for_stop(&mut self.arpeggios);
                },
                _ => {}
            }
        }
    }

    fn stop_arpeggios(&mut self) {
        drain_and_wait_for_stop(&mut self.arpeggios);
    }
}

fn drain_and_stop<N>(arpeggios: &mut HashMap<N, Player>) -> Vec<Player> {
    let mut players = Vec::new();
    for (_, mut player) in arpeggios.drain() {
        player.stop();
        players.push(player);
    }
    players
}

fn drain_and_wait_for_stop<N>(arpeggios: &mut HashMap<N, Player>) {
    for player in drain_and_stop(arpeggios) {
        player.ensure_stopped().unwrap(); //TODO handle error
    }
}
