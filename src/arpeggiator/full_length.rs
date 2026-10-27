use wmidi::{Channel, MidiMessage, Note, Velocity, U7};
use std::collections::HashSet;
use std::error::Error;
use std::mem;
use std::sync::mpsc;
use crate::arpeggiator::Pattern;
use crate::midi;
use crate::arpeggio::full_length::{Arpeggio, Player};
use crate::presets::Preset;
use crate::settings::Settings;
use crate::status::StatusSignal;
use super::Arpeggiator;

pub struct EvenMutator<'a> {
    midi_out: &'a midi::OutputDevice,
    channel_out: Channel,
    arpeggio: State,
    // from Settings:
    fixed_velocity: Option<u8>,
    fixed_steps: Option<usize>, // assumed in 1 beat
    pattern: Pattern,
    double_notes: Option<Vec<i8>>,
}

enum State {
    Playing(Player),
    Starting(Arpeggio, u8),
    None
}

impl<'a> EvenMutator<'a> {
    pub fn new(midi_out: &'a midi::OutputDevice, channel_out: Channel, settings: &Settings) -> Self {
        Self {
            midi_out,
            channel_out,
            arpeggio: State::None,
            fixed_velocity: settings.fixed_velocity,
            fixed_steps: settings.fixed_steps,
            pattern: settings.pattern,
            double_notes: settings.double_notes.clone(),
        }
    }
}

const START_THRESHOLD_TICKS: u8 = 2;

// NOTE: EvenMutator does not support sustain pedal (but could in the future)
impl<'a> Arpeggiator for EvenMutator<'a> {
    fn process(&mut self, received: MidiMessage<'static>, status: &mut dyn StatusSignal) -> Result<(), Box<dyn Error>> {
        match received {
            MidiMessage::NoteOn(_, n, actual_v) => {
                let v = if let Some(fixed_v) = self.fixed_velocity {
                    U7::from_u8_lossy(fixed_v)
                } else {
                    actual_v
                };
                match &mut self.arpeggio {
                    State::Playing(player) => player.note_on(n, v),
                    State::Starting(arp, _) => arp.note_on(n, v),
                    State::None => self.arpeggio = State::Starting(Arpeggio::from(n, v, self.fixed_steps.unwrap_or(1), self.pattern), START_THRESHOLD_TICKS)
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
                            let mut player = Player::init(arp, self.midi_out, self.channel_out, &self.double_notes);
                            self.arpeggio = if player.play_tick()? {
                                State::Playing(player)
                            } else {
                                State::None
                            };
                        } else {
                            panic!()
                        }
                        status.reset_beat(self.channel_out);
                    },
                    State::Starting(_, n) => {
                        *n -= 1;
                    },
                    State::None => { }
                };
            },
            _ => {}
        }
        status.update_count(self.channel_out, self.count_arpeggios());
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

pub struct TriggeredChords<'a> {
    midi_out: &'a midi::OutputDevice,
    channel_out: Channel,
    not_playing: HashSet<Preset>,
    notes: HashSet<Note>,
    playing: HashSet<Preset>,
    // from Settings:
    fixed_velocity: Option<u8>,
}

impl<'a> TriggeredChords<'a> {
    pub fn new(midi_out: &'a midi::OutputDevice, channel_out: Channel, settings: &Settings) -> Self {
        Self {
            midi_out,
            channel_out,
            not_playing: HashSet::from_iter(settings.presets.clone().unwrap_or_else(|| Vec::new())), // not helpful if presets is missing, but better than crashing
            notes: HashSet::new(),
            playing: HashSet::new(),
            fixed_velocity: settings.fixed_velocity,
        }
    }
}

impl<'a> TriggeredChords<'a> {
    fn play(&self, midi_out: &midi::OutputDevice, preset: &Preset, velocity: &Velocity) -> Result<(), mpsc::SendError<MidiMessage<'static>>> {
        for n in preset.steps.iter() {
            let message = MidiMessage::NoteOn(self.channel_out, n.into(), *velocity);
            midi_out.send(message)?;
        }
        Ok(())
    }

    fn stop(&self, midi_out: &midi::OutputDevice, preset: &Preset) -> Result<(), mpsc::SendError<MidiMessage<'static>>> {
        for n in preset.steps.iter() {
            let message = MidiMessage::NoteOff(self.channel_out, n.into(), U7::MIN);
            midi_out.send(message)?;
        }
        Ok(())
    }
}

impl<'a> Arpeggiator for TriggeredChords<'a> {
    fn process(&mut self, received: MidiMessage<'static>, status: &mut dyn StatusSignal) -> Result<(), Box<dyn Error>> {
        match received {
            MidiMessage::NoteOn(_, n, mut v) => {
                self.notes.insert(n);
                let should_play: Vec<Preset> = self.not_playing.iter().filter(|p| p.is_triggered_by(&self.notes)).cloned().collect();
                if let Some(fixed_v) = self.fixed_velocity {
                    v = U7::from_u8_lossy(fixed_v);
                }
                for p in should_play {
                    self.not_playing.remove(&p);
                    self.play(self.midi_out, &p, &v)?;
                    self.playing.insert(p);
                    status.reset_beat(self.channel_out);
                }
            },
            MidiMessage::NoteOff(_, n, _) => {
                self.notes.remove(&n);
                let should_stop: Vec<Preset> = self.playing.iter().filter(|p| !p.is_triggered_by(&self.notes)).cloned().collect();
                for p in should_stop {
                    self.playing.remove(&p);
                    self.stop(self.midi_out, &p)?;
                    self.not_playing.insert(p);
                }
            },
            _ => {}
        };
        status.update_count(self.channel_out, self.count_arpeggios());
        Ok(())
    }

    fn stop_arpeggios(&mut self) -> Result<(), Box<dyn Error>> {
        let drained_items: Vec<_> = self.playing.drain().collect();
        for p in drained_items {
            self.stop(self.midi_out, &p)?;
            self.not_playing.insert(p);
        }
        Ok(())
    }

    fn count_arpeggios(&self) -> usize {
        self.playing.len()
    }
}
