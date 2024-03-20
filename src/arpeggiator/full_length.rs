use wmidi::{Note, MidiMessage, ControlFunction};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::mem;
use std::time::Instant;
use crate::midi;
use crate::arpeggio::{NoteDetails, Step};
use crate::arpeggio::full_length::{Arpeggio, Player};
use crate::settings::Settings;
use crate::status::StatusSignal;
use super::Arpeggiator;

pub struct MutatingHold<'a> {
    midi_out: &'a midi::OutputDevice,
    arpeggio: State
}

enum State {
    Playing(Player),
    Starting(Arpeggio, u8),
    None
}

impl<'a> MutatingHold<'a> {
    pub fn new(midi_out: &'a midi::OutputDevice) -> Self {
        Self {
            midi_out,
            arpeggio: State::None
        }
    }
}

const START_THRESHOLD_TICKS: u8 = 4;

impl<'a> Arpeggiator for MutatingHold<'a> {
    fn process(&mut self, received: MidiMessage<'static>, settings: &Settings, status: &mut dyn StatusSignal) -> Result<(), Box<dyn Error>> {
        match received {
            MidiMessage::NoteOn(_, n, _) => {
                match &self.arpeggio {
                    State::Playing(player) => player.note_on(n),
                    State::Starting(arp, _) => arp.note_on(n),
                    State::None => self.arpeggio = State::Starting(Arpeggio::from(n), START_THRESHOLD_TICKS)
                };
            },
            MidiMessage::NoteOff(_, n, _) => {
                match &self.arpeggio {
                    State::Playing(player) => player.note_off(n),
                    State::Starting(arp, _) => arp.note_off(n),
                    State::None => { }
                };
            },
            MidiMessage::TimingClock => {
                //TODO whatever this is?
                // if self.changed && (self.arpeggio.is_none() || !self.arpeggio.as_ref().unwrap().should_stop) { // don't process new notes the arp is already stopping
                //     self.changed = false;
                //     if self.held_notes.len() == 0 {
                //         if let Some(existing) = &mut self.arpeggio {
                //             existing.stop();
                //         }
                //     } else {
                //         let steps: Vec<Step> = self.held_notes.iter().map(|n| Step::note(*n)).collect();
                //         let steps_len = steps.len();
                //         let arp = Arpeggio::from(steps, steps_len, settings.finish_pattern);
                //         if let Some(existing) = &mut self.arpeggio {
                //             existing.change_arpeggio(arp)?;
                //         } else {
                //             self.arpeggio = Some(Player::init(arp, &self.midi_out));
                //             status.reset_beat();
                //         }
                //     }
                // }
                match &mut self.arpeggio {
                    State::Playing(player) => {
                        if !arp.play_tick()? {
                            self.arpeggio = State::None;
                        }
                    },
                    State::Starting(arp, 0) => {
                        self.arpeggio = State::Playing(Player::init(arp, self.midi_out));
                    },
                    State::Starting(arp, n) => {
                        *n -= 1;
                    },
                    State::None => { }
                };
            },
            _ => {}
        }
        Ok(())
    }

    fn stop_arpeggios(&mut self) -> Result<(), Box<dyn Error>> {
        if let Some(arp) = &mut self.arpeggio {
            arp.force_stop()?;
            self.arpeggio = None;
        }
        Ok(())
    }

    fn count_arpeggios(&self) -> usize {
        if self.arpeggio.is_some() {
            1
        } else {
            0
        }
    }
}

fn drain_and_force_stop_vec<N>(arpeggios: &mut Vec<(N, Player)>) -> Result<(), Box<dyn Error>> {
    for (_, mut player) in arpeggios.drain(0..arpeggios.len()) {
        player.force_stop()?;
    }
    Ok(())
}

fn drain_and_force_stop_map<N>(arpeggios: &mut HashMap<N, Player>) -> Result<(), Box<dyn Error>> {
    for (_, mut player) in arpeggios.drain() {
        player.force_stop()?;
    }
    Ok(())
}
