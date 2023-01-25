use std::sync::mpsc;
use std::fs;
use std::thread;
use std::io::{Read, Write};
use std::error::Error;
use wmidi::MidiMessage;

pub struct InputDevice {
    pub receiver: mpsc::Receiver<MidiMessage<'static>>
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
        while f.read_exact(&mut buf).is_ok() {
            todo!();
            // if tx.send(buf[0]).is_err() {
            //     panic!("Error writing to queue.");
            // }
        }
        println!("NOTE: Input device is not connected.");
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
