use std::path::PathBuf;
use std::sync::mpsc;
use std::fs;
use std::thread;
use std::io::{Read, Write};
use std::error::Error;
use std::thread::JoinHandle;
use std::time::Duration;
use wmidi::FromBytesError;
use wmidi::MidiMessage;

pub struct InputDevice {
    pub receiver: mpsc::Receiver<MidiMessage<'static>>
}

struct ClockDevice {
    path: PathBuf
}

pub struct OutputDevice {
    pub sender: mpsc::Sender<MidiMessage<'static>>
}

pub const TICKS_PER_BEAT: usize = 24;

impl InputDevice {
    pub fn open(midi_in: &str, include_clock_ticks: bool) -> Result<Self, Box<dyn Error>> {
        let (tx, rx) = mpsc::channel();
        let mut input = fs::File::options().read(true).open(midi_in).map_err(|e| format!("Cannot open MIDI IN '{}': {}", midi_in, e))?;
        thread::Builder::new().name(format!("midi-in")).spawn(move || Self::read_into_queue(&mut input, tx, include_clock_ticks))?;
        Ok(Self {
            receiver: rx
        })
    }

    pub fn open_with_external_clock(midi_in: &str, clock_in: &str) -> Result<Self, Box<dyn Error>> {
        let (tx, rx) = mpsc::channel();
        let include_clock_ticks = midi_in == clock_in;
        let mut input = fs::File::options().read(true).open(midi_in).map_err(|e| format!("Cannot open MIDI IN '{}': {}", midi_in, e))?;
        let clock = ClockDevice::init(clock_in)?;
        if !include_clock_ticks {
            clock.connect(tx.clone())?;
        }
        thread::Builder::new().name(format!("midi-in")).spawn(move || Self::read_into_queue(&mut input, tx, include_clock_ticks))?;
        Ok(Self {
            receiver: rx
        })
    }

    fn read_into_queue(f: &mut fs::File, tx: mpsc::Sender<MidiMessage>, include_clock_ticks: bool) {
        let mut buf: [u8; 1] = [0; 1];
        let mut bytes = Vec::new();
        while f.read_exact(&mut buf).is_ok() {
            bytes.push(buf[0]);
            match MidiMessage::try_from(bytes.as_slice()) {
                Ok(MidiMessage::TimingClock) if !include_clock_ticks => {
                    // skip clock tick if not required
                    bytes.clear();
                },
                Ok(message) => {
                    // message complete, send to queue
                    if tx.send(message.to_owned()).is_err() {
                        panic!("Error sending to queue.");
                    }
                    bytes.clear();
                },
                Err(FromBytesError::NoBytes) | Err(FromBytesError::NoSysExEndByte) | Err(FromBytesError::NotEnoughBytes) => {
                    // wait for more bytes
                }, 
                _ => {
                    // invalid message, clear and wait for next message
                    bytes.clear();
                }
            }
        }
        println!("NOTE: Input device is not connected.");
    }
}


impl ClockDevice {
    const MIDI_TICK: u8 = 0xF8;
    
    pub fn init(midi_clock: &str) -> Result<Self, Box<dyn Error>> {
        let mut clock = Self {
            path: PathBuf::from(midi_clock)
        };
        let test: JoinHandle<Result<ClockDevice, String>> = thread::spawn(move || {
            clock.wait_for_tick()?; // confirm that the file opens AND that it the device is sending CLOCK TICKS
            Ok(clock)
        });
        const SLEEP_COUNT: u64 = 100;
        const SLEEP_MS: u64 = 10;
        for _ in 0..SLEEP_COUNT {
            if test.is_finished() {
                break;
            }
            thread::sleep(Duration::from_millis(SLEEP_MS));
        }
        if !test.is_finished() {
            Err(format!("MIDI CLOCK did not send a clock signal within {}ms (less than {:0.0} bpm): {}", SLEEP_COUNT * SLEEP_MS, 60000.0 / ((TICKS_PER_BEAT as u64 * SLEEP_COUNT * SLEEP_MS) as f64), midi_clock).into())
        } else {
            match test.join() {
                Ok(Ok(clock)) => Ok(clock),
                Ok(Err(s)) => Err(s.into()),
                Err(e) => Err(format!("{:?}", e).into())
            }
        }
    }

    pub fn wait_for_tick(&mut self) -> Result<(), String> {
        let mut f = fs::File::options().read(true).open(&self.path).map_err(|e| format!("Cannot open MIDI CLOCK '{}': {}", self.path.display(), e))?;
        let mut buf: [u8; 1] = [0; 1];
        while f.read_exact(&mut buf).is_ok() {
            if buf[0] == Self::MIDI_TICK {
                // tick detected
                return Ok(());
            }
        }
        Err(format!("Clock device disconnected: {}", self.path.display()))
    }

    pub fn connect(self, sender: mpsc::Sender<MidiMessage<'static>>) -> Result<(), Box<dyn Error>> {
        let mut clock = fs::File::options().read(true).open(&self.path).map_err(|e| format!("Cannot open MIDI CLOCK '{}': {}", self.path.display(), e))?;
        thread::Builder::new().name(format!("midi-clock")).spawn(move || Self::read_clocks_into_queue(&mut clock, sender))?;
        Ok(())
    }

    fn read_clocks_into_queue(f: &mut fs::File, tx: mpsc::Sender<MidiMessage>) {
        let mut buf: [u8; 1] = [0; 1];
        while f.read_exact(&mut buf).is_ok() {
            if buf[0] == Self::MIDI_TICK {
                // tick detected, send to queue
                if tx.send(MidiMessage::TimingClock).is_err() {
                    panic!("Error sending to queue.");
                }
            }
        }
        println!("NOTE: Clock device is not connected.");
    }
}

impl OutputDevice {
    pub fn open(midi_out: &str) -> Result<Self, Box<dyn Error>> {
        let (tx, rx) = mpsc::channel();
        let mut output = fs::File::options().write(true).open(midi_out).map_err(|e| format!("Cannot open MIDI OUT '{}': {}", midi_out, e))?;
        thread::Builder::new().name(format!("midi-out")).spawn(move || Self::write_from_queue(&mut output, rx))?;
        Ok(Self {
            sender: tx
        })
    }

    fn write_from_queue(f: &mut fs::File, rx: mpsc::Receiver<MidiMessage>) {
        let mut buf = Vec::new();
        for received in rx {
            let expected = received.bytes_size();
            buf.resize(expected, 0);
            match received.copy_to_slice(&mut buf) {
                Ok(found) if found != expected => panic!("Error writing midi message: Not enough bytes (expected {} found {}).", expected, found),
                Err(_) => panic!("Error writing midi message: Too many bytes (expected {}).", expected),
                _ => {}
            }
            if f.write_all(&buf).is_err() {
                panic!("Error writing to device.")
            }
            if f.flush().is_err() {
                panic!("Error flushing to device.");
            }
        }
        panic!("Writing from queue has finished.");
    }
}
