//! The Executor crate gives a high-level framework for parallell processing.
//!
//! Main features:
//!  * Accept input lazily from an Iterator.
//!  * Return all output via a .results() Iterator.
//!  * `panic`s in your worker threads are propagated out of the .results() Iterator. (No silent
//!     loss of data.)
//!
//! TODO: Provide example code.

#[cfg(test)]
mod tests;


use std::marker::PhantomData;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{self, sync_channel};
use std::thread::spawn;


pub struct Executor<In, Out, F>
where In: Send + 'static, Out: Send + 'static, F: Fn(In) -> Out + Send + Sync + 'static {
    callable: F,
    
    // Options:
    num_workers: usize,
    out_buffer: usize,
    in_buffer: usize,
    
    _pd1: PhantomData<In>,
    _pd2: PhantomData<Out>,
}

impl<In, Out, F> Executor<In, Out, F> 
where In: Send + 'static,  Out: Send + 'static, F: Fn(In) -> Out + Send + Sync + 'static {
    
    /// Create a new Executor which will run `callable` on each input.
    pub fn new(callable: F) -> Self {
        Executor {
            callable: callable,
            num_workers: 10, 
            out_buffer: 10,
            in_buffer: 10,
            _pd1: PhantomData, _pd2: PhantomData,
        }
    }
    
    /// Work the input, and make the results available via the ExecutorIterator.
    pub fn work<It>(self, input: It) -> ExecutorIter<Out>
    where It: Iterator<Item=In> + Send + 'static
    {
        let Executor{callable, num_workers, out_buffer, in_buffer, ..} = self;
        
        let (input_tx, input_rx) = sync_channel(in_buffer).into_multi();
        let (output_tx, output_rx) = sync_channel(out_buffer);
        let callable = Arc::new(callable);
        
        let in_thread = spawn(move || {
            for value in input {
                input_tx.send(value).unwrap();
            }
        });
        
        let mut iter = ExecutorIter {
            output: output_rx.into_iter(), 
            worker_threads: Vec::with_capacity(num_workers),
            producer_threads: Vec::with_capacity(1),
        };
        iter.producer_threads.push(in_thread);
        
        // Spawn N worker threads.
        for _ in 0..10 {
            let input_rx = input_rx.clone();
            let output_tx = output_tx.clone();
            let callable = callable.clone();
            
            iter.worker_threads.push(spawn(move || {
                loop {
                    let input = match input_rx.recv() {
                        // TODO: Replace this match with an iterator.
                        Ok(x) => x,
                        Err(_) => break, 
                    };
                    // TODO: Handle panics and send them down the wire.
                    let output = callable(input);
                    let result = output_tx.send(output);
                    if result.is_err() {
                        // The receiver is closed. No need to continue.
                        break;
                    }
                } // loop
            })); // worker
        } // spawning threads
        iter
    }
}

pub struct ExecutorIter<Out>
{
    output: mpsc::IntoIter<Out>,
    worker_threads: Vec<std::thread::JoinHandle<()>>,
    // In reality, only one:
    producer_threads: Vec<std::thread::JoinHandle<()>>,
}

impl<T> ExecutorIter<T> {
    /// Makes panics that were experienced in the worker/producer threads visible on the
    /// consumer thread. This must be called after we've drained self.output, which ensures
    /// that all threads are already finished.
    fn propagate_panics(&mut self) {
        // precondition: All threads should be finished by now:
        assert!(self.output.next().is_none());
        
        use std::mem;
        let workers = mem::replace(&mut self.worker_threads, Vec::new());
        let producers = mem::replace(&mut self.producer_threads, Vec::new());
        for joiners in vec![workers, producers] {
            for joiner in joiners {
                let panic_err = match joiner.join() {
                    Ok(_) => continue, // no error
                    Err(err) => err,
                };
                let orig_msg = panic_msg_from(panic_err.as_ref());
                panic!("Worker thread panicked with message: [{}]", orig_msg);
            }
        }
    }
}

use std::any::Any;

/// Try to reconstruct a panic message from the original:
// Thanks to kimundi on #rust-beginners for helping me sort this out. :) 
fn panic_msg_from<'a>(panic_data: &'a Any) -> &'a str {    
    
    if let Some(msg) = panic_data.downcast_ref::<&'static str>() {
        return msg;
    }
    if let Some(msg) = panic_data.downcast_ref::<String>() {
        return msg.as_str();
    }
    
    "<Unrecoverable panic message.>"
}

impl<T> std::iter::Iterator for ExecutorIter<T> {
    type Item = T;
    
    /// Iterates through executor results.
    /// 
    /// # Panics #
    /// Note, this call will panic if any of the workre threads panicked. 
    /// This is because, in that case, you can't be sure you've received a result for
    /// each of your inputs.
    fn next(&mut self) -> Option<Self::Item> {
        let next = self.output.next();
        if next.is_none() {
            self.propagate_panics();
        }
        return next;
    }
}

/// Implement multi-receive for multi-producer/multi-consumer channels:
struct MultiReceiver<T> {
    recv_mut: Arc<Mutex<mpsc::Receiver<T>>>,
}

impl<T> MultiReceiver<T> {
    fn from(receiver: mpsc::Receiver<T>) -> Self {
        MultiReceiver {
            recv_mut: Arc::new(Mutex::new(receiver))
        }
    }
    
    fn recv(&self) -> Result<T, mpsc::RecvError> {
        let rx = self.recv_mut.lock().expect("No poisoning for MultiReceiver uses.");
        rx.recv()
    }
    
    // TODO: impl IntoIterator && Iterator
    // TODO: Full interface of mpsc::Receiver. (Maybe as trait?)
}

impl<T> Clone for MultiReceiver<T> {
    fn clone(&self) -> Self {
        MultiReceiver {
            recv_mut: self.recv_mut.clone()
        }
    }
}

/// Trait for things that can be converted into a MultiReceiver.
trait IntoMultiReceiver {
    type Output;
    fn into_multi(self) -> Self::Output;
}

impl<T> IntoMultiReceiver for mpsc::Receiver<T> {
    type Output = MultiReceiver<T>;
    fn into_multi(self) -> Self::Output {
        MultiReceiver::from(self)
    }
}

/// For use w/ the return values from channel() and sync_channel().
impl<S, T> IntoMultiReceiver for (S, mpsc::Receiver<T>) {
    type Output = (S, MultiReceiver<T>);
    fn into_multi(self) -> Self::Output {
        let (tx, rx) = self;
        return (tx, MultiReceiver::from(rx));
    }
}

