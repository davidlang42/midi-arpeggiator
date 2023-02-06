use wmidi::{Note, MidiMessage};
use std::collections::{HashMap, HashSet};
use std::time::Instant;
use crate::midi;
use crate::arpeggio::NoteDetails;
use crate::arpeggio::synced::{Arpeggio, Player};
use super::{Arpeggiator, Pattern};

pub struct PressHold {
    midi_in: midi::InputDevice,
    midi_out: midi::OutputDevice,
    held_notes: HashMap<Note, (Instant, NoteDetails)>,
    arpeggios: Vec<(HashSet<Note>, Player)>,
    pattern: Pattern,
    finish_full_arpeggio: bool
}

impl PressHold {
    const TRIGGER_TIME_MS: u128 = 50;

    pub fn new(midi_in: midi::InputDevice, midi_out: midi::OutputDevice, pattern: Pattern, finish_full_arpeggio: bool) -> Self {
        Self {
            midi_in,
            midi_out,
            held_notes: HashMap::new(),
            arpeggios: Vec::new(),
            pattern,
            finish_full_arpeggio
        }
    }
}

impl<'a> Arpeggiator for PressHold {
    fn listen(&mut self) {
        for received in &self.midi_in.receiver {
            match received {
                //TODO handle pedal up/down
                MidiMessage::NoteOn(c, n, v) => {
                    self.held_notes.insert(n, (Instant::now(), NoteDetails { c, n, v }));
                },
                MidiMessage::NoteOff(_, n, _) => {
                    self.held_notes.remove(&n);
                    for (note_set, player) in self.arpeggios.iter_mut() {
                        if note_set.remove(&n) && note_set.len() == 0 {
                            player.stop();
                        }
                    }
                },
                MidiMessage::TimingClock => {
                    if self.held_notes.len() != 0 && self.held_notes.values().map(|(i, _)| i).min().unwrap().elapsed().as_millis() > Self::TRIGGER_TIME_MS {
                        let note_details: Vec<NoteDetails> = self.held_notes.drain().map(|(_, (_, d))| d).collect();
                        let note_set: HashSet<Note> = note_details.iter().map(|d| d.n).collect();
                        let arp = Arpeggio::from(&self.pattern.of(note_details), self.finish_full_arpeggio);
                        println!("Arp: {}", arp);
                        self.arpeggios.push((note_set, Player::init(arp, &self.midi_out)));
                    }
                    let mut i = 0;
                    while i < self.arpeggios.len() {
                        if !self.arpeggios[i].1.play_tick().unwrap() { //TODO handle error
                            self.arpeggios.remove(i);
                        } else {
                            i += 1;
                        }
                    }
                },
                MidiMessage::Reset => {
                    self.held_notes.clear();
                    drain_and_force_stop(&mut self.arpeggios);
                },
                _ => {}
            }
        }
    }

    fn stop_arpeggios(&mut self) {
        drain_and_force_stop(&mut self.arpeggios);
    }
}

pub struct MutatingHold {
    midi_in: midi::InputDevice,
    midi_out: midi::OutputDevice,
    held_notes: Vec<NoteDetails>,
    changed: bool,
    arpeggio: Option<Player>,
    finish_full_arpeggio: bool
}

impl MutatingHold {
    pub fn new(midi_in: midi::InputDevice, midi_out: midi::OutputDevice, finish_full_arpeggio: bool) -> Self {
        Self {
            midi_in,
            midi_out,
            held_notes: Vec::new(),
            changed: false,
            arpeggio: None,
            finish_full_arpeggio
        }
    }
}

impl<'a> Arpeggiator for MutatingHold {
    fn listen(&mut self) {
        for received in &self.midi_in.receiver {
            match received {
                //TODO handle pedal up/down
                MidiMessage::NoteOn(c, n, v) => {
                    self.held_notes.push(NoteDetails { c, n, v });
                    self.changed = true;
                },
                MidiMessage::NoteOff(_, n, _) => {
                    let mut i = 0;
                    while i < self.held_notes.len() {
                        if self.held_notes[i].n == n {
                            self.held_notes.remove(i);
                            self.changed = true;
                        } else {
                            i += 1;
                        }
                    }
                },
                MidiMessage::TimingClock => {
                    if self.changed && (self.arpeggio.is_none() || !self.arpeggio.as_ref().unwrap().should_stop) { // don't process new notes the arp is already stopping
                        self.changed = false;
                        if self.held_notes.len() == 0 {
                            if let Some(existing) = &mut self.arpeggio {
                                existing.stop();
                            }
                        } else {
                            let arp = Arpeggio::from(&self.held_notes, self.finish_full_arpeggio);
                            println!("Arp: {}", arp);
                            if let Some(existing) = &mut self.arpeggio {
                                existing.change_arpeggio(arp).unwrap(); //TODO handle error
                            } else {
                                self.arpeggio = Some(Player::init(arp, &self.midi_out));
                            }
                        }
                    }
                    if let Some(arp) = &mut self.arpeggio {
                        if !arp.play_tick().unwrap() { //TODO handle error
                            self.arpeggio = None;
                        }
                    }
                },
                MidiMessage::Reset => {
                    self.held_notes.clear();
                    if let Some(arp) = &mut self.arpeggio {
                        arp.force_stop();
                        self.arpeggio = None;
                    }
                },
                _ => {}
            }
        }
    }

    fn stop_arpeggios(&mut self) {
        if let Some(arp) = &mut self.arpeggio {
            arp.force_stop();
            self.arpeggio = None;
        }
    }
}

fn drain_and_force_stop<N>(arpeggios: &mut Vec<(N, Player)>) {
    if arpeggios.len() != 0 {
        let mut i = arpeggios.len() - 1;
        loop {
            arpeggios[i].1.force_stop();
            arpeggios.remove(i);
            if i == 0 {
                break;
            } else {
                i -= 1;
            }
        }
    }
}
