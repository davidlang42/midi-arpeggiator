use wmidi::{Channel, ControlFunction, MidiMessage, Note, U7};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::mem;
use std::time::Instant;

use super::{Arpeggiator, Pattern};
use crate::midi::OutputDevice;
use crate::arpeggio::{NoteDetails, Step};
use crate::arpeggio::synced::{Arpeggio, Player};
use crate::presets::Preset;
use crate::notename::NoteName;
use crate::settings::{Settings, MidiNoteEvent};
use crate::status::StatusSignal;

pub struct PressHold<'a> {
    midi_out: &'a OutputDevice,
    channel_out: Channel,
    held_notes: HashMap<Note, (Instant, NoteDetails)>,
    pedal_notes_off: HashSet<Note>,
    pedal: bool,
    arpeggios: Vec<(HashSet<Note>, Player)>,
    // from Settings:
    finish_pattern: bool,
    fixed_velocity: Option<u8>,
    fixed_steps: Option<usize>, // assumed in 1 beat
    fixed_notes_per_step: Option<usize>,
    pattern: Pattern,
    double_notes: Option<Vec<i8>>,
    max_repeats: Option<usize>,
}

impl<'a> PressHold<'a> {
    const TRIGGER_TIME_MS: u128 = 50;

    pub fn new(midi_out: &'a OutputDevice, channel_out: Channel, settings: &Settings) -> Self {
        Self {
            midi_out,
            channel_out,
            held_notes: HashMap::new(),
            pedal: false,
            pedal_notes_off: HashSet::new(),
            arpeggios: Vec::new(),
            finish_pattern: settings.finish_pattern,
            fixed_velocity: settings.fixed_velocity,
            fixed_steps: settings.fixed_steps,
            fixed_notes_per_step: settings.fixed_notes_per_step,
            pattern: settings.pattern,
            double_notes: settings.double_notes.clone(),
            max_repeats: settings.max_repeats,
        }
    }

    fn release_note(&mut self, n: Note) {
        self.held_notes.remove(&n);
        for (note_set, player) in self.arpeggios.iter_mut() {
            if note_set.remove(&n) && note_set.len() == 0 {
                player.stop();
            }
        }
    }
}

