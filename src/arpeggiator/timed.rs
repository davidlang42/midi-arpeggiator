use std::collections::HashMap;
use std::error::Error;
use std::mem;
use std::time::Instant;
use wmidi::{Channel, Note, MidiMessage, ControlFunction};
use crate::status::StatusSignal;
use crate::midi;
use crate::arpeggio::NoteDetails;
use crate::arpeggio::timed::{Arpeggio, Player};
use crate::settings::Settings;
use super::Arpeggiator;

pub struct RepeatRecorder<'a> {
    midi_out: &'a midi::OutputDevice,
    channel_out: Channel,
    held_notes: HashMap<Note, (Instant, NoteDetails)>,
    last_note_off: Option<(Instant, NoteDetails)>,
    arpeggios: HashMap<Note, Player>,
    // from Settings:
    finish_pattern: bool,
    fixed_velocity: Option<u8>,
    double_notes: Option<Vec<i8>>,
}

impl<'a> RepeatRecorder<'a> {
    pub fn new(midi_out: &'a midi::OutputDevice, channel_out: Channel, settings: &Settings) -> Self {
        Self {
            midi_out,
            channel_out,
            held_notes: HashMap::new(),
            last_note_off: None,
            arpeggios: HashMap::new(),
            finish_pattern: settings.finish_pattern,
            fixed_velocity: settings.fixed_velocity,
            double_notes: settings.double_notes.clone(),
        }
    }
}

impl<'a> Arpeggiator for RepeatRecorder<'a> {
    fn process(&mut self, received: MidiMessage<'static>, status: &mut dyn StatusSignal) -> Result<(), Box<dyn Error>> {
        match received {
            MidiMessage::NoteOn(_, n, v) => {
                match &self.last_note_off {
                    Some((first_i, first)) if first.n == n => {
                        let finish = Instant::now();
                        let mut notes: Vec<(Instant, NoteDetails)> = self.held_notes.drain().map(|(_, v)| v).collect();
                        notes.push((*first_i, *first));
                        notes.sort_by(|(a, _), (b, _)| a.cmp(&b));
                        let arp = Arpeggio::from(notes, finish, self.finish_pattern);
                        self.arpeggios.insert(n, Player::start(arp, &self.midi_out, self.channel_out, &self.double_notes)?);
                        status.reset_beat(self.channel_out);
                    },
                    _ => {
                        self.held_notes.insert(n, (Instant::now(), NoteDetails::new(n, v, self.fixed_velocity)));
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
                drain_and_wait_for_stop(&mut self.arpeggios)?;
            },
            _ => {}
        }
        status.update_count(self.channel_out, self.count_arpeggios());
        Ok(())
    }

    fn stop_arpeggios(&mut self) -> Result<(), Box<dyn Error>> {
        drain_and_wait_for_stop(&mut self.arpeggios)
    }

    fn count_arpeggios(&self) -> usize {
        self.arpeggios.len()
    }
}

pub struct PedalRecorder<'a> {
    midi_out: &'a midi::OutputDevice,
    channel_out: Channel,
    notes: Vec<(Instant, NoteDetails)>,
    thru_notes: HashMap<Note, NoteDetails>,
    pedal: bool,
    arpeggios: HashMap<Note, Player>,
    recorded: Option<Arpeggio>,
    // from Settings:
    finish_pattern: bool,
    fixed_velocity: Option<u8>,
    double_notes: Option<Vec<i8>>,
}

impl<'a> PedalRecorder<'a> {
    pub fn new(midi_out: &'a midi::OutputDevice, channel_out: Channel, settings: &Settings) -> Self {
        Self {
            midi_out,
            channel_out,
            notes: Vec::new(),
            thru_notes: HashMap::new(),
            pedal: false,
            arpeggios: HashMap::new(),
            recorded: None,
            finish_pattern: settings.finish_pattern,
            fixed_velocity: settings.fixed_velocity,
            double_notes: settings.double_notes.clone(),
        }
    }
}

impl<'a> Arpeggiator for PedalRecorder<'a> {
    fn process(&mut self, received: MidiMessage<'static>, status: &mut dyn StatusSignal) -> Result<(), Box<dyn Error>> {
        match received {
            MidiMessage::ControlChange(_, ControlFunction::DAMPER_PEDAL, value) => {
                if u8::from(value) >= 64 {
                    self.pedal = true;
                    status.reset_beat(self.channel_out);
                    self.recorded = None;
                    drain_and_stop(&mut self.arpeggios);
                } else {
                    self.pedal = false;
                    for (_, thru_note) in self.thru_notes.drain() {
                        if self.midi_out.send(MidiMessage::NoteOff(self.channel_out, thru_note.n, thru_note.v)).is_err() {
                            return Err(format!("Unable to send to output queue").into());
                        }
                    }
                    if self.notes.len() > 0 {
                        // save recorded arpeggio
                        let finish = Instant::now();
                        let notes = mem::replace(&mut self.notes, Vec::new());
                        self.recorded = Some(Arpeggio::from(notes, finish, self.finish_pattern));
                        // start play in original key
                        let arp = self.recorded.as_ref().unwrap();
                        let original = arp.first_note();
                        let new_arp = arp.transpose(original, original);
                        self.arpeggios.insert(original, Player::start(new_arp, &self.midi_out, self.channel_out, &self.double_notes)?);
                        status.reset_beat(self.channel_out);
                    }
                }
            },
            MidiMessage::NoteOn(_, n, v) => {
                if self.pedal {
                    if self.midi_out.send(received).is_err() {
                        return Err(format!("Unable to forward to output queue").into());
                    }
                    let d = NoteDetails::new(n, v, self.fixed_velocity);
                    self.thru_notes.insert(n, d);
                    self.notes.push((Instant::now(), d));
                } else if self.arpeggios.contains_key(&n) {
                    // already playing, do nothing
                } else if let Some(arp) = &self.recorded {
                    let original = arp.first_note();
                    let new_arp = arp.transpose(original, n);
                    self.arpeggios.insert(n, Player::start(new_arp, &self.midi_out, self.channel_out, &self.double_notes)?);
                    status.reset_beat(self.channel_out);
                }
            },
            MidiMessage::NoteOff(_, n, _) => {
                if self.pedal {
                    if self.midi_out.send(received).is_err() {
                        return Err(format!("Unable to forward to output queue").into());
                    }
                    self.thru_notes.remove(&n);
                } else if let Some(mut player) = self.arpeggios.remove(&n) {
                    player.stop();
                }
            },
            MidiMessage::Reset => {
                self.notes.clear();
                self.pedal = false;
                drain_and_wait_for_stop(&mut self.arpeggios)?;
            },
            _ => {}
        }
        status.update_count(self.channel_out, self.count_arpeggios());
        Ok(())
    }

    fn stop_arpeggios(&mut self) -> Result<(), Box<dyn Error>> {
        drain_and_wait_for_stop(&mut self.arpeggios)
    }

    fn count_arpeggios(&self) -> usize {
        self.arpeggios.len()
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

fn drain_and_wait_for_stop<N>(arpeggios: &mut HashMap<N, Player>) -> Result<(), Box<dyn Error>> {
    for player in drain_and_stop(arpeggios) {
        player.ensure_stopped()?;
    }
    Ok(())
}
