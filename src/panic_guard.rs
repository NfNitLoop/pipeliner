use std::sync::mpsc;
use std::ops::Drop;
use std::thread;

/// A PanicGuard wraps a Sender<Item=Result<T,()>> and sends an Err(()) through it
/// if the PanicGuard is dropped while the current thread is panicking.
/// This lets the consumer on the other end know immediately that a thread
/// has panicked. (Instead of having to wait until later when we join() the threads.)
pub struct PanicGuard<T, S: Sender<Item=Result<T,()>>>
{
    sender: S
}

impl<T, S: Sender<Item=Result<T,()>>> PanicGuard<T,S> {
    pub fn new(sender: S) -> Self {
        PanicGuard{sender: sender}
    }
    
    pub fn send(&self, item: T) -> Result<(), S::Error> {
        self.sender.send(Ok(item))
    }
}

/// A trait for the common functionality in
/// * mpsc::Sender
/// * mpsc::SyncSender
/// * crossbeam_channel::Sender
pub trait Sender {
    type Item;
    type Error;
    fn send(&self, t: Self::Item) -> Result<(), Self::Error>;
}

impl<T> Sender for mpsc::Sender<T> {
    type Item = T;
    type Error = mpsc::SendError<Self::Item>;
    fn send(&self, t: Self::Item) -> Result<(), Self::Error> {
        mpsc::Sender::send(&self, t)
    }

}
impl<T> Sender for mpsc::SyncSender<T> {
    type Item = T;
    type Error = mpsc::SendError<Self::Item>;
    fn send(&self, t: Self::Item) -> Result<(), Self::Error> {
        mpsc::SyncSender::send(&self, t)
    }
}

impl <T> Sender for ::crossbeam_channel::Sender<T> {
    type Item = T;
    type Error = ::crossbeam_channel::SendError<Self::Item>;
    fn send(&self, t: Self::Item) -> Result<(), Self::Error> {
        ::crossbeam_channel::Sender::send(&self, t)
    }
}

impl<T, S: Sender<Item=Result<T,()>>> Drop for PanicGuard<T,S>
{
    fn drop(&mut self) {
        if thread::panicking() {
            let _result = self.sender.send(Err(()));
        }
    }
}