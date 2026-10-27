use std::io::{self, Read};
use std::sync::mpsc::{self, Receiver};
use std::thread;

enum Message {
    Data(Vec<u8>),
    Eof,
    Error(io::Error),
}

pub struct NonBlockingReader {
    receiver: Receiver<Message>,
    cached_msg: std::cell::Cell<Option<Message>>,
    is_eof_reached: std::cell::Cell<bool>,
}

impl NonBlockingReader {
    pub fn from_fd<R: Read + Send + 'static>(mut reader: R) -> Result<Self, io::Error> {
        let (sender, receiver) = mpsc::channel();
        
        thread::spawn(move || {
            let mut buf = vec![0u8; 1024];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => {
                        let _ = sender.send(Message::Eof);
                        break;
                    }
                    Ok(n) => {
                        if sender.send(Message::Data(buf[..n].to_vec())).is_err() {
                            break; 
                        }
                    }
                    Err(e) => {
                        let _ = sender.send(Message::Error(e));
                        break;
                    }
                }
            }
        });

        Ok(Self { 
            receiver,
            cached_msg: std::cell::Cell::new(None),
            is_eof_reached: std::cell::Cell::new(false),
        })
    }

    /// Checks if the stream has reached End-Of-File (EOF) without blocking.
    pub fn is_eof(&self) -> bool {
        if self.is_eof_reached.get() {
            return true;
        }

        if let Some(msg) = self.cached_msg.take() {
            let res = matches!(msg, Message::Eof);
            self.cached_msg.set(Some(msg));
            return res;
        }

        match self.receiver.try_recv() {
            Ok(Message::Eof) => {
                self.is_eof_reached.set(true);
                true
            }
            Ok(msg) => {
                self.cached_msg.set(Some(msg));
                false
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.is_eof_reached.set(true);
                true
            }
            Err(mpsc::TryRecvError::Empty) => false,
        }
    }

    /// Appends all currently available bytes into the provided vector without blocking.
    pub fn read_available(&mut self, buf: &mut Vec<u8>) -> Result<(), io::Error> {
        // 1. Drain the cache first if a message was stashed by `is_eof`
        if let Some(msg) = self.cached_msg.take() {
            match msg {
                Message::Data(mut data) => buf.append(&mut data),
                Message::Eof => {
                    self.is_eof_reached.set(true);
                    return Ok(());
                }
                Message::Error(e) => return Err(e),
            }
        }

        // 2. Continually drain the channel loop to grab all ready chunks
        loop {
            match self.receiver.try_recv() {
                Ok(Message::Data(mut data)) => {
                    buf.append(&mut data);
                }
                Ok(Message::Eof) => {
                    self.is_eof_reached.set(true);
                    break;
                }
                Ok(Message::Error(e)) => {
                    return Err(e);
                }
                Err(mpsc::TryRecvError::Empty) => {
                    // No more data available right now
                    break;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.is_eof_reached.set(true);
                    break;
                }
            }
        }

        Ok(())
    }
}
