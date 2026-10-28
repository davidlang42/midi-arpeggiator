use std::array;
use std::error::Error;
use wmidi::{Channel, ControlFunction, MidiMessage, U7};

use strum_macros::EnumIter;

use crate::arpeggio::{NoteDetails, Step};
use crate::status::StatusSignal;
use crate::midi::{self, InputDevice, OutputDevice};
use crate::settings::{Settings, SettingsChanger};

pub mod timed;
pub mod synced;
pub mod triggered;
pub mod full_length;

#[derive(Copy, Clone, EnumIter, Debug, Serialize, Deserialize, PartialEq)]
pub enum Pattern {
    Up,
    Down
}

impl Pattern {
    pub fn of(&self, mut notes: Vec<NoteDetails>, steps: usize) -> Vec<Step> {
        if steps == 0 {
            panic!("Cannot generate Pattern in 0 steps");
        }
        // put the notes in order based on the pattern type
        match self {
            Pattern::Up => notes.sort_by(|a, b| a.n.cmp(&b.n)),
            Pattern::Down => notes.sort_by(|a, b| b.n.cmp(&a.n)),
        }
        // expand notes until there are at least enough notes for 1 note per step
        while notes.len() < steps {
            Self::expand(&mut notes);
        }
        // calculate how many notes in each step (prioritising earlier steps)
        let minimum_notes_per_step = notes.len() / steps;
        let mut notes_per_step = [minimum_notes_per_step].repeat(steps);
        let mut notes_remaining = notes.len() % steps;
        for i in 0..steps {
            if notes_remaining == 0 {
                break;
            } else {
                notes_per_step[i] += 1;
                notes_remaining -= 1;
            }
        }
        // generate steps
        let mut steps = Vec::new();
        let mut iter = notes.into_iter();
        for notes_in_this_step in notes_per_step {
            steps.push(Step::notes((&mut iter).take(notes_in_this_step).collect()));
        }
        steps
    }

    fn expand(notes: &mut Vec<NoteDetails>) {
        // create extra notes by repeating the existing notes in reverse
        let range = match notes.len() {
            0 => panic!("Cannot generate Pattern of 0 notes"),
            1 => 0..1, // if there is only 1 note, repeat it
            2 => 0..2, // if there are only 2 notes, repeat them both
            _ => 1..(notes.len() - 1) // otherwise repeat all except first and last notes
        };
        for i in range.rev() {
            notes.push(notes[i].clone())
        }
    }
}

pub trait Arpeggiator {
    fn process(&mut self, message: MidiMessage<'static>, signal: &mut dyn StatusSignal) -> Result<(), Box<dyn Error>>;
    fn stop_arpeggios(&mut self) -> Result<(), Box<dyn Error>>;
    fn count_arpeggios(&self) -> usize;
}

#[derive(PartialEq, EnumIter, Copy, Clone, Debug, Serialize, Deserialize)]
pub enum ArpeggiatorMode {
    Passthrough,
    RepeatRecorder,
    TimedPedalRecorder,
    PressHold,
    MutatingHold,
    SyncedPedalRecorder,
    EvenMutator,
    PrerecordedSets,
    TriggeredChords,
    MidiLoop,
    PatternOnPress,
}

fn create_arpeggiator<'a>(settings: &Settings, midi_out: &'a OutputDevice, channel_out: Channel, output_device_is_input_device: bool) -> Box<dyn Arpeggiator + 'a> {
    match settings.mode {
        ArpeggiatorMode::Passthrough => Box::new(Passthrough::new(midi_out, output_device_is_input_device, settings)),
        ArpeggiatorMode::MutatingHold => Box::new(synced::MutatingHold::new(midi_out, channel_out, settings)),
        ArpeggiatorMode::PressHold => Box::new(synced::PressHold::new(midi_out, channel_out, settings)),
        ArpeggiatorMode::TimedPedalRecorder => Box::new(timed::PedalRecorder::new(midi_out, channel_out, settings)),
        ArpeggiatorMode::RepeatRecorder => Box::new(timed::RepeatRecorder::new(midi_out, channel_out, settings)),
        ArpeggiatorMode::SyncedPedalRecorder => Box::new(synced::PedalRecorder::new(midi_out, channel_out, settings)),
        ArpeggiatorMode::EvenMutator => Box::new(full_length::EvenMutator::new(midi_out, channel_out, settings)),
        ArpeggiatorMode::PrerecordedSets => Box::new(synced::PrerecordedSets::new(midi_out, channel_out, settings)),
        ArpeggiatorMode::TriggeredChords => Box::new(triggered::TriggeredChords::new(midi_out, channel_out, settings)),
        ArpeggiatorMode::MidiLoop => Box::new(synced::MidiLoop::new(midi_out, channel_out, settings)),
        ArpeggiatorMode::PatternOnPress => Box::new(triggered::PatternOnPress::new(midi_out, channel_out, settings)),
    }
}

fn get_channel(message: &MidiMessage) -> Option<Channel> {
    match message {
        MidiMessage::NoteOff(c, _, _) => Some(*c),
        MidiMessage::NoteOn(c, _, _) => Some(*c),
        MidiMessage::PolyphonicKeyPressure(c, _, _) => Some(*c),
        MidiMessage::ControlChange(c, _, _) => Some(*c),
        MidiMessage::ProgramChange(c, _) => Some(*c),
        MidiMessage::ChannelPressure(c, _) => Some(*c),
        MidiMessage::PitchBendChange(c, _) => Some(*c),
        _ => None,
    }
}

pub struct MultiArpeggiator<'a, SC: SettingsChanger, SS: StatusSignal> {
    midi_in: InputDevice,
    midi_out: OutputDevice,
    output_device_is_input_device: bool,
    settings_changer: SC,
    status: &'a mut SS,
}

