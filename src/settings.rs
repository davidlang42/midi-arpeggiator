use wmidi::{MidiMessage, U7, ControlFunction};

use crate::arpeggio::{NoteDetails, Step};
use crate::arpeggiator::Pattern;

pub trait MidiReceiver {
    fn passthrough_midi(&mut self, message: MidiMessage<'static>) -> Option<MidiMessage<'static>> {
        Some(message)
    }
}

pub trait FinishSettings: MidiReceiver {
    fn finish_pattern(&self) -> bool;
}

pub trait PatternSettings: MidiReceiver {
    fn generate_steps(&self, notes: Vec<NoteDetails>) -> Vec<Step>;
}

pub enum StopArpeggio {
    Immediately,
    WhenFinished
}

impl FinishSettings for StopArpeggio {
    fn finish_pattern(&self) -> bool {
        match self {
            Self::Immediately => false,
            Self::WhenFinished => true
        }
    }
}

impl MidiReceiver for StopArpeggio { }

pub struct FixedSteps(pub usize, pub Pattern, pub StopArpeggio);

impl FinishSettings for FixedSteps {
    fn finish_pattern(&self) -> bool {
        self.2.finish_pattern()
    }
}

impl PatternSettings for FixedSteps {
    fn generate_steps(&self, notes: Vec<NoteDetails>) -> Vec<Step> {
        self.1.of(notes, self.0)
    }
}

impl MidiReceiver for FixedSteps { }

impl AllSettings for FixedSteps { }

pub struct FixedNotesPerStep(pub usize, pub Pattern, pub StopArpeggio);

impl FinishSettings for FixedNotesPerStep {
    fn finish_pattern(&self) -> bool {
        self.2.finish_pattern()
    }
}

impl PatternSettings for FixedNotesPerStep {
    fn generate_steps(&self, notes: Vec<NoteDetails>) -> Vec<Step> {
        let notes_per_step = self.0;
        let mut steps = 0;
        let mut notes_remaining = notes.len();
        while notes_remaining > 0 {
            steps += 1;
            if notes_remaining <= notes_per_step {
                notes_remaining = 0;
            } else {
                notes_remaining -= notes_per_step;
            }
        }
        self.1.of(notes, steps)
    }
}

impl MidiReceiver for FixedNotesPerStep { }

impl AllSettings for FixedNotesPerStep { }

//TODO implement methods for receiving settings:
// - (MK4902 preset buttons) MSB/lsb/prog changes
// - (RD300NX live set changes) bpm of clock ticks - measure bpm, even number => up, odd number => down
// - (RD300NX live set changes) fc1/fc2 set to zero if enabled on each channel (0,1,2) on patch change
// ** by enabling/disabling pedal/fc1/fc2/bend/mod functions on a certain layer (on patch change keys sends default value (0/8192) to each of these)
// ** could use 2 of these (which must NEVER be used, so I guess fc1/fc2 are fairly safe), with 3 layers, thats 6 bits, but one must always be on so it is noticed, so 2^6 - 1 = 63 signals (off + 62 signals)
// ** 3 output channels x 4 directions x 1-5 steps per beat = 60 combos < 62 signals
// - (RD300NX live set changes) follow rhythm output (or should this be a RhythmFollower synced arpeggiator?)
// ** set keyboard rhythm volume to 0, midi out to ch10, pattern to *something* and turn it on
// ** handle any note-on for ch10 as triggers for arpeggio steps (rather than clock ticks)
// ** "learn" pattern in first beat (24 ticks) by determining steps based on there being any notes on during a tick (how we do know where the start of the beat is? only matters on non-even rhythms)
// ** this determines the number and duration of each step, then when notes are played, they are divided evenly between the steps, with extra notes on earlier steps as required
// ** this requires reading more note-on from midi_out (which currently just reads clock)

trait AllSettings: PatternSettings + FinishSettings { }

pub struct ReceiveProgramChanges {
    settings: Box<dyn AllSettings>,
    msb: U7,
    lsb: U7,
    pc: U7
}

impl FinishSettings for ReceiveProgramChanges {
    fn finish_pattern(&self) -> bool {
        self.settings.finish_pattern()
    }
}

impl PatternSettings for ReceiveProgramChanges {
    fn generate_steps(&self, notes: Vec<NoteDetails>) -> Vec<Step> {
        self.settings.generate_steps(notes)
    }
}

impl MidiReceiver for ReceiveProgramChanges {
    fn passthrough_midi(&mut self, message: MidiMessage<'static>) -> Option<MidiMessage<'static>> {
        match message {
            MidiMessage::ControlChange(_, ControlFunction::BANK_SELECT, msb) => {
                self.msb = msb;
                None
            },
            MidiMessage::ControlChange(_, ControlFunction::BANK_SELECT_LSB, lsb) => {
                self.lsb = lsb;
                None
            },
            MidiMessage::ProgramChange(_, pc) => {
                self.pc = pc;
                self.settings = Self::select_program(self.msb, self.lsb, self.pc);
                None
            },
            _ => Some(message)
        }
    }
}

impl ReceiveProgramChanges {
    const DEFAULT_MSB: u8 = 0;
    const DEFAULT_LSB: u8 = 0;
    const DEFAULT_PC: u8 = 0;

    pub fn new() -> Self {
        let msb = U7::from_u8_lossy(Self::DEFAULT_MSB);
        let lsb = U7::from_u8_lossy(Self::DEFAULT_LSB);
        let pc = U7::from_u8_lossy(Self::DEFAULT_PC);
        let settings = Self::select_program(msb, lsb, pc);
        Self {
            msb,
            lsb,
            pc,
            settings
        }
    }

    fn select_program(msb: U7, lsb: U7, pc: U7) -> Box<dyn AllSettings> {
        // Program Change represents basic settings:
        // - finish (bits 0-2)
        // - pattern (bits 3-6)
        // Bank Select MSB is used for fixed steps (LSB==0):
        // - fixed steps 1-24
        // Bank Select LSB is used for fixed notes (MSB==0):
        // - fixed notes per step 1-127
        let pc_u8: u8 = pc.into();
        if Pattern::OPTIONS.len() > 63 {
            panic!("Too many patterns, not enough space left in U7 for finish flag");
        }
        let pattern = Pattern::OPTIONS[pc_u8 as usize % Pattern::OPTIONS.len()].clone();
        let finish = if pc_u8 as usize > Pattern::OPTIONS.len() { StopArpeggio::WhenFinished } else { StopArpeggio::Immediately };
        match (msb.into(), lsb.into()) {
            (0, lsb_u8) => Box::new(FixedNotesPerStep(lsb_u8 as usize, pattern, finish)),
            (msb_u8, 0) => Box::new(FixedSteps(msb_u8 as usize, pattern, finish)),
            _ => Box::new(FixedSteps(1, Pattern::Down, StopArpeggio::Immediately)) // fallback
        }
    }
}
