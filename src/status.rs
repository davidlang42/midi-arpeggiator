use std::io::Write;
use crate::settings::Settings;
use crate::midi::MidiReceiver;
use wmidi::Channel;

#[cfg(feature = "led")]
use std::cmp::min;

#[cfg(feature = "led")]
use crate::arpeggiator::{ArpeggiatorMode, Pattern};

#[cfg(feature = "led")]
use crate::midi;

#[cfg(feature = "led")]
use wmidi::MidiMessage;

#[cfg(feature = "led")]
use smart_leds::{SmartLedsWrite, RGB8};

#[cfg(feature = "led")]
use ws281x_rpi::Ws2812Rpi;

pub trait StatusSignal: MidiReceiver {
    fn update_settings(&mut self, settings: &Settings);
    fn update_count(&mut self, channel: Channel, arpeggios: usize);
    fn reset_beat(&mut self, channel: Channel);
    fn waiting_for_midi_connect(&mut self);
    fn waiting_for_midi_disconnect(&mut self);
    fn waiting_for_midi_clock(&mut self);
}

pub struct TextStatus<W: Write> {
    counts: [usize; 16],
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
            counts: [0; 16],
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

    fn update_count(&mut self, channel: Channel, arpeggios: usize) {
        let idx = channel.index() as usize;
        if idx < 16 && self.counts[idx] != arpeggios {
            self.counts[idx] = arpeggios;
            writeln!(self.writer, "[Ch {}] Arpeggio count: {}", idx + 1, arpeggios).unwrap();
        }
    }

    fn reset_beat(&mut self, channel: Channel) {
        let idx = channel.index() as usize;
        writeln!(self.writer, "[Ch {}] **Reset beat**", idx + 1).unwrap();
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

#[cfg(feature = "led")]
pub struct GlobalLedStatus<const N: usize> {
    driver: Ws2812Rpi,
    tick: usize,
    running: bool, // runs green if true, red if false
    fixed_steps: Option<usize>, // bar graph from 0 in white
    pattern: Option<Pattern>, // sets run direction
    waiting: Option<(WaitFor, usize)>
}

#[cfg(feature = "led")]
impl<const N: usize> GlobalLedStatus<N> {
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
                data[i] = RGB8::new(16, 16, 16); // white
            }
        }
        if let Some(pattern) = self.pattern {
            let progress = self.tick * data.len() / midi::TICKS_PER_BEAT;
            let index = match pattern {
                Pattern::Up => progress,
                Pattern::Down => data.len() - progress - 1
            };
            data[index] = if self.running {
                RGB8::new(0, 64, 0) // green if arp running
            } else {
                RGB8::new(64, 0, 0) // red if arp stopped
            };
        } else {
            if self.tick < midi::TICKS_PER_BEAT / 2 {
                for i in 0..data.len() {
                    data[i] = RGB8::new(0, 0, 16); // blue for passthrough mode
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
            WaitFor::Clock => RGB8::new(64, 41, 0)
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

#[cfg(feature = "led")]
impl<const N: usize> MidiReceiver for GlobalLedStatus<N> {
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

#[cfg(feature = "led")]
impl<const N: usize> StatusSignal for GlobalLedStatus<N> {
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

    fn update_count(&mut self, _channel: Channel, arpeggios: usize) {
        self.running = arpeggios > 0;
    }

    fn reset_beat(&mut self, _channel: Channel) {
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

#[cfg(feature = "led")]
pub struct ChannelHueLedStatus<const N: usize> {
    driver: Ws2812Rpi,
    tick: usize,
    counts: [usize; 16],
    fixed_steps: Option<usize>,
    pattern: Option<Pattern>,
    waiting: Option<(WaitFor, usize)>
}

#[cfg(feature = "led")]
impl<const N: usize> ChannelHueLedStatus<N> {
    pub fn new(pin: u8) -> Self {
        let mut status = Self {
            driver: Ws2812Rpi::new(N as i32, pin as i32).unwrap(),
            tick: 0,
            counts: [0; 16],
            fixed_steps: None,
            pattern: None,
            waiting: None
        };
        status.update_leds();
        status
    }

    fn channel_color(channel_idx: usize) -> RGB8 {
        // Simple distinct hue mapping based on channel index (0..15)
        match channel_idx % 6 {
            0 => RGB8::new(0, 64, 0),   // Green
            1 => RGB8::new(0, 0, 64),   // Blue
            2 => RGB8::new(64, 64, 0),  // Yellow
            3 => RGB8::new(0, 64, 64),  // Cyan
            4 => RGB8::new(64, 0, 64),  // Magenta
            _ => RGB8::new(64, 32, 0),  // Orange
        }
    }

    fn update_leds(&mut self) {
        let mut data: [RGB8; N] = [RGB8::default(); N];
        if let Some(steps) = self.fixed_steps {
            for i in 0..min(data.len(), steps) {
                data[i] = RGB8::new(16, 16, 16);
            }
        }
        // Display active channels on the strip or highlight based on running status
        let active_channels: Vec<usize> = self.counts.iter().enumerate().filter(|(_, &c)| c > 0).map(|(i, _)| i).collect();
        if !active_channels.is_empty() {
            let progress = self.tick * data.len() / midi::TICKS_PER_BEAT;
            if progress < data.len() {
                let ch_idx = active_channels[self.tick % active_channels.len()];
                data[progress] = Self::channel_color(ch_idx);
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
            WaitFor::Clock => RGB8::new(64, 41, 0)
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

#[cfg(feature = "led")]
impl<const N: usize> MidiReceiver for ChannelHueLedStatus<N> {
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

#[cfg(feature = "led")]
impl<const N: usize> StatusSignal for ChannelHueLedStatus<N> {
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

    fn update_count(&mut self, channel: Channel, arpeggios: usize) {
        let idx = u8::from(channel) as usize;
        if idx < 16 {
            self.counts[idx] = arpeggios;
        }
    }

    fn reset_beat(&mut self, _channel: Channel) {
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