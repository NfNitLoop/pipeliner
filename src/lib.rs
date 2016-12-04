//! The Executor crate gives a high-level framework for parallel processing.
//!
//! Main features:
//!
//!  * Accept input lazily from an Iterator.
//!  * Performs work in a user-specified number of threads.
//!  * Return all output via an Iterator.
//!  * Optionally buffer output.
//!  * `panic`s in your worker threads are propagated out of the output Iterator. (No silent
//!     loss of data.)
//!
//! Since `IntoIterator`s implement [Executable], you can, for example:
//! 
//! ```
//! use executor::Executable;
//! for result in (0..100).with_threads(10).work(|x| x + 1) {
//!     println!("result: {}", result);
//! }
//! ```
//! 
//! And, since the output is also an iterator, you can easily create a pipeline
//! with varying number of threads for each step of work:
//!
//! ```
//! use executor::Executable;
//! // You might want a high number of threads for high-latency work:
//! let results = (0..100).with_threads(50).work(|x| {
//!     x + 1 // Let's pretend this is high latency. (ex: network access)
//! })
//! // But you might want lower thread usage for cpu-bound work:
//! .with_threads(4).out_buffer(100).work(|x| {
//!     x * x // ow my CPUs :p
//! }); 
//! for result in results {
//!     println!("result: {}", result);
//! }
//! ```
//!
//! [Executable]: trait.Executable.html

#[cfg(test)]
mod tests;

use std::sync::{Arc, Mutex};
use std::sync::mpsc::{self, sync_channel};
use std::thread::spawn;

/// Things which implement this can be used with the Executor library.
pub trait Executable<It, In>
where It: Iterator<Item=In> + Send + 'static, In: Send + 'static
{
    /// Returns an Executor that executes using this many threads, and 0 buffering.
    fn with_threads(self, num_threads: usize) -> Executor<It, In>;
}

/// IntoIterators (and Iterators!) can be worked with an executor.
impl<Ii,It,In> Executable<It, In> for Ii
where Ii: IntoIterator<Item=In, IntoIter=It>,
      It: Iterator<Item=In> + Send + 'static,
      In: Send + 'static
{
    fn with_threads(self, num_threads: usize) -> Executor<It, In> {
        Executor::new(self.into_iter()).num_threads(num_threads) 
    }
}

/// This is an intermediate data structure which allows you to configure how your pipeline
/// should run.
pub struct Executor<It: Iterator<Item=In>, In: Send + 'static> {
    // The inner iterator which yields the input values
    input: It,
    
    // Options:
    num_threads: usize,
    out_buffer: usize,
}

impl<It, In> Executor<It, In>
where It: Iterator<Item=In> + Send + 'static, In: Send + 'static
{
    
    fn new(iterator: It) -> Self {
        Executor {
            input: iterator,
            num_threads: 1, 
            out_buffer: 0,
        }
    }
    /// Set how many worker threads should be used to perform this work.
    /// A value of 0 is interpreted as 1.
    pub fn num_threads(mut self, num_threads: usize) -> Self {
        self.num_threads = std::cmp::max(1, num_threads);
        self
    }
    
    /// Set how many output values to cache. The default, 0, results in synchronous output.
    /// Note that in effect each thread caches its output as it waits to send it, so
    /// in many cases you may not need additional output buffering.
    pub fn out_buffer(mut self, size: usize) -> Self {
        self.out_buffer = size;
        self
    }
    
    /// Work the input, and make the results available via the ExecutorIterator.
    pub fn work<F, Out>(self, callable: F) -> ExecutorIter<Out>
    where Out: Send + 'static, F: Fn(In) -> Out + Send + Sync + 'static
    {
        let Executor{input, num_threads, out_buffer} = self;
        
        let input = SharedIterator::wrap(input);
        
        let (output_tx, output_rx) = sync_channel(out_buffer);
        let callable = Arc::new(callable);
                
        let mut iter = ExecutorIter {
            output: output_rx.into_iter(), 
            worker_threads: Vec::with_capacity(num_threads),
        };
        
        // Spawn N worker threads.
        for _ in 0..num_threads {
            let input = input.clone();
            let output_tx = output_tx.clone();
            let callable = callable.clone();
            
            iter.worker_threads.push(spawn(move || {
                for value in input {
                    // TODO: Handle panics and send them down the wire.
                    let output = callable(value);
                    let result = output_tx.send(output);
                    if result.is_err() {
                        // The receiver is closed. No need to continue.
                        break;
                    }
                } 
            })); // worker
        } // spawning threads
        iter
    }
}

pub struct ExecutorIter<Out>
{
    output: mpsc::IntoIter<Out>,
    worker_threads: Vec<std::thread::JoinHandle<()>>,
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
        for joiner in workers {
            let panic_err = match joiner.join() {
                Ok(_) => continue, // no error
                Err(err) => err,
            };
            let orig_msg = panic_msg_from(panic_err.as_ref());
            panic!("Worker thread panicked with message: [{}]", orig_msg);
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
    /// Note, this call will panic if any of the worker threads panicked.
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

/// An iterator which can be safely shared between threads.
struct SharedIterator<I: Iterator> {
    iterator: Arc<Mutex<I>>,
}

// TODO: It feels weird to leak that I'm using Fuse in the impl interface here:
impl<I: Iterator> SharedIterator<std::iter::Fuse<I>> {
    fn wrap(iterator: I) -> Self {
        // Since we're going to be sharing among multiple threads, each thread will need to
        // get a None of its own to end. We need to make sure our iterator doesn't cycle:
        let iterator = iterator.fuse(); 
        
        SharedIterator{iterator: Arc::new(Mutex::new(iterator))}
    }
}

impl<I: Iterator> Clone for SharedIterator<I> {
    fn clone(&self) -> Self {
        SharedIterator{iterator: self.iterator.clone()}
    }
}

impl<I: Iterator> Iterator for SharedIterator<I> {
    type Item = I::Item;
    
    fn next(&mut self) -> Option<Self::Item> {
        let mut iterator = self.iterator.lock().expect("No poisoning in SharedIterator");
        iterator.next()
    }
}

