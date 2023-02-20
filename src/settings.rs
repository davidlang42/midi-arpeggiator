use wmidi::{MidiMessage, U7, ControlFunction};
use strum::IntoEnumIterator;

use crate::arpeggio::{NoteDetails, Step};
use crate::arpeggiator::{Pattern, ArpeggiatorMode};

pub trait MidiReceiver {
    fn passthrough_midi(&mut self, message: MidiMessage<'static>) -> Option<MidiMessage<'static>> {
        Some(message)
    }
}

pub trait FinishSettings: MidiReceiver {
    fn finish_pattern(&self) -> bool;
}

pub trait PatternSettings: FinishSettings {
    fn generate_steps(&self, notes: Vec<NoteDetails>) -> Vec<Step>;
}

pub trait ModeSettings: PatternSettings {
    fn get_mode(&self) -> ArpeggiatorMode;
}

#[derive(Debug)]
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

//TODO implement methods for receiving settings:
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

pub struct ReceiveProgramChanges {
    mode: ArpeggiatorMode,
    settings: Box<dyn PatternSettings>,
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

impl ModeSettings for ReceiveProgramChanges {
    fn get_mode(&self) -> ArpeggiatorMode {
        self.mode
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
                (self.mode, self.settings) = Self::select_program(self.msb, self.lsb, self.pc);
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
        let (mode, settings) = Self::select_program(msb, lsb, pc);
        Self {
            msb,
            lsb,
            pc,
            mode,
            settings
        }
    }

    fn select_program(msb: U7, lsb: U7, pc: U7) -> (ArpeggiatorMode, Box<dyn PatternSettings>) {
        //let (msb_u8, lsb_u8, pc_u8) = (u8::from(msb) as usize, u8::from(lsb) as usize, u8::from(pc) as usize);
        //TODO shitty hack for testing
        let (msb_u8, lsb_u8, pc_u8) = match (u8::from(msb) as usize, u8::from(lsb) as usize, u8::from(pc) as usize) {
            (0, 0, 0) => (2, 4, 64),
            (0, 0, 1) => (2, 3, 65),
            (0, 0, 2) => (2, 1, 0),
            (0, 0, 3) => (4, 0, 64),
            _ => (0, 0, 0)
        };
        println!("Settings change: MSB {}, LSB {}, PC {}", msb_u8, lsb_u8, pc_u8);//TODO confirm all existing printlns to proper status
        // Bank Select MSB is used for ModeSettings:
        // - 0-127 = ArpeggiatorMode
        // Bank Select LSB is used for PatternSettings type:
        // - 0-63 (first bit=0) for Fixed Steps (1-24)
        // - 64-127 (first bit=1) for Fixed Notes per step (1-63)
        // Program Change represents PatternSettings direction & FinishSettings:
        // - 0-63 (first bit=0) for StopImmediately, Pattern direction (0-63)
        // - 64-127 (first bit=1) for FinishSteps, Pattern direction (0-63)
        let (finish, pattern) = if pc_u8 < 64 {
            (StopArpeggio::Immediately, Pattern::iter().nth(pc_u8 % Pattern::iter().len()).unwrap())
        } else {
            (StopArpeggio::WhenFinished, Pattern::iter().nth((pc_u8 - 64) % Pattern::iter().len()).unwrap())
        };
        println!("{:?}", finish);
        println!("{:?}", pattern);
        
        let settings: Box<dyn PatternSettings> = if lsb_u8 < 64 {
            println!("FixedSteps({})", cap_range(lsb_u8, 1, 24));
            Box::new(FixedSteps(cap_range(lsb_u8, 1, 24), pattern, finish))
        } else {
            println!("FixedNotesPerSteps({})", cap_range(lsb_u8 - 64, 1, 63));
            Box::new(FixedNotesPerStep(cap_range(lsb_u8 - 64, 1, 63), pattern, finish))
        };
        let mode = ArpeggiatorMode::iter().nth(msb_u8 % ArpeggiatorMode::iter().len()).unwrap();
        println!("{:?}", mode);
        (mode, settings)
    }
}

fn cap_range(value: usize, min: usize, max: usize) -> usize {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}