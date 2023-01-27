use wmidi::{Note, MidiMessage};
use std::collections::{HashMap, HashSet};
use crate::midi;
use crate::arpeggio::NoteDetails;
use crate::arpeggio::synced::{Arpeggio, Player};
use super::{Arpeggiator, Pattern};

//TODO OrderedHold -> single arpeggio pressing/removing keys adds/removes keys from this arp
//TODO PressHold -> press all within a TICK, play in specified Pattern, release when all keys released (or hold if pedal on)

// pub struct OrderedHold {
//     midi_in: midi::InputDevice,
//     midi_out: midi::OutputDevice,
//     midi_clock: Option<midi::ClockDevice>,
//     held_notes: Vec<NoteDetails>,
//     arpeggio: Option<Player<Arpeggio>>,
//     finish_full_arpeggio: bool
// }

pub struct PressHold {
    midi_in: midi::InputDevice,
    midi_out: midi::OutputDevice,
    held_notes: HashMap<Note, NoteDetails>,
    arpeggios: Vec<(HashSet<Note>, Player)>,
    pattern: Pattern,
    finish_full_arpeggio: bool
}

impl PressHold {
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
                    self.held_notes.insert(n, NoteDetails { c, n, v });
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
                    if self.held_notes.len() != 0 {
                        let note_details: Vec<NoteDetails> = self.held_notes.drain().map(|(_, v)| v).collect();
                        let note_set: HashSet<Note> = note_details.iter().map(|d| d.n).collect();
                        let arp = Arpeggio::from(self.pattern.of(note_details), self.finish_full_arpeggio);
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
