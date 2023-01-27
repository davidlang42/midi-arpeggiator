use crate::arpeggio::NoteDetails;

pub mod timed;
pub mod synced;

pub enum Pattern {
    Down,
    Up
    //TODO more patterns: DownUp, UpDown, Random, Out, In, OutIn, InOut
}

impl Pattern {
    fn of(&self, mut notes: Vec<NoteDetails>) -> Vec<NoteDetails> {
        match self {
            Pattern::Down => notes.sort_by(|a, b| a.n.cmp(&b.n)),
            Pattern::Up => notes.sort_by(|a, b| b.n.cmp(&a.n)),
        }
        notes
    }
}

pub trait Arpeggiator {
    fn listen(&mut self);
    fn stop_arpeggios(&mut self);
}