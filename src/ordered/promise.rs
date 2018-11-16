//! A simple promise implementation. 

use crossbeam_channel;

/// Create a new promise sender/receiver for sending a single value to aonther
/// waiting thread.
pub(crate) fn new<T>() -> (Sender<T>, Receiver<T>) {
    let (tx, rx) = crossbeam_channel::bounded(1);
    (Sender{channel: tx}, Receiver{channel: rx})
}

pub(crate) struct Sender<T>
{
    channel: crossbeam_channel::Sender<T>,
}

impl<T> Sender<T> {
    /// Sends an item, consuming the sender. Never blocks.
    /// Does not error or panic if the receiver is already closed.
    /// (You can only ever send one thing.)
    pub fn send(self, t: T) {
        // We know this never blocks, because we allocate 1 space in the channel,
        // and we only ever send to it this once:
        let result = self.channel.send(t);
        if result.is_err() {
            // This can only happen if the receiver has been dropped.
            // We don't really care. The receiver obviously didn't. :p
        }
    }
}

pub(crate) struct Receiver<T>
{
    channel: crossbeam_channel::Receiver<T>,
}

impl <T> Receiver <T>
{
    /// Block waiting for the value. May return an error if the sender was
    /// dropped before a value was sent.
    pub fn receive(self) -> Result<T, ()> {
        match self.channel.recv() {
            Ok(t) => Ok(t),
            Err(_) => Err(()),
        }
    }
}