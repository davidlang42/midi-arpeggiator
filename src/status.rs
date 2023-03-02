use std::io::Write;

use crate::settings::Settings;
use crate::midi::MidiReceiver;

//TODO (STATUS) call reset_beat() in PedalRecorder when the pedal is pressed down

pub trait StatusSignal: MidiReceiver {
    fn update_settings(&mut self, settings: &Settings);
    fn update_count(&mut self, arpeggios: usize);
    fn reset_beat(&mut self);
}

pub struct TextStatus<W: Write> {
    count: Option<usize>,
    settings: Option<Settings>,
    writer: W
}

impl<W: Write> TextStatus<W> {
    pub fn new(writer: W) -> Self {
        Self {
            count: None,
            settings: None,
            writer
        }
    }
}

impl<W: Write> MidiReceiver for TextStatus<W> { }

impl<W: Write> StatusSignal for TextStatus<W> {
    fn update_settings(&mut self, settings: &Settings) {
        if self.settings.is_none() || self.settings.as_ref().unwrap() != settings {
            self.settings = Some(settings.clone());
            write!(self.writer, "Settings: {:?}", self.settings.as_ref().unwrap()).unwrap();
        }
    }

    fn update_count(&mut self, arpeggios: usize) {
        if self.count.is_none() || self.count.unwrap() != arpeggios {
            self.count = Some(arpeggios);
            write!(self.writer, "Arpeggio count: {}", self.count.unwrap()).unwrap();
        }
    }

    fn reset_beat(&mut self) {
        write!(self.writer, "**Reset beat**").unwrap();
    }
}