use std::collections::HashSet;

use wmidi::Note;

use crate::{arpeggio::synced::Arpeggio, notename::NoteName};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Preset {
    pub trigger: Vec<NoteName>,
    steps: Vec<NoteName>
}

impl Preset {
    pub fn is_triggered_by(&self, notes: &HashSet<Note>) -> bool {
        if self.trigger.len() != notes.len() {
            false
        } else {
            self.trigger.iter().all(|n| notes.contains(&n.into()))
        }
    }

    pub fn make_arpeggio(&self) -> Arpeggio {
        //TODO
        todo!()
    }
}
