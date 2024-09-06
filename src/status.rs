
use std::io::Write;
use crate::settings::Settings;
use crate::midi::{MidiReceiver};


pub trait StatusSignal: MidiReceiver {
    fn update_settings(&mut self, settings: &Settings);
    fn update_count(&mut self, arpeggios: usize);
    fn reset_beat(&mut self);
    fn waiting_for_midi_connect(&mut self);
    fn waiting_for_midi_disconnect(&mut self);
    fn waiting_for_midi_clock(&mut self);
}

pub struct TextStatus<W: Write> {
    count: Option<usize>,
    settings: Option<Settings>,
    waiting: Option<WaitFor>,
    writer: W
}

#[derive(Copy, Clone, PartialEq)]
enum WaitFor {
    Connect,
    Disconnect,
    Clock
}

impl<W: Write> TextStatus<W> {
    pub fn _new(writer: W) -> Self {
        Self {
            count: None,
            settings: None,
            waiting: None,
            writer
        }
    }

    fn show_wait(&mut self, wait_for: WaitFor) {
        if let Some(already) = self.waiting {
            if already == wait_for {
                return;
            }
        }
        match wait_for {
            WaitFor::Connect => println!("Waiting for MIDI devices to connect"),
            WaitFor::Disconnect => println!("Waiting for MIDI devices to disconnect"),
            WaitFor::Clock => println!("Waiting for MIDI devices to send clock ticks")
        }
        self.waiting = Some(wait_for);
    }

    fn clear_wait(&mut self) {
        self.waiting = None;
    }
}

impl<W: Write> MidiReceiver for TextStatus<W> { }

impl<W: Write> StatusSignal for TextStatus<W> {
    fn update_settings(&mut self, settings: &Settings) {
        self.clear_wait();
        if self.settings.is_none() || self.settings.as_ref().unwrap() != settings {
            self.settings = Some(settings.clone());
            writeln!(self.writer, "{:?}", self.settings.as_ref().unwrap()).unwrap();
        }
    }

    fn update_count(&mut self, arpeggios: usize) {
        if self.count.is_none() || self.count.unwrap() != arpeggios {
            self.count = Some(arpeggios);
            writeln!(self.writer, "Arpeggio count: {}", self.count.unwrap()).unwrap();
        }
    }

    fn reset_beat(&mut self) {
        writeln!(self.writer, "**Reset beat**").unwrap();
    }
    
    fn waiting_for_midi_connect(&mut self) {
        self.show_wait(WaitFor::Connect);
    }
    
    fn waiting_for_midi_disconnect(&mut self) {
        self.show_wait(WaitFor::Disconnect);
    }
    
    fn waiting_for_midi_clock(&mut self) {
        self.show_wait(WaitFor::Clock);
    }
}