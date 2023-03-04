use wmidi::{Note, MidiMessage, ControlFunction};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::mem;
use std::time::Instant;
use crate::midi;
use crate::arpeggio::{NoteDetails, Step};
use crate::arpeggio::synced::{Arpeggio, Player};
use crate::settings::Settings;
use crate::status::StatusSignal;
use super::Arpeggiator;

pub struct PressHold<'a> {
    midi_out: &'a midi::OutputDevice,
    held_notes: HashMap<Note, (Instant, NoteDetails)>,
    arpeggios: Vec<(HashSet<Note>, Player)>
}

impl<'a> PressHold<'a> {
    const TRIGGER_TIME_MS: u128 = 50;

    pub fn new(midi_out: &'a midi::OutputDevice) -> Self {
        Self {
            midi_out,
            held_notes: HashMap::new(),
            arpeggios: Vec::new()
        }
    }
}

impl<'a> Arpeggiator for PressHold<'a> {
    fn process(&mut self, received: MidiMessage<'static>, settings: &Settings, status: &mut dyn StatusSignal) -> Result<(), Box<dyn Error>> {
        match received {
            MidiMessage::NoteOn(c, n, v) => {
                self.held_notes.insert(n, (Instant::now(), NoteDetails::new(c, n, v, settings.fixed_velocity)));
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
                    let arp = Arpeggio::from(settings.generate_steps(note_details), 1, settings.finish_pattern);
                    self.arpeggios.push((note_set, Player::init(arp, &self.midi_out)));
                    status.reset_beat();
                }
                let mut i = 0;
                while i < self.arpeggios.len() {
                    if !self.arpeggios[i].1.play_tick()? {
                        self.arpeggios.remove(i);
                    } else {
                        i += 1;
                    }
                }
            },
            MidiMessage::Reset => {
                self.held_notes.clear();
                drain_and_force_stop_vec(&mut self.arpeggios)?;
            },
            _ => {}
        }
        Ok(())
    }

    fn stop_arpeggios(&mut self) -> Result<(), Box<dyn Error>> {
        drain_and_force_stop_vec(&mut self.arpeggios)
    }

    fn count_arpeggios(&self) -> usize {
        self.arpeggios.len()
    }
}

pub struct MutatingHold<'a> {
    midi_out: &'a midi::OutputDevice,
    held_notes: Vec<NoteDetails>,
    changed: bool,
    arpeggio: Option<Player>,
}

impl<'a> MutatingHold<'a> {
    pub fn new(midi_out: &'a midi::OutputDevice) -> Self {
        Self {
            midi_out,
            held_notes: Vec::new(),
            changed: false,
            arpeggio: None
        }
    }
}

impl<'a> Arpeggiator for MutatingHold<'a> {
    fn process(&mut self, received: MidiMessage<'static>, settings: &Settings, status: &mut dyn StatusSignal) -> Result<(), Box<dyn Error>> {
        match received {
            MidiMessage::NoteOn(c, n, v) => {
                self.held_notes.push(NoteDetails::new(c, n, v, settings.fixed_velocity));
                self.changed = true;
            },
            MidiMessage::NoteOff(_, n, _) => {
                let mut i = 0;
                while i < self.held_notes.len() {
                    if self.held_notes[i].n == n {
                        self.held_notes.remove(i);
                    } else {
                        i += 1;
                    }
                }
                if self.held_notes.len() == 0 {
                    self.changed = true; // only mutate the arp when notes are added or *all* notes are released, otherwise it mutates down to 1 step during release and the arp doesn't finish its cycle
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
                        let steps: Vec<Step> = self.held_notes.iter().map(|n| Step::note(*n)).collect();
                        let steps_len = steps.len();
                        let arp = Arpeggio::from(steps, steps_len, settings.finish_pattern);
                        if let Some(existing) = &mut self.arpeggio {
                            existing.change_arpeggio(arp)?;
                        } else {
                            self.arpeggio = Some(Player::init(arp, &self.midi_out));
                            status.reset_beat();
                        }
                    }
                }
                if let Some(arp) = &mut self.arpeggio {
                    if !arp.play_tick()? {
                        self.arpeggio = None;
                    }
                }
            },
            MidiMessage::Reset => {
                self.held_notes.clear();
                if let Some(arp) = &mut self.arpeggio {
                    arp.force_stop()?;
                    self.arpeggio = None;
                }
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

pub struct PedalRecorder<'a> {
    midi_out: &'a midi::OutputDevice,
    notes: Vec<(Instant, NoteDetails)>,
    ticks_since_last_note: usize,
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
            ticks_since_last_note: 0,
            pedal: false,
            arpeggios: HashMap::new(),
            recorded: None
        }
    }
}

impl<'a> PedalRecorder<'a> {
    const TRIGGER_TIME_MS: u128 = 50;
}

impl<'a> Arpeggiator for PedalRecorder<'a> {
    fn process(&mut self, received: MidiMessage<'static>, settings: &Settings, status: &mut dyn StatusSignal) -> Result<(), Box<dyn Error>> {
        match received {
            MidiMessage::ControlChange(_, ControlFunction::DAMPER_PEDAL, value) => {
                if !self.pedal && u8::from(value) >= 64 {
                    self.pedal = true;
                    status.reset_beat();
                    self.recorded = None;
                    drain_and_force_stop_map(&mut self.arpeggios)?;
                } else if self.pedal && u8::from(value) < 64 {
                    self.pedal = false;
                    for (_, thru_note) in self.thru_notes.drain() {
                        self.midi_out.sender.send(MidiMessage::NoteOff(thru_note.c, thru_note.n, thru_note.v))?;
                    }
                    if self.notes.len() > 0 {
                        // save recorded arpeggio
                        let notes = mem::replace(&mut self.notes, Vec::new());
                        let mut steps = Vec::new();
                        let mut step_notes = Vec::new();
                        let mut last_instant = None;
                        for (instant, note) in notes {
                            if last_instant.is_some() && instant.duration_since(last_instant.unwrap()).as_millis() > Self::TRIGGER_TIME_MS {
                                steps.push(Step::notes(step_notes));
                                step_notes = Vec::new();
                            }
                            step_notes.push(note);
                            last_instant = Some(instant);
                        }
                        steps.push(Step::notes(step_notes));
                        let total_beats = steps.len();
                        self.recorded = Some(Arpeggio::from(steps, total_beats, settings.finish_pattern));
                        // start play in original key
                        let arp = self.recorded.as_ref().unwrap();
                        let original = arp.first_note();
                        let new_arp = arp.transpose(original, original);
                        self.arpeggios.insert(original, Player::init(new_arp, &self.midi_out));
                        status.reset_beat();
                    }
                }
            },
            MidiMessage::NoteOn(c, n, v) => {
                if self.pedal {
                    self.midi_out.sender.send(received)?;
                    let d = NoteDetails::new(c, n, v, settings.fixed_velocity);
                    self.thru_notes.insert(n, d);
                    self.notes.push((Instant::now(), d));
                    self.ticks_since_last_note = 0;
                } else if self.arpeggios.contains_key(&n) {
                    // already playing, do nothing
                } else if let Some(arp) = &self.recorded {
                    let original = arp.first_note();
                    let new_arp = arp.transpose(original, n);
                    self.arpeggios.insert(n, Player::init(new_arp, &self.midi_out));
                    status.reset_beat();
                }
            },
            MidiMessage::NoteOff(_, n, _) => {
                if self.pedal {
                    self.midi_out.sender.send(received)?;
                    self.thru_notes.remove(&n);
                } else if let Some(player) = self.arpeggios.get_mut(&n) {
                    player.stop();
                }
            },
            MidiMessage::TimingClock => {
                let mut finished = Vec::new();
                for (note, player) in &mut self.arpeggios {
                    if !player.play_tick()? {
                        finished.push(*note);
                    }
                }
                for note in finished {
                    self.arpeggios.remove(&note);
                }
                self.ticks_since_last_note += 1;
            },
            MidiMessage::Reset => {
                self.notes.clear();
                self.thru_notes.clear();
                self.pedal = false;
                drain_and_force_stop_map(&mut self.arpeggios)?;
            },
            _ => {}
        }
        Ok(())
    }

    fn stop_arpeggios(&mut self) -> Result<(), Box<dyn Error>> {
        drain_and_force_stop_map(&mut self.arpeggios)
    }

    fn count_arpeggios(&self) -> usize {
        self.arpeggios.len()
    }
}
