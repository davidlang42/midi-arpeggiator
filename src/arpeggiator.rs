use std::error::Error;
use crate::arpeggio::{NoteDetails, Step};

pub mod timed;
pub mod synced;

pub enum Pattern {
    Down,
    Up
    //TODO more patterns: Random, Out, In
}

impl Pattern {
    pub fn of(&self, mut notes: Vec<NoteDetails>, steps: usize) -> Vec<Step> {
        // put the notes in order based on the pattern type
        match self {
            Pattern::Down => notes.sort_by(|a, b| a.n.cmp(&b.n)),
            Pattern::Up => notes.sort_by(|a, b| b.n.cmp(&a.n)),
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
        let range = if notes.len() == 2 {
            // if there are only 2 notes, repeat them both
            (0..2).rev()
        } else {
            // otherwise repeat all except first and last notes
            (1..(notes.len() - 1)).rev()
        };
        for i in range {
            notes.push(notes[i].clone())
        }
    }
}

pub trait Arpeggiator {
    fn listen(&mut self) -> Result<(), Box<dyn Error>>;
    fn stop_arpeggios(&mut self) -> Result<(), Box<dyn Error>>;
}
