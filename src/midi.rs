use std::path::PathBuf;
use std::sync::mpsc;
use std::fs;
use std::thread;
use std::io::{Read, Write};
use std::error::Error;
use std::thread::JoinHandle;
use std::time::Duration;
use wmidi::ControlFunction;
use wmidi::FromBytesError;
use wmidi::MidiMessage;
use wmidi::Note;
use wmidi::U7;
use nonblock::NonBlockingReader;

pub trait MidiReceiver {
    fn passthrough_midi(&mut self, message: MidiMessage<'static>) -> Option<MidiMessage<'static>> {
        Some(message)
    }
}

pub struct InputDevice {
    receiver: mpsc::Receiver<MidiMessage<'static>>,
    threads: Vec<JoinHandle<()>>
}

pub struct ClockDevice {
    path: PathBuf
}

pub struct OutputDevice {
    sender: mpsc::Sender<MidiMessage<'static>>,
    thread: JoinHandle<()>
}

pub const TICKS_PER_BEAT: usize = 24;

impl InputDevice {
    pub fn _open(midi_in: &str, include_clock_ticks: bool) -> Result<Self, Box<dyn Error>> {
        let (tx, rx) = mpsc::channel();
        let mut input = fs::File::options().read(true).open(midi_in).map_err(|e| format!("Cannot open MIDI IN '{}': {}", midi_in, e))?;
        let join_handle = thread::Builder::new().name(format!("midi-in")).spawn(move || Self::read_into_queue(&mut input, tx, include_clock_ticks, true))?;
        Ok(Self {
            receiver: rx,
            threads: vec![join_handle]
        })
    }

    pub fn open_with_external_clock(midi_in: &str, clock_in: &str, include_msb_lsb_prog_change_from_clock: bool) -> Result<Self, Box<dyn Error>> {
        let (tx, rx) = mpsc::channel();
        let include_clock_ticks = midi_in == clock_in;
        let mut input = fs::File::options().read(true).open(midi_in).map_err(|e| format!("Cannot open MIDI IN '{}': {}", midi_in, e))?;
        let clock = ClockDevice::init(clock_in)?;
        let mut threads = Vec::new();
        if !include_clock_ticks {
            threads.push(clock.connect(tx.clone(), include_msb_lsb_prog_change_from_clock)?);
        }
        threads.push(thread::Builder::new().name(format!("midi-in")).spawn(move || Self::read_into_queue(&mut input, tx, include_clock_ticks, true))?);
        Ok(Self {
            receiver: rx,
            threads
        })
    }

    pub fn read(&mut self) -> Result<MidiMessage<'static>, Box<dyn Error>> {
        for thread in &self.threads {
            if thread.is_finished() {
                // this needs to be an error, because self.receiver can be receiving from multiple senders,
                // and we need to consider this device as finished if either source disconnects
                return Err("Input thread has finished".into());
            }
        }
        let message = self.receiver.recv()?;
        Ok(message)
    }