impl<'a> Arpeggiator for PressHold<'a> {
    fn process(&mut self, received: MidiMessage<'static>, status: &mut dyn StatusSignal) -> Result<(), Box<dyn Error>> {
        match received {
            MidiMessage::ControlChange(_, ControlFunction::DAMPER_PEDAL, value) => {
                let new_pedal = u8::from(value) >= 64;
                if self.pedal != new_pedal {
                    self.pedal = new_pedal;
                    if !self.pedal {
                        // pedal released
                        let notes_to_release: Vec<Note> = self.pedal_notes_off.drain().collect();
                        for n in notes_to_release {
                            self.release_note(n);
                        }
                    }
                }
            },
            MidiMessage::NoteOn(_, n, v) => {
                if self.pedal_notes_off.remove(&n) { // this implies self.pedal
                    // we are re-pressing a note which isn't actually off yet, because we're holding the pedal
                    // so we just removed it from what will be released when the pedal is released
                } else {
                    self.held_notes.insert(n, (Instant::now(), NoteDetails::new(n, v, self.fixed_velocity)));
                }
            },
            MidiMessage::NoteOff(_, n, _) => {
                if self.pedal {
                    // if the pedal is down, we don't actually release the note, just add it to a list
                    // when the pedal is released, all the notes in the list get "released"
                    self.pedal_notes_off.insert(n);
                } else {
                    self.release_note(n);
                }
            },
            MidiMessage::TimingClock => {
                if self.held_notes.len() != 0 && self.held_notes.values().map(|(i, _)| i).min().unwrap().elapsed().as_millis() > Self::TRIGGER_TIME_MS {
                    let note_details: Vec<NoteDetails> = self.held_notes.drain().map(|(_, (_, d))| d).collect();
                    let note_set: HashSet<Note> = note_details.iter().map(|d| d.n).collect();
                    let steps = Settings::generate_steps(self.fixed_steps, self.pattern, self.fixed_notes_per_step, note_details);
                    let arp = Arpeggio::from(steps, 1, self.finish_pattern);
                    self.arpeggios.push((note_set, Player::init(arp, &self.midi_out, self.channel_out, &self.double_notes, self.max_repeats)));
                    status.reset_beat(self.channel_out);
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
        status.update_count(self.channel_out, self.count_arpeggios());
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
    midi_out: &'a OutputDevice,
    channel_out: Channel,
    held_notes: Vec<NoteDetails>,
    changed: bool,
    arpeggio: Option<Player>,
    pedal: bool,
    pedal_notes_off: HashSet<Note>,
    // from Settings:
    finish_pattern: bool,
    fixed_velocity: Option<u8>,
    double_notes: Option<Vec<i8>>,
    max_repeats: Option<usize>,
}

impl<'a> MutatingHold<'a> {
    pub fn new(midi_out: &'a OutputDevice, channel_out: Channel, settings: &Settings) -> Self {
        Self {
            midi_out,
            channel_out,
            held_notes: Vec::new(),
            changed: false,
            arpeggio: None,
            pedal: false,
            pedal_notes_off: HashSet::new(),
            finish_pattern: settings.finish_pattern,
            fixed_velocity: settings.fixed_velocity,
            double_notes: settings.double_notes.clone(),
            max_repeats: settings.max_repeats,
        }
    }

    fn release_note(&mut self, n: Note) {
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
    }
}

impl<'a> Arpeggiator for MutatingHold<'a> {
    fn process(&mut self, received: MidiMessage<'static>, status: &mut dyn StatusSignal) -> Result<(), Box<dyn Error>> {
        match received {
            MidiMessage::ControlChange(_, ControlFunction::DAMPER_PEDAL, value) => {
                let new_pedal = u8::from(value) >= 64;
                if self.pedal != new_pedal {
                    self.pedal = new_pedal;
                    if !self.pedal {
                        // pedal released
                        let notes_to_release: Vec<Note> = self.pedal_notes_off.drain().collect();
                        for n in notes_to_release {
                            self.release_note(n);
                        }
                    }
                }
            },
            MidiMessage::NoteOn(_, n, v) => {
                if self.pedal_notes_off.remove(&n) { // this implies self.pedal
                    // we are re-pressing a note which isn't actually off yet, because we're holding the pedal
                    // so we just removed it from what will be released when the pedal is released
                } else {
                    self.held_notes.push(NoteDetails::new(n, v, self.fixed_velocity));
                    self.changed = true;
                }
            },
            MidiMessage::NoteOff(_, n, _) => {
                if self.pedal {
                    // if the pedal is down, we don't actually release the note, just add it to a list
                    // when the pedal is released, all the notes in the list get "released"
                    self.pedal_notes_off.insert(n);
                } else {
                    self.release_note(n);
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
                        let arp = Arpeggio::from(steps, steps_len, self.finish_pattern);
                        if let Some(existing) = &mut self.arpeggio {
                            existing.change_arpeggio(arp)?;
                        } else {
                            self.arpeggio = Some(Player::init(arp, &self.midi_out, self.channel_out, &self.double_notes, self.max_repeats));
                            status.reset_beat(self.channel_out);
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
        status.update_count(self.channel_out, self.count_arpeggios());
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
    midi_out: &'a OutputDevice,
    channel_out: Channel,
    notes: Vec<(Instant, NoteDetails)>,
    ticks_since_last_note: usize,
    thru_notes: HashMap<Note, NoteDetails>,
    pedal: bool,
    arpeggios: HashMap<Note, Player>,
    recorded: Option<Arpeggio>,
    // from Settings:
    finish_pattern: bool,
    fixed_velocity: Option<u8>,
    double_notes: Option<Vec<i8>>,
    max_repeats: Option<usize>,
}

impl<'a> PedalRecorder<'a> {
    pub fn new(midi_out: &'a OutputDevice, channel_out: Channel, settings: &Settings) -> Self {
        Self {
            midi_out,
            channel_out,
            notes: Vec::new(),
            thru_notes: HashMap::new(),
            ticks_since_last_note: 0,
            pedal: false,
            arpeggios: HashMap::new(),
            recorded: None,
            finish_pattern: settings.finish_pattern,
            fixed_velocity: settings.fixed_velocity,
            double_notes: settings.double_notes.clone(),
            max_repeats: settings.max_repeats,
        }
    }
}

impl<'a> PedalRecorder<'a> {
    const TRIGGER_TIME_MS: u128 = 50;
}

impl<'a> Arpeggiator for PedalRecorder<'a> {
    fn process(&mut self, received: MidiMessage<'static>, status: &mut dyn StatusSignal) -> Result<(), Box<dyn Error>> {
        match received {
            MidiMessage::ControlChange(_, ControlFunction::DAMPER_PEDAL, value) => {
                if !self.pedal && u8::from(value) >= 64 {
                    self.pedal = true;
                    status.reset_beat(self.channel_out);
                    self.recorded = None;
                    drain_and_force_stop_map(&mut self.arpeggios)?;
                } else if self.pedal && u8::from(value) < 64 {
                    self.pedal = false;
                    for (_, thru_note) in self.thru_notes.drain() {
                        self.midi_out.send(MidiMessage::NoteOff(self.channel_out, thru_note.n, thru_note.v))?;
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
                        self.recorded = Some(Arpeggio::from(steps, total_beats, self.finish_pattern));
                        // start play in original key
                        let arp = self.recorded.as_ref().unwrap();
                        let original = arp.first_note();
                        let new_arp = arp.transpose(original, original);
                        self.arpeggios.insert(original, Player::init(new_arp, &self.midi_out, self.channel_out, &self.double_notes, self.max_repeats));
                        status.reset_beat(self.channel_out);
                    }
                }
            },
            MidiMessage::NoteOn(_, n, v) => {
                if self.pedal {
                    self.midi_out.send(received)?;
                    let d = NoteDetails::new(n, v, self.fixed_velocity);
                    self.thru_notes.insert(n, d);
                    self.notes.push((Instant::now(), d));
                    self.ticks_since_last_note = 0;
                } else if self.arpeggios.contains_key(&n) {
                    // already playing, do nothing
                } else if let Some(arp) = &self.recorded {
                    let original = arp.first_note();
                    let new_arp = arp.transpose(original, n);
                    self.arpeggios.insert(n, Player::init(new_arp, &self.midi_out, self.channel_out, &self.double_notes, self.max_repeats));
                    status.reset_beat(self.channel_out);
                }
            },
            MidiMessage::NoteOff(_, n, _) => {
                if self.pedal {
                    self.midi_out.send(received)?;
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
        status.update_count(self.channel_out, self.count_arpeggios());
        Ok(())
    }

    fn stop_arpeggios(&mut self) -> Result<(), Box<dyn Error>> {
        drain_and_force_stop_map(&mut self.arpeggios)
    }

    fn count_arpeggios(&self) -> usize {
        self.arpeggios.len()
    }
}

pub struct PrerecordedSets<'a> {
    midi_out: &'a OutputDevice,
    channel_out: Channel,
    presets: Vec<Preset>,
    notes: HashSet<Note>,
    changed: bool,
    playing: Option<Player>,
    pedal: bool,
    // from Settings:
    finish_pattern: bool,
    fixed_velocity: Option<u8>,
    fixed_notes_per_step: Option<usize>,
    double_notes: Option<Vec<i8>>,
    max_repeats: Option<usize>,
    use_pedal: Option<bool>
}

impl<'a> PrerecordedSets<'a> {
    pub fn new(midi_out: &'a OutputDevice, channel_out: Channel, settings: &Settings) -> Self {
        Self {
            midi_out,
            channel_out,
            notes: HashSet::new(),
            changed: false,
            playing: None,
            pedal: false,
            finish_pattern: settings.finish_pattern,
            fixed_velocity: settings.fixed_velocity,
            fixed_notes_per_step: settings.fixed_notes_per_step,
            double_notes: settings.double_notes.clone(),
            presets: settings.presets.clone().unwrap_or_else(|| Vec::new()), // not helpful if presets is missing, but better than crashing
            max_repeats: settings.max_repeats,
            use_pedal: settings.use_pedal
        }
    }
}

impl<'a> PrerecordedSets<'a> {
    fn find_preset(&self, notes: &HashSet<Note>) -> Option<usize> {
        for i in 0..self.presets.len() {
            if self.presets[i].is_triggered_by(&notes) {
                return Some(i);
            }
        }
        None
    }
}

impl<'a> Arpeggiator for PrerecordedSets<'a> {
    fn process(&mut self, received: MidiMessage<'static>, status: &mut dyn StatusSignal) -> Result<(), Box<dyn Error>> {
        match received {
            MidiMessage::ControlChange(_, ControlFunction::DAMPER_PEDAL, value) => {
                self.pedal = u8::from(value) >= 64;
            },
            MidiMessage::NoteOn(_, n, _) => {
                self.notes.insert(n);
                self.changed = true;
            },
            MidiMessage::NoteOff(_, n, _) => {
                self.notes.remove(&n);
                self.changed = true;
            },
            MidiMessage::TimingClock => {
                let dont_change_because_pedal = self.use_pedal == Some(true) && self.pedal;
                if self.changed && !dont_change_because_pedal {
                    self.changed = false;
                    if let Some(p) = self.find_preset(&self.notes) {
                        if let Some(existing) = &mut self.playing {
                            existing.force_stop()?;
                        }
                        let new_arp = Arpeggio::from_preset(&self.presets[p], U7::from_u8_lossy(self.fixed_velocity.unwrap_or(100)), self.finish_pattern, self.fixed_notes_per_step.unwrap_or(1));
                        self.playing = Some(Player::init(new_arp, self.midi_out, self.channel_out, &self.double_notes, self.max_repeats));
                        status.reset_beat(self.channel_out);
                    } else {
                        if let Some(existing) = &mut self.playing {
                            existing.stop();
                        }
                    }
                }
                if let Some(player) = &mut self.playing {
                    if !player.play_tick()? {
                        self.playing = None;
                    }
                }
            },
            _ => {}
        }
        status.update_count(self.channel_out, self.count_arpeggios());
        Ok(())
    }

    fn stop_arpeggios(&mut self) -> Result<(), Box<dyn Error>> {
        if let Some(player) = &mut self.playing {
            player.stop();
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

pub struct MidiLoop<'a> {
    midi_out: &'a OutputDevice,
    channel_out: Channel,
    playing: bool,
    internal_tick_counter: u64,
    event_index: usize,
    active_notes: HashSet<Note>,
    midi_events: Vec<MidiNoteEvent>,
    max_tick: u64,
    start_key: Option<NoteName>,
    stop_key: Option<NoteName>,
    fixed_velocity: Option<u8>,
}

impl<'a> MidiLoop<'a> {
    pub fn new(midi_out: &'a OutputDevice, channel_out: Channel, settings: &Settings) -> Self {
        let midi_events = settings.midi_notes.clone().unwrap_or_else(|| Vec::new()); // not helpful if midi_notes is missing, but better than crashing
        let max_tick = midi_events.last().map_or(0, |e| e.tick);
        Self {
            midi_out,
            channel_out,
            playing: false,
            internal_tick_counter: 0,
            event_index: 0,
            active_notes: HashSet::new(),
            midi_events,
            max_tick,
            start_key: settings.start_key,
            stop_key: settings.stop_key,
            fixed_velocity: settings.fixed_velocity,
        }
    }

    fn turn_off_all_active_notes(&mut self) -> Result<(), Box<dyn Error>> {
        for note in &self.active_notes {
            self.midi_out.send(MidiMessage::NoteOff(self.channel_out, *note, U7::MIN))?;
        }
        self.active_notes.clear();
        Ok(())
    }
}

impl<'a> Arpeggiator for MidiLoop<'a> {
    fn process(&mut self, received: MidiMessage<'static>, status: &mut dyn StatusSignal) -> Result<(), Box<dyn Error>> {
        match received {
            MidiMessage::NoteOn(_, n, _) => {
                let matches_start = if let Some(start_note) = &self.start_key {
                    <&NoteName as Into<Note>>::into(start_note) == n
                } else {
                    true
                };
                let matches_stop = if let Some(stop_note) = &self.stop_key {
                    <&NoteName as Into<Note>>::into(stop_note) == n
                } else {
                    true
                };
                if self.playing && matches_stop {
                    self.playing = false;
                    self.turn_off_all_active_notes()?;
                    status.update_count(self.channel_out, self.count_arpeggios());
                } else if !self.playing && matches_start {
                    self.playing = true;
                    status.update_count(self.channel_out, self.count_arpeggios());
                    status.reset_beat(self.channel_out);
                }
            }
            MidiMessage::TimingClock => {
                if self.playing {
                    while self.event_index < self.midi_events.len() && self.midi_events[self.event_index].tick == self.internal_tick_counter {
                        let ev = &self.midi_events[self.event_index];
                        if ev.is_on {
                            let vel = U7::from_u8_lossy(self.fixed_velocity.unwrap_or(ev.velocity.into()));
                            self.midi_out.send(MidiMessage::NoteOn(self.channel_out, ev.note, vel))?;
                            self.active_notes.insert(ev.note);
                        } else {
                            self.midi_out.send(MidiMessage::NoteOff(self.channel_out, ev.note, U7::MIN))?;
                            self.active_notes.remove(&ev.note);
                        }
                        self.event_index += 1;
                    }

                    if self.internal_tick_counter == self.max_tick {
                        self.internal_tick_counter = 0;
                        self.event_index = 0;
                    } else {
                        self.internal_tick_counter += 1;
                    }
                }
            }
            MidiMessage::Reset => {
                self.internal_tick_counter = 0;
                self.event_index = 0;
                self.stop_arpeggios()?;
                status.update_count(self.channel_out, self.count_arpeggios());
            }
            _ => {}
        }
        Ok(())
    }

    fn stop_arpeggios(&mut self) -> Result<(), Box<dyn Error>> {
        self.playing = false;
        self.turn_off_all_active_notes()
    }

    fn count_arpeggios(&self) -> usize {
        if self.playing { 1 } else { 0 }
    }
}