use wmidi::{MidiMessage, U7};
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

// NOTE: EvenMutator does not support sustain pedal (but could in the future)
impl<'a> Arpeggiator for EvenMutator<'a> {
    fn process(&mut self, received: MidiMessage<'static>, settings: &Settings, status: &mut dyn StatusSignal) -> Result<(), Box<dyn Error>> {
        match received {
            MidiMessage::NoteOn(_, n, actual_v) => {
                let v = if let Some(fixed_v) = settings.fixed_velocity {
                    U7::from_u8_lossy(fixed_v)
                } else {
                    actual_v
                };
                match &mut self.arpeggio {
                    State::Playing(player) => player.note_on(n, v),
                    State::Starting(arp, _) => arp.note_on(n, v),
                    State::None => self.arpeggio = State::Starting(Arpeggio::from(n, v, settings.fixed_steps.unwrap_or(1), settings.pattern), START_THRESHOLD_TICKS)
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
                            let mut player = Player::init(arp, self.midi_out);
                            self.arpeggio = if player.play_tick()? {
                                State::Playing(player)
                            } else {
                                State::None
                            };
                        } else {
                            panic!()
                        }
                        status.reset_beat();
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
