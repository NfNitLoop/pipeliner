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

use std::sync::{Arc, Mutex};
use std::sync::mpsc::{self, sync_channel};
use std::thread::spawn;

/// Things which implement this can be used with the Executor library.
pub trait Executable<It, In>
where It: Iterator<Item=In> + Send + 'static, In: Send + 'static
{
    fn executor(self) -> Executor<It, In>;
}

/// IntoIterators (and Iterators!) can be worked with an executor.
impl<Ii,It,In> Executable<It, In> for Ii
where Ii: IntoIterator<Item=In, IntoIter=It>,
      It: Iterator<Item=In> + Send + 'static,
      In: Send + 'static
{
    fn executor(self) -> Executor<It, In> {
        Executor {
            input: self.into_iter(),
            num_workers: 10, 
            out_buffer: 10,
        }
    }
}

pub struct Executor<It: Iterator<Item=In>, In: Send + 'static> {
    input: It,
    
    // Options:
    num_workers: usize,
    out_buffer: usize,
}

impl<It, In> Executor<It, In>
where It: Iterator<Item=In> + Send + 'static, In: Send + 'static
{
    /// Work the input, and make the results available via the ExecutorIterator.
    pub fn work<F, Out>(self, callable: F) -> ExecutorIter<Out>
    where Out: Send + 'static, F: Fn(In) -> Out + Send + Sync + 'static
    {
        let Executor{input, num_workers, out_buffer} = self;
        
        let input = SharedIterator::wrap(input);
        
        let (output_tx, output_rx) = sync_channel(out_buffer);
        let callable = Arc::new(callable);
                
        let mut iter = ExecutorIter {
            output: output_rx.into_iter(), 
            worker_threads: Vec::with_capacity(num_workers),
        };
        
        // Spawn N worker threads.
        for _ in 0..10 {
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

