use wmidi::MidiMessage;
use std::error::Error;
use std::mem;
use crate::midi;
use crate::arpeggio::full_length::{Arpeggio, Player};
use crate::settings::Settings;
use crate::status::StatusSignal;
use super::Arpeggiator;

pub struct EvenMutator<'a> {
    midi_out: &'a midi::OutputDevice,
    arpeggio: State
}

enum State {
    Playing(Player),
    Starting(Arpeggio, u8),
    None
}

impl<'a> EvenMutator<'a> {
    pub fn new(midi_out: &'a midi::OutputDevice) -> Self {
        Self {
            midi_out,
            arpeggio: State::None
        }
    }
}

const START_THRESHOLD_TICKS: u8 = 2;

impl<'a> Arpeggiator for EvenMutator<'a> {
    fn process(&mut self, received: MidiMessage<'static>, settings: &Settings, _status: &mut dyn StatusSignal) -> Result<(), Box<dyn Error>> {
        match received {
            MidiMessage::NoteOn(_, n, _) => {
                match &mut self.arpeggio {
                    State::Playing(player) => player.note_on(n),
                    State::Starting(arp, _) => arp.note_on(n),
                    State::None => self.arpeggio = State::Starting(Arpeggio::from(n, settings.fixed_steps.unwrap_or(1)), START_THRESHOLD_TICKS)
                };
            },
            MidiMessage::NoteOff(_, n, _) => {
                match &mut self.arpeggio {
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
                        if !player.play_tick()? {
                            self.arpeggio = State::None;
                        }
                    },
                    State::Starting(_, 0) => {
                        let mut temp = State::None;
                        mem::swap(&mut self.arpeggio, &mut temp);
                        if let State::Starting(arp, _) = temp {
                            self.arpeggio = State::Playing(Player::init(arp, self.midi_out));
                        } else {
                            panic!()
                        }
                    },
                    State::Starting(_, n) => {
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
        if let State::Playing(player) = &mut self.arpeggio {
            player.force_stop()?;
            self.arpeggio = State::None;
        }
        Ok(())
    }

    fn count_arpeggios(&self) -> usize {
        if let State::Playing(_) = &self.arpeggio {
            1
        } else {
            0
        }
    }
}
