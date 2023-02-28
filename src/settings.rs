use wmidi::{MidiMessage, U7, ControlFunction};
use strum::IntoEnumIterator;

use crate::arpeggio::{NoteDetails, Step};
use crate::arpeggiator::{Pattern, ArpeggiatorMode};

pub struct Settings {
    finish_pattern: bool,
    fixed_velocity: Option<U7>, //TODO u8?
    pub mode: ArpeggiatorMode,
    fixed_steps: Option<u8>,
    //TODO fixed steps per beat, fixed_beats?
    fixed_notes_per_step: Option<u8>,
}

impl Settings {
    pub fn velocity(&self, recorded_velocity: U7) -> U7 {
        if let Some(fixed) = self.fixed_velovity {
            fixed
        } else {
            recorded_velocity
        }
    }

    pub fn generate_steps(&self, notes: Vec<NoteDetails>) -> Vec<Step> {
        //fixed_steps: self.1.of(notes, self.0)
        todo!();
        //fixed notes per step:
        // let notes_per_step = self.0;
        // let mut steps = 0;
        // let mut notes_remaining = notes.len();
        // while notes_remaining > 0 {
        //     steps += 1;
        //     if notes_remaining <= notes_per_step {
        //         notes_remaining = 0;
        //     } else {
        //         notes_remaining -= notes_per_step;
        //     }
        // }
        // self.1.of(notes, steps)
    }
}

//TODO (SETTINGS) implement rhythm follower settings getter
// FIRST: make sure this provides value for the types of arp I need, if it doesn't turn it into a github issue for future reference
// ** set keyboard rhythm volume to 0, midi out to ch10, pattern to *something* and turn it on
// ** handle any note-on for ch10 as triggers for arpeggio steps (rather than clock ticks)
// ** "learn" pattern in first beat (24 ticks) by determining steps based on there being any notes on during a tick (how we do know where the start of the beat is? only matters on non-even rhythms)
// ** this determines the number and duration of each step, then when notes are played, they are divided evenly between the steps, with extra notes on earlier steps as required
// ** this requires reading more note-on from midi_out (which currently just reads clock)

pub trait SettingsGetter {
    fn passthrough_midi(&mut self, message: MidiMessage<'static>) -> Option<MidiMessage<'static>>;
    fn get(&self) -> &Settings;
}

pub struct PredefinedProgramChanges {
    predefined: Vec<Settings>,
    index: usize,
    msb: U7,
    lsb: U7,
    pc: U7
}


impl SettingsGetter for PredefinedProgramChanges {
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
                self.index = ((self.msb * U7::MAX + self.lsb) * U7::MAX + self.pc).into() % self.predefined.len();
                None
            },
            _ => Some(message)
        }
    }

    fn get(&self) -> &Settings {
        &self.predefined[self.index]
    }
}

impl PredefinedProgramChanges {
    pub fn new(predefined: Vec<Settings>) -> Self {
        if predefined.len() > U7::MAX * U7::MAX * U7::MAX {
            panic!("Too many predefined program changes for 3 U7s");
        }
        Self {
            predefined,
            msb: 0,
            lsb: 0,
            pc: 0,
            index: 0
        }
    }
}

//TODO finish implementing DecodeProgramChanges (if worth it)
// pub struct DecodeProgramChanges {
//     current: Settings,
//     msb: U7,
//     lsb: U7,
//     pc: U7,
// }

// impl SettingsGetter for DecodeProgramChanges {
//     fn passthrough_midi(&mut self, message: MidiMessage<'static>) -> Option<MidiMessage<'static>> {
//         match message {
//             MidiMessage::ControlChange(_, ControlFunction::BANK_SELECT, msb) => {
//                 self.msb = msb;
//                 None
//             },
//             MidiMessage::ControlChange(_, ControlFunction::BANK_SELECT_LSB, lsb) => {
//                 self.lsb = lsb;
//                 None
//             },
//             MidiMessage::ProgramChange(_, pc) => {
//                 self.pc = pc;
//                 self.current = Self::select_program(self.msb, self.lsb, self.pc);
//                 None
//             },
//             _ => Some(message)
//         }
//     }

//     fn get(&self) -> &Settings {
//         &self.current
//     }
// }

// impl DecodeProgramChanges {
//     const DEFAULT_MSB: u8 = 0;
//     const DEFAULT_LSB: u8 = 0;
//     const DEFAULT_PC: u8 = 0;

//     pub fn new() -> Self {
//         let msb = U7::from_u8_lossy(Self::DEFAULT_MSB);
//         let lsb = U7::from_u8_lossy(Self::DEFAULT_LSB);
//         let pc = U7::from_u8_lossy(Self::DEFAULT_PC);
//         let current = Self::select_program(msb, lsb, pc);
//         Self {
//             msb,
//             lsb,
//             pc,
//             current
//         }
//     }

//     fn select_program(msb: U7, lsb: U7, pc: U7) -> Settings {
//         //let (msb_u8, lsb_u8, pc_u8) = (u8::from(msb) as usize, u8::from(lsb) as usize, u8::from(pc) as usize);
//         //TODO shitty hack for testing - probably load these from a file instead
//         let (msb_u8, lsb_u8, pc_u8) = match (u8::from(msb) as usize, u8::from(lsb) as usize, u8::from(pc) as usize) {
//             (0, 0, 0) => (2, 4, 64),
//             (0, 0, 1) => (2, 3, 65),
//             (0, 0, 2) => (2, 1, 0),
//             (0, 0, 3) => (4, 0, 64),
//             _ => (0, 0, 0)
//         };
//         //TODO (STATUS) convert all existing printlns to proper status
//         println!("Settings change: MSB {}, LSB {}, PC {}", msb_u8, lsb_u8, pc_u8);
//         // Bank Select MSB is used for ModeSettings:
//         // - 0-127 = ArpeggiatorMode
//         // Bank Select LSB is used for PatternSettings type:
//         // - 0-63 (first bit=0) for Fixed Steps (1-24)
//         // - 64-127 (first bit=1) for Fixed Notes per step (1-63)
//         // Program Change represents PatternSettings direction & FinishSettings:
//         // - 0-63 (first bit=0) for StopImmediately, Pattern direction (0-63)
//         // - 64-127 (first bit=1) for FinishSteps, Pattern direction (0-63)
//         let (finish, pattern) = if pc_u8 < 64 {
//             (StopArpeggio::Immediately, Pattern::iter().nth(pc_u8 % Pattern::iter().len()).unwrap())
//         } else {
//             (StopArpeggio::WhenFinished, Pattern::iter().nth((pc_u8 - 64) % Pattern::iter().len()).unwrap())
//         };
//         println!("{:?}", finish);
//         println!("{:?}", pattern);
        
//         let settings: Box<dyn PatternSettings> = if lsb_u8 < 64 {
//             println!("FixedSteps({})", cap_range(lsb_u8, 1, 24));
//             Box::new(FixedSteps(cap_range(lsb_u8, 1, 24), pattern, Self::FIXED_VELOCITY, finish))
//         } else {
//             println!("FixedNotesPerSteps({})", cap_range(lsb_u8 - 64, 1, 63));
//             Box::new(FixedNotesPerStep(cap_range(lsb_u8 - 64, 1, 63), pattern, Self::FIXED_VELOCITY, finish))
//         };
//         let mode = ArpeggiatorMode::iter().nth(msb_u8 % ArpeggiatorMode::iter().len()).unwrap();
//         println!("{:?}", mode);
//         (mode, settings)
//     }
// }

fn cap_range(value: usize, min: usize, max: usize) -> usize {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}