impl<'a, SC: SettingsChanger, SS: StatusSignal> MultiArpeggiator<'a, SC, SS> {
    pub fn new(midi_in: &str, midi_out: &str, settings_changer: SC, status: &'a mut SS) -> Result<Self, Box<dyn Error>> {
        let output_device_is_input_device = midi_in == midi_out;
        Ok(Self {
            midi_in: if output_device_is_input_device {
                InputDevice::open(&midi_in, true)?
            } else {
                InputDevice::open_with_external_clock(&midi_in, &midi_out, true)?
            },
            midi_out: OutputDevice::open(&midi_out)?,
            output_device_is_input_device,
            settings_changer,
            status,
        })
    }

    pub fn listen(mut self) -> Result<(), Box<dyn Error>> {
        let mut changers: [SC; midi::CHANNELS] = array::from_fn(|_| self.settings_changer.clone());
        let default_settings = self.settings_changer.get_default();
        let mut arpeggiators: [Box<dyn Arpeggiator>; midi::CHANNELS] = array::from_fn(|ch| create_arpeggiator(default_settings, &self.midi_out, Channel::from_index(ch.try_into().unwrap()).unwrap(), self.output_device_is_input_device));
        
        loop {
            // read next midi message
            let message = self.midi_in.read()?;
            if let Some(channel) = get_channel(&message) {
                // process channel specific messages through settings -> status -> arpeggiator
                let index = channel.index() as usize;
                // check for settings changes
                let mut m = changers[index].passthrough_midi(message);
                if let Some(new_settings) = changers[index].get_change() {
                    arpeggiators[index].stop_arpeggios()?;
                    self.status.update_settings(channel, new_settings);
                    arpeggiators[index] = create_arpeggiator(new_settings, &self.midi_out, channel, self.output_device_is_input_device);
                }
                // pass message through status
                if m.is_none() { continue; }
                m = self.status.passthrough_midi(m.unwrap());
                // send message to arpeggiator
                if m.is_none() { continue; }
                arpeggiators[index].process(m.unwrap(), self.status)?;
            } else {
                // broadcast system/global messages (like TimingClock, Reset, etc.) only to arpeggiators
                for arpeggiator in &mut arpeggiators {
                    arpeggiator.process(message.clone(), self.status)?;
                }
            }
        }
    }
}

struct Passthrough<'a> {
    output: &'a OutputDevice,
    output_device_is_input_device: bool,
    fixed_velocity: Option<u8>,
    double_notes: Option<Vec<i8>>,
}

impl<'a> Passthrough<'a> {
    fn new(output: &'a OutputDevice, output_device_is_input_device: bool, settings: &Settings) -> Self {
        Self {
            output,
            output_device_is_input_device,
            fixed_velocity: settings.fixed_velocity,
            double_notes: settings.double_notes.clone()
        }
    }

    fn should_passthrough(&self, message: &MidiMessage) -> bool {
        if self.output_device_is_input_device {
            // never passthrough messages back to the same device
            return false;
        }
        match message {
            // dont send patch changes
            MidiMessage::ProgramChange(_, _) => false,
            MidiMessage::ControlChange(_, ControlFunction::BANK_SELECT, _) => false,
            MidiMessage::ControlChange(_, ControlFunction::BANK_SELECT_LSB, _) => false,
            // do send notes and expression
            MidiMessage::NoteOff(_, _, _) => true,
            MidiMessage::NoteOn(_, _, _) => true,
            MidiMessage::PolyphonicKeyPressure(_, _, _) => true,
            MidiMessage::ControlChange(_, _, _) => true,
            MidiMessage::ChannelPressure(_, _) => true,
            MidiMessage::PitchBendChange(_, _) => true,
            // dont send other weirdness
            MidiMessage::SysEx(_) => false,
            MidiMessage::OwnedSysEx(_) => false,
            MidiMessage::MidiTimeCode(_) => false,
            MidiMessage::SongPositionPointer(_) => false,
            MidiMessage::SongSelect(_) => false,
            MidiMessage::Reserved(_) => false,
            MidiMessage::TuneRequest => false,
            MidiMessage::TimingClock => false,
            MidiMessage::Start => false,
            MidiMessage::Continue => false,
            MidiMessage::Stop => false,
            MidiMessage::ActiveSensing => false,
            MidiMessage::Reset => false
        }
    }
}

impl<'a> Arpeggiator for Passthrough<'a> {
    fn process(&mut self, mut message: MidiMessage<'static>, _signal: &mut dyn StatusSignal) -> Result<(), Box<dyn Error>> {
        if self.should_passthrough(&message) {
            if let Some(fixed) = self.fixed_velocity {
                message = match message {
                    MidiMessage::NoteOff(c, n, _) => MidiMessage::NoteOff(c, n, U7::from_u8_lossy(fixed)),
                    MidiMessage::NoteOn(c, n, _) => MidiMessage::NoteOn(c, n, U7::from_u8_lossy(fixed)),
                    MidiMessage::PolyphonicKeyPressure(c, n, _) => MidiMessage::PolyphonicKeyPressure(c, n, U7::from_u8_lossy(fixed)),
                    _ => message
                };
            }
            if let Some(doubling) = &self.double_notes {
                self.output.send_with_doubling(message, doubling.iter())?;
            } else {
                self.output.send(message)?;
            }
        }
        Ok(())
    }

    fn stop_arpeggios(&mut self) -> Result<(), Box<dyn Error>> {
        Ok(())
    }

    fn count_arpeggios(&self) -> usize {
        1
    }
}
