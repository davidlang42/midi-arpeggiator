use crate::arpeggio::{NoteDetails, Step};
use crate::arpeggiator::Pattern;

pub trait FinishSettings {
    fn finish_pattern(&self) -> bool;
}

pub trait PatternSettings {
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

//TODO more methods for receiving settings:
// ** (MK4902 preset buttons) MSB/lsb/prog changes
// ** (RD300NX live set changes) bpm of clock ticks - measure bpm, even number => up, odd number => down
// ** (RD300NX live set changes) fc1/fc2 set to zero if enabled on each channel (0,1,2) on patch change
// ^^ by enabling/disabling pedal/fc1/fc2/bend/mod functions on a certain layer (on patch change keys sends default value (0/8192) to each of these)
// ^^ could use 2 of these (which must NEVER be used, so I guess fc1/fc2 are fairly safe), with 3 layers, thats 6 bits, but one must always be on so it is noticed, so 2^6 - 1 = 63 signals (off + 62 signals)
// ^^ 3 output channels x 4 directions x 1-5 steps per beat = 60 combos < 62 signals
