use std::cmp::min;
use std::io::Write;

use crate::arpeggiator::{ArpeggiatorMode, Pattern};
use crate::settings::Settings;
use crate::midi::{self, MidiReceiver};

use smart_leds::{SmartLedsWrite, RGB8};
use wmidi::MidiMessage;
use ws281x_rpi::Ws2812Rpi;

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

pub struct LedStatus<const N: usize> {
    driver: Ws2812Rpi,
    tick: usize,
    running: bool, // runs green if true, red if false
    fixed_steps: Option<usize>, // bar graph from 0 in white
    pattern: Option<Pattern>, // sets run direction
    waiting: Option<(WaitFor, usize)>
}

impl<const N: usize> LedStatus<N> {
    pub fn new(pin: u8) -> Self {
        let mut status = Self {
            driver: Ws2812Rpi::new(N as i32, pin as i32).unwrap(),
            tick: 0,
            running: false,
            fixed_steps: None,
            pattern: None,
            waiting: None
        };
        status.update_leds();
        status
    }

    fn update_leds(&mut self) {
        let mut data: [RGB8; N] = [RGB8::default(); N];
        if let Some(steps) = self.fixed_steps {
            for i in 0..min(data.len(), steps) {
                data[i] = RGB8::new(16, 16, 16);
            }
        }
        let running_color = if self.running {
            RGB8::new(0, 64, 0)
        } else {
            RGB8::new(64, 0, 0)
        };
        if let Some(pattern) = self.pattern {
            let progress = self.tick * data.len() / midi::TICKS_PER_BEAT;
            let index = match pattern {
                Pattern::Up => progress,
                Pattern::Down => data.len() - progress - 1
            };
            data[index] = running_color;
        } else {
            if self.tick < midi::TICKS_PER_BEAT / 2 {
                for i in 0..data.len() {
                    data[i] = running_color;
                }
            }
        }
        self.driver.write(data.into_iter()).unwrap();
    }

    fn show_wait(&mut self, wait_for: WaitFor) {
        let new_index = match self.waiting {
            None => 0,
            Some((different, _)) if different != wait_for => 0,
            Some((_same, c)) if c == N - 1 => 0,
            Some((_same, c)) => c + 1
        };
        let new_color = match wait_for {
            WaitFor::Connect => RGB8::new(0, 64, 0),
            WaitFor::Disconnect => RGB8::new(64, 0, 0),
            WaitFor::Clock => RGB8::new(64, 64, 0)
        };
        let mut data: [RGB8; N] = [RGB8::default(); N];
        for i in 0..(new_index + 1) {
            data[i] = new_color;
        }
        self.driver.write(data.into_iter()).unwrap();
        self.waiting = Some((wait_for, new_index));
    }

    fn clear_wait(&mut self) {
        self.waiting = None;
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
        self.clear_wait();
        if settings.mode == ArpeggiatorMode::Passthrough {
            self.fixed_steps = None;
            self.pattern = None;
        } else {
            self.fixed_steps = settings.fixed_steps;
            self.pattern = Some(settings.pattern);
        }
    }

    fn update_count(&mut self, arpeggios: usize) {
        self.running = arpeggios > 0;
    }

    fn reset_beat(&mut self) {
        self.tick = 0;
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