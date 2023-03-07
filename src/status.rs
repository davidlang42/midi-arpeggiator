use std::cmp::min;
use std::io::Write;

use crate::arpeggiator::Pattern;
use crate::settings::Settings;
use crate::midi::{self, MidiReceiver};

use smart_leds::{SmartLedsWrite, RGB8};
use wmidi::MidiMessage;
use ws281x_rpi::Ws2812Rpi;

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
    pub fn _new(writer: W) -> Self {
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
}

pub struct LedStatus<const N: usize> {
    driver: Ws2812Rpi,
    tick: usize,
    running: bool, // runs green if true, red if false
    fixed_steps: Option<usize>, // bar graph from 0 in white
    pattern: Pattern // sets run direction
}

impl<const N: usize> LedStatus<N> {
    pub fn new(pin: u8) -> Self {
        let mut status = Self {
            driver: Ws2812Rpi::new(N as i32, pin as i32).unwrap(),
            tick: 0,
            running: false,
            fixed_steps: None,
            pattern: Pattern::Up
        };
        status.update_leds();
        status
    }

    fn update_leds(&mut self) {
        let mut data: [RGB8; N] = [RGB8::default(); N];
        if let Some(steps) = self.fixed_steps {
            for i in 0..min(data.len(), steps) {
                data[i] = RGB8::new(32, 32, 32);
            }
        }
        if self.tick < data.len() {
            let index = match self.pattern {
                Pattern::Up => self.tick,
                Pattern::Down => data.len() - self.tick - 1
            };
            data[index] = if self.running {
                RGB8::new(0, 32, 0)
            } else {
                RGB8::new(32, 0, 0)
            };
        }
        self.driver.write(data.into_iter()).unwrap();
    }
}

impl<const N: usize> MidiReceiver for LedStatus<N> {
    fn passthrough_midi(&mut self, message: wmidi::MidiMessage<'static>) -> Option<wmidi::MidiMessage<'static>> {
        if let MidiMessage::TimingClock = message {
            if self.tick == midi::TICKS_PER_BEAT - 1 {
                self.tick = 0;
            } else {
                self.tick += 1;
            }
            self.update_leds();
        }
        Some(message)
    }
}

impl<const N: usize> StatusSignal for LedStatus<N> {
    fn update_settings(&mut self, settings: &Settings) {
        self.fixed_steps = settings.fixed_steps;
        self.pattern = settings.pattern;
    }

    fn update_count(&mut self, arpeggios: usize) {
        self.running = arpeggios > 0;
    }

    fn reset_beat(&mut self) {
        self.tick = 0;
    }
}