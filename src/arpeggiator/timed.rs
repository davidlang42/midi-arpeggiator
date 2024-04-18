use std::collections::HashMap;
use std::error::Error;
use std::mem;
use std::time::{Duration, Instant};
use wmidi::{ControlFunction, MidiMessage, Note, U7};
use crate::status::StatusSignal;
use crate::midi;
use crate::arpeggio::NoteDetails;
use crate::arpeggio::timed::{Arpeggio, Player};
use crate::settings::Settings;
use super::Arpeggiator;

pub struct RepeatRecorder<'a> {
    midi_out: &'a midi::OutputDevice,
    held_notes: HashMap<Note, (Instant, NoteDetails)>,
    last_note_off: Option<(Instant, NoteDetails)>,
    arpeggios: HashMap<Note, Player>
}

impl<'a> RepeatRecorder<'a> {
    pub fn new(midi_out: &'a midi::OutputDevice) -> Self {
        Self {
            midi_out,
            held_notes: HashMap::new(),
            last_note_off: None,
            arpeggios: HashMap::new()
        }
    }
}

impl<'a> Arpeggiator for RepeatRecorder<'a> {
    fn process(&mut self, received: MidiMessage<'static>, settings: &Settings, status: &mut dyn StatusSignal) -> Result<(), Box<dyn Error>> {
        match received {
            MidiMessage::NoteOn(c, n, v) => {
                match &self.last_note_off {
                    Some((first_i, first)) if first.n == n => {
                        let finish = Instant::now();
                        let mut notes: Vec<(Instant, NoteDetails)> = self.held_notes.drain().map(|(_, v)| v).collect();
                        notes.push((*first_i, *first));
                        notes.sort_by(|(a, _), (b, _)| a.cmp(&b));
                        let arp = Arpeggio::from(notes, finish, settings.finish_pattern);
                        self.arpeggios.insert(n, Player::start(arp, &self.midi_out, &settings.double_notes)?);
                        status.reset_beat();
                    },
                    _ => {
                        self.held_notes.insert(n, (Instant::now(), NoteDetails::new(c, n, v, settings.fixed_velocity)));
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
    notes: Vec<(Instant, NoteDetails)>,
    thru_notes: HashMap<Note, NoteDetails>,
    pedal: bool,
    arpeggios: HashMap<Note, Player>,
    recorded: Option<Arpeggio>
}

impl<'a> PedalRecorder<'a> {
    pub fn new(midi_out: &'a midi::OutputDevice) -> Self {
        Self {
            midi_out,
            notes: Vec::new(),
            thru_notes: HashMap::new(),
            pedal: false,
            arpeggios: HashMap::new(),
            recorded: None
        }
    }
}

impl<'a> Arpeggiator for PedalRecorder<'a> {
    fn process(&mut self, received: MidiMessage<'static>, settings: &Settings, status: &mut dyn StatusSignal) -> Result<(), Box<dyn Error>> {
        match received {
            MidiMessage::ControlChange(_, ControlFunction::DAMPER_PEDAL, value) => {
                if u8::from(value) >= 64 {
                    self.pedal = true;
                    status.reset_beat();
                    self.recorded = None;
                    drain_and_stop(&mut self.arpeggios);
                } else {
                    self.pedal = false;
                    for (_, thru_note) in self.thru_notes.drain() {
                        if self.midi_out.send(MidiMessage::NoteOff(thru_note.c, thru_note.n, thru_note.v)).is_err() {
                            return Err(format!("Unable to send to output queue").into());
                        }
                    }
                    if self.notes.len() > 0 {
                        // save recorded arpeggio
                        let finish = Instant::now();
                        let notes = mem::replace(&mut self.notes, Vec::new());
                        self.recorded = Some(Arpeggio::from(notes, finish, settings.finish_pattern));
                        // start play in original key
                        let arp = self.recorded.as_ref().unwrap();
                        let original = arp.first_note();
                        let new_arp = arp.transpose(original, original);
                        self.arpeggios.insert(original, Player::start(new_arp, &self.midi_out, &settings.double_notes)?);
                        status.reset_beat();
                    }
                }
            },
            MidiMessage::NoteOn(c, n, v) => {
                if self.pedal {
                    if self.midi_out.send(received).is_err() {
                        return Err(format!("Unable to forward to output queue").into());
                    }
                    let d = NoteDetails::new(c, n, v, settings.fixed_velocity);
                    self.thru_notes.insert(n, d);
                    self.notes.push((Instant::now(), d));
                } else if self.arpeggios.contains_key(&n) {
                    // already playing, do nothing
                } else if let Some(arp) = &self.recorded {
                    let original = arp.first_note();
                    let new_arp = arp.transpose(original, n);
                    self.arpeggios.insert(n, Player::start(new_arp, &self.midi_out, &settings.double_notes)?);
                    status.reset_beat();
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

const START_THRESHOLD_TICKS: u8 = 3;

pub struct OneShot<'a> {
    midi_out: &'a midi::OutputDevice,
    starting: Option<(u8, Vec<NoteDetails>)>,
    playing: Option<Player>,
    min_wait: Duration,
    max_wait: Duration,
    expected_wait: Duration,
    last_arp: Instant,
    last_arp_length: usize
}

impl<'a> OneShot<'a> {
    pub fn new(midi_out: &'a midi::OutputDevice) -> Self {
        let expected_wait = 125; // 125ms = 1 semiquaver @ 120bpm
        let min_wait = Duration::from_millis((expected_wait as f64 * 0.8) as u64);
        let max_wait = Duration::from_millis((expected_wait as f64 * 1.2) as u64);
        Self {
            midi_out,
            starting: None,
            playing: None,
            min_wait,
            max_wait,
            expected_wait: Duration::from_millis(expected_wait),
            last_arp: Instant::now(),
            last_arp_length: 1
        }
    }
}

impl<'a> Arpeggiator for OneShot<'a> {
    fn process(&mut self, received: MidiMessage<'static>, settings: &Settings, status: &mut dyn StatusSignal) -> Result<(), Box<dyn Error>> {
        match received {
            MidiMessage::NoteOn(c, n, v) => {
                let nd = NoteDetails::new(c, n, v, settings.fixed_velocity);
                if let Some((_, notes)) = &mut self.starting {
                    notes.push(nd);
                } else {
                    self.starting = Some((START_THRESHOLD_TICKS, vec![nd]));
                }
            },
            MidiMessage::TimingClock => {
                let mut temp = None;
                mem::swap(&mut self.starting, &mut temp);
                if let Some((remaining_ticks, notes)) = temp {
                    if remaining_ticks == 0 {
                        let now = Instant::now();
                        let time_passed = (now - self.last_arp);
                        let mut arp_wait = (now - self.last_arp) / self.last_arp_length as u32;
                        if arp_wait > self.max_wait || arp_wait < self.min_wait {
                            println!("Use default: {}ms (would have been {}ms / {})", self.expected_wait.as_millis(), time_passed.as_millis(), self.last_arp_length);
                            arp_wait = self.expected_wait;
                        } else {
                            println!("Wait: {}ms", arp_wait.as_millis());
                        }
                        
                        self.last_arp_length = notes.len();
                        self.last_arp = now;
                        if let Some(existing) = &mut self.playing {
                            existing.stop();
                        }
                        let arp = Arpeggio::even(notes, arp_wait, settings.pattern, settings.finish_pattern);
                        self.playing = Some(Player::play_once(arp, &self.midi_out, &settings.double_notes)?);
                        //status.reset_beat();
                        self.starting = None;
                    } else {
                        self.starting = Some((remaining_ticks - 1, notes));
                    }
                }
            },
            _ => {}
        }
        Ok(())
    }

    fn stop_arpeggios(&mut self) -> Result<(), Box<dyn Error>> {
        let mut temp = None;
        mem::swap(&mut self.playing, &mut temp);
        if let Some(existing) = temp {
            existing.ensure_stopped()?;
        }
        Ok(())
    }

    fn count_arpeggios(&self) -> usize {
        if self.playing.is_some() {
            1
        } else {
            0
        }
    }
}