    fn read_into_queue(f: &mut fs::File, tx: mpsc::Sender<MidiMessage>, include_clock_ticks: bool, rewrite_note_zero_as_off: bool) {
        let mut buf: [u8; 1] = [0; 1];
        let mut bytes = Vec::new();
        while f.read_exact(&mut buf).is_ok() {
            bytes.push(buf[0]);
            match MidiMessage::try_from(bytes.as_slice()) {
                Ok(MidiMessage::TimingClock) if !include_clock_ticks => {
                    // skip clock tick if not required
                    bytes.clear();
                },
                Ok(MidiMessage::NoteOn(c, n, U7::MIN)) if rewrite_note_zero_as_off => {
                    // some keyboards send NoteOn(velocity: 0) instead of NoteOff (eg. Kaysound MK-4902)
                    if let Err(e) = tx.send(MidiMessage::NoteOff(c, n, U7::MIN)) {
                        panic!("Error rewriting NoteOn(0) as NoteOff to input queue: {}", e);
                    }
                    bytes.clear();
                },
                Ok(message) => {
                    // message complete, send to queue
                    if let Err(e) = tx.send(message.to_owned()) {
                        panic!("Error sending to input queue: {}", e);
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
        println!("Input device has disconnected");
    }
}


impl ClockDevice {
    const MIDI_TICK: u8 = 0xF8;
    
    pub fn init(midi_clock: &str) -> Result<Self, Box<dyn Error>> {
        let mut clock = Self {
            path: PathBuf::from(midi_clock)
        };
        clock.wait_for_tick(1000)?;
        Ok(clock)
    }

    pub fn wait_for_tick(&mut self, timeout_ms: u64) -> Result<(), Box<dyn Error>> {
        const SLEEP_MS: u64 = 100;
        let f = fs::File::options().read(true).open(&self.path)
            .map_err(|e| format!("Cannot open Clock device '{}': {}", self.path.display(), e))?;
        let mut noblock = NonBlockingReader::from_fd(f)?;
        let mut elapsed = 0;
        while !noblock.is_eof() && elapsed < timeout_ms {
            let mut buf = Vec::new();
            noblock.read_available(&mut buf)?;
            for byte in buf {
                if byte == Self::MIDI_TICK {
                    // tick detected
                    return Ok(());
                }
            }
            thread::sleep(Duration::from_millis(SLEEP_MS));
            elapsed += SLEEP_MS;
        }
        if noblock.is_eof() {
            Err(format!("Clock device disconnected: {}", self.path.display()).into())
        } else {
            Err(format!("Clock device did not send a clock signal within {}ms: {}", timeout_ms, self.path.display()).into())
        }
    }

    pub fn connect(self, sender: mpsc::Sender<MidiMessage<'static>>, include_msb_lsb_program_change: bool) -> Result<JoinHandle<()>, Box<dyn Error>> {
        let mut clock = fs::File::options().read(true).open(&self.path)
            .map_err(|e| format!("Cannot open Clock device '{}': {}", self.path.display(), e))?;
        if include_msb_lsb_program_change {
            Ok(thread::Builder::new().name(format!("midi-clock")).spawn(move || Self::read_clocks_and_prog_change_into_queue(&mut clock, sender))?)
        } else {
            Ok(thread::Builder::new().name(format!("midi-clock")).spawn(move || Self::read_clocks_into_queue(&mut clock, sender))?)
        }
    }

    fn read_clocks_into_queue(f: &mut fs::File, tx: mpsc::Sender<MidiMessage>) {
        let mut buf: [u8; 1] = [0; 1];
        while f.read_exact(&mut buf).is_ok() {
            if buf[0] == Self::MIDI_TICK {
                // tick detected, send to queue
                if let Err(e) = tx.send(MidiMessage::TimingClock) {
                    panic!("Error sending clock to queue: {}", e);
                }
            }
        }
        println!("Clock device has disconnected");
    }

    fn read_clocks_and_prog_change_into_queue(f: &mut fs::File, tx: mpsc::Sender<MidiMessage>) {
        let mut buf: [u8; 1] = [0; 1];
        let mut bytes = Vec::new();
        while f.read_exact(&mut buf).is_ok() {
            bytes.push(buf[0]);
            match MidiMessage::try_from(bytes.as_slice()) {
                Ok(MidiMessage::TimingClock) => {
                    if let Err(e) = tx.send(MidiMessage::TimingClock) {
                        panic!("Error sending clock to queue: {}", e);
                    }
                    bytes.clear();
                },
                Ok(MidiMessage::ControlChange(ch, ControlFunction::BANK_SELECT, msb)) => {
                    if let Err(e) = tx.send(MidiMessage::ControlChange(ch, ControlFunction::BANK_SELECT, msb)) {
                        panic!("Error sending MSB to queue: {}", e);
                    }
                    bytes.clear();
                },
                Ok(MidiMessage::ControlChange(ch, ControlFunction::BANK_SELECT_LSB, lsb)) => {
                    if let Err(e) = tx.send(MidiMessage::ControlChange(ch, ControlFunction::BANK_SELECT_LSB, lsb)) {
                        panic!("Error sending LSB to queue: {}", e);
                    }
                    bytes.clear();
                },
                Ok(MidiMessage::ProgramChange(ch, pc)) => {
                    if let Err(e) = tx.send(MidiMessage::ProgramChange(ch, pc)) {
                        panic!("Error sending PC to queue: {}", e);
                    }
                    bytes.clear();
                },
                Err(FromBytesError::NoBytes) | Err(FromBytesError::NoSysExEndByte) | Err(FromBytesError::NotEnoughBytes) => {
                    // wait for more bytes
                }, 
                _ => {
                    // invalid (or unwanted) message, clear and wait for next message
                    bytes.clear();
                }
            }
        }
        println!("Clock device has disconnected");
    }
}

impl OutputDevice {
    pub fn open(midi_out: &str) -> Result<Self, Box<dyn Error>> {
        let (tx, rx) = mpsc::channel();
        let mut output = fs::File::options().write(true).open(midi_out).map_err(|e| format!("Cannot open MIDI OUT '{}': {}", midi_out, e))?;
        let thread = thread::Builder::new().name(format!("midi-out")).spawn(move || Self::write_from_queue(&mut output, rx))?;
        Ok(Self {
            sender: tx,
            thread
        })
    }

    pub fn send(&self, message: MidiMessage<'static>) -> Result<(), mpsc::SendError<MidiMessage<'static>>> {
        if self.thread.is_finished() {
            println!("Output thread has finished");
        }
        self.sender.send(message)
    }

    pub fn with_doubling(&self, doubling: &Option<Vec<i8>>) -> MidiOutput {
        MidiOutput::new(self.sender.clone(), if let Some(d) = doubling { d.clone() } else { Vec::new() })
    }

    pub fn send_with_doubling<'a, I: Iterator<Item = &'a i8>>(&self, message: MidiMessage<'static>, doubling: I) -> Result<(), mpsc::SendError<MidiMessage<'static>>> {
        MidiOutput::send_doubles(message, &self.sender, doubling)
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
            if let Err(e) = f.write_all(&buf) {
                panic!("Error writing to output device: {}", e);
            }
            if let Err(e) = f.flush() {
                panic!("Error flushing output device: {}", e);
            }
        }
        println!("Output device has disconnected");
    }
}

pub struct MidiOutput {
    sender: mpsc::Sender<MidiMessage<'static>>,
    doubling: Vec<i8>
}

impl MidiOutput {
    fn new(sender: mpsc::Sender<MidiMessage<'static>>, doubling: Vec<i8>) -> Self {
        Self {
            sender,
            doubling
        }
    }

    pub fn send(&self, message: MidiMessage<'static>) -> Result<(), mpsc::SendError<MidiMessage<'static>>> {
        Self::send_doubles(message, &self.sender, self.doubling.iter())
    }

    fn send_doubles<'a, I: Iterator<Item = &'a i8>>(message: MidiMessage<'static>, sender: &mpsc::Sender<MidiMessage<'static>>, doubling: I) -> Result<(), mpsc::SendError<MidiMessage<'static>>> {
        match message {
            MidiMessage::NoteOff(c, n, v) => for i in doubling {
                if let Some(t) = Self::transpose(n, i) {
                    sender.send(MidiMessage::NoteOff(c, t, v))?;
                }
            },
            MidiMessage::NoteOn(c, n, v) => for i in doubling {
                if let Some(t) = Self::transpose(n, i) {
                    sender.send(MidiMessage::NoteOn(c, t, v))?;
                }
            },
            MidiMessage::PolyphonicKeyPressure(c, n, v) => for i in doubling {
                if let Some(t) = Self::transpose(n, i) {
                    sender.send(MidiMessage::PolyphonicKeyPressure(c, t, v))?;
                }
            },
            _ => {}
        }
        sender.send(message)
    }

    fn transpose(note: Note, delta: &i8) -> Option<Note> {
        let transposed: isize = note as u8 as isize + *delta as isize;
        if transposed >= 0 && transposed <= 127 {
            Some(Note::from_u8_lossy(transposed as u8))
        } else {
            None
        }
    }
}