use std::collections::HashSet;

use wmidi::Note;

use crate::notename::NoteName;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct Preset {
    pub trigger: Vec<NoteName>,
    pub steps: Vec<NoteName>,
    pub ticks_per_step: usize
}

impl Preset {
    pub fn is_triggered_by(&self, notes: &HashSet<Note>) -> bool {
        self.trigger.iter().all(|n| notes.contains(&n.into()))
    }
}
