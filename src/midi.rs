use std::path::PathBuf;
use std::sync::mpsc;
use std::fs;
use std::thread;
use std::io::{Read, Write};
use std::error::Error;
use wmidi::FromBytesError;
use wmidi::MidiMessage;

pub struct InputDevice {
    pub receiver: mpsc::Receiver<MidiMessage<'static>>
}

pub struct ClockDevice {
    path: PathBuf
}

pub struct OutputDevice {
    pub sender: mpsc::Sender<MidiMessage<'static>>
}

impl InputDevice {
    pub fn open(midi_in: &str) -> Result<Self, Box<dyn Error>> {
        let (tx, rx) = mpsc::channel();
        let mut input = fs::File::options().read(true).open(midi_in).map_err(|e| format!("Cannot open MIDI IN '{}': {}", midi_in, e))?;
        thread::Builder::new().name(format!("midi-in")).spawn(move || Self::read_into_queue(&mut input, tx))?;
        Ok(Self {
            receiver: rx
        })
    }

    fn read_into_queue(f: &mut fs::File, tx: mpsc::Sender<MidiMessage>) {
        let mut buf: [u8; 1] = [0; 1];
        let mut bytes = Vec::new();
        while f.read_exact(&mut buf).is_ok() {
            bytes.push(buf[0]);
            match MidiMessage::try_from(bytes.as_slice()) {
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
        let clock = Self {
            path: PathBuf::from(midi_clock)
        };
        clock.wait_for_tick()?; // confirm that the file opens AND that it the device is sending CLOCK TICKS
        Ok(clock)
    }

    pub fn wait_for_tick(&self) -> Result<(), Box<dyn Error>> {
        let mut f = fs::File::options().read(true).open(&self.path).map_err(|e| format!("Cannot open MIDI CLOCK '{}': {}", self.path.display(), e))?;
        let mut buf: [u8; 1] = [0; 1];
        while f.read_exact(&mut buf).is_ok() {
            if buf[0] == Self::MIDI_TICK {
                // tick detected
                return Ok(());
            }
        }
        Err(format!("Clock device disconnected: {}", self.path.display()).into())
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
        for mut received in rx {
            let mut buf = Vec::new();
            if received.read_to_end(&mut buf).is_err() {
                panic!("Error writing midi message.")
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
