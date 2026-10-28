use wmidi::{Channel, MidiMessage, Note, Velocity, U7};
use std::collections::HashSet;
use std::error::Error;
use std::sync::mpsc;
use crate::arpeggio::NoteDetails;
use crate::midi::OutputDevice;
use crate::presets::Preset;
use crate::settings::Settings;
use crate::status::StatusSignal;
use super::Arpeggiator;

pub struct TriggeredChords<'a> {
    midi_out: &'a OutputDevice,
    channel_out: Channel,
    not_playing: HashSet<Preset>,
    notes: HashSet<Note>,
    playing: HashSet<Preset>,
    // from Settings:
    fixed_velocity: Option<u8>,
}

impl<'a> TriggeredChords<'a> {
    pub fn new(midi_out: &'a OutputDevice, channel_out: Channel, settings: &Settings) -> Self {
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
    fn play(&self, midi_out: &OutputDevice, preset: &Preset, velocity: &Velocity) -> Result<(), mpsc::SendError<MidiMessage<'static>>> {
        for n in preset.steps.iter() {
            let message = MidiMessage::NoteOn(self.channel_out, n.into(), *velocity);
            midi_out.send(message)?;
        }
        Ok(())
    }

    fn stop(&self, midi_out: &OutputDevice, preset: &Preset) -> Result<(), mpsc::SendError<MidiMessage<'static>>> {
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

pub struct PatternOnPress<'a> {
    midi_out: &'a OutputDevice,
    channel_out: Channel,
    current_note: Option<Note>,
    offset_index: usize,
    active_offset_note: Option<NoteDetails>,
    offsets: Vec<i8>,
    fixed_velocity: Option<u8>,
}

impl<'a> PatternOnPress<'a> {
    pub fn new(midi_out: &'a OutputDevice, channel_out: Channel, settings: &Settings) -> Self {
        // not helpful if midi_notes is missing or empty, but better than crashing
        let mut offsets = settings.double_notes.clone().unwrap_or_else(|| Vec::new());
        if offsets.len() == 0 {
            // can't be empty
            offsets.push(0);
        }
        Self {
            midi_out,
            channel_out,
            current_note: None,
            offset_index: 0,
            active_offset_note: None,
            offsets,
            fixed_velocity: settings.fixed_velocity,
        }
    }

    fn stop_active(&mut self) -> Result<(), Box<dyn Error>> {
        if let Some(nd) = self.active_offset_note.take() {
            self.midi_out.send(MidiMessage::NoteOff(self.channel_out, nd.n, nd.v))?;
        }
        Ok(())
    }
}

impl<'a> Arpeggiator for PatternOnPress<'a> {
    fn process(&mut self, received: MidiMessage<'static>, status: &mut dyn StatusSignal) -> Result<(), Box<dyn Error>> {
        match received {
            MidiMessage::NoteOn(_, n, v) => {
                let velocity = if let Some(fv) = self.fixed_velocity {
                    U7::from_u8_lossy(fv)
                } else {
                    v
                };

                if self.current_note != Some(n) {
                    self.current_note = Some(n);
                    status.update_count(self.channel_out, self.count_arpeggios());
                    self.offset_index = 0;
                    status.reset_beat(self.channel_out);
                }

                self.stop_active()?;
                let offset = self.offsets[self.offset_index];
                let target_note = n.step(offset).unwrap_or(n);

                self.midi_out.send(MidiMessage::NoteOn(self.channel_out, target_note, velocity))?;
                self.active_offset_note = Some(NoteDetails {
                    n: target_note,
                    v: velocity
                });
                self.offset_index += 1;
                if self.offset_index == self.offsets.len() {
                    self.offset_index = 0;
                    status.reset_beat(self.channel_out);
                }
            }
            MidiMessage::NoteOff(_, _, _) => {
                self.stop_active()?;
            }
            MidiMessage::Reset => {
                self.stop_arpeggios()?;
                status.update_count(self.channel_out, self.count_arpeggios());
            }
            _ => {}
        }
        Ok(())
    }

    fn stop_arpeggios(&mut self) -> Result<(), Box<dyn Error>> {
        self.stop_active()?;
        self.current_note = None;
        self.offset_index = 0;
        Ok(())
    }

    fn count_arpeggios(&self) -> usize {
        if self.current_note.is_some() { 1 } else { 0 }
    }
}
