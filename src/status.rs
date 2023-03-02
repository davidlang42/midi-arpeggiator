use std::io::Write;

use crate::settings::Settings;

//TODO (STATUS) show tempo
//TODO (STATUS) arpeggiators send a "start beat" signal, which syncs the clock beat to start at the next midi tick (for example PedalRecorder will mark the start of the beat when the pedal is pressed down)

pub trait StatusSignal {
    fn update_settings(&mut self, settings: &Settings);
    fn update_count(&mut self, arpeggios: usize);
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
}