//! Implementation internals for Pipeliner.map()
//! which provides efficient, but unordered processing of
//! input iterators.

use std::sync::{Arc, Mutex};
use std::thread::spawn;

use PipelineBuilder;
use panic_guard::PanicGuard;

/// An unorderd output iterator which manages its own threads.
pub(crate) struct PipelineIter<I>
where I: Iterator
{
    // This is optional because we may want to drop it to cause our threads to die gracefully:
    output: Option<I>,
    worker_threads: Vec<std::thread::JoinHandle<()>>,
}

impl<I> PipelineIter<I> 
where I: Iterator
{
    pub fn new<F, Out>(builder: PipelineBuilder<I>, callable: F) -> impl Iterator<Item=Out>
    where I: Iterator + Send + 'static,
          I::Item: Send + 'static,
          Out: Send + 'static,
          F: Fn(I::Item) -> Out + Send + Sync + 'static,
    {
        let PipelineBuilder{input, num_threads, out_buffer} = builder;
        
        let input = SharedIterator::wrap(input);
        
        let (output_tx, output_rx) = crossbeam_channel::bounded(out_buffer);
        let callable = Arc::new(callable);
                
        let mut iter = PipelineIter {
            output: Some(output_rx.into_iter()), 
            worker_threads: Vec::with_capacity(num_threads),
        };
        
        // Spawn N worker threads.
        for _ in 0..num_threads {
            let input = input.clone();
            let output_tx = PanicGuard::new(output_tx.clone());
            let callable = callable.clone();
            
            iter.worker_threads.push(spawn(move || {
                for value in input {
                    let output = callable(value);
                    let result = output_tx.send(output);
                    if result.is_err() {
                        // The receiver is closed (has likely been dropped).
                        // No need to continue. Exit our threads to free
                        // up thread/channel resources.
                        break;
                    }
                } 
            })); // worker
        } // spawning threads
        iter
    }

    /// Makes panics that were experienced in the worker/producer threads visible on the
    /// consumer thread. Calling this function ends output -- we will wait for threads to finish
    /// and propagate any panics we find.
    fn propagate_panics(&mut self) {
        // Drop our output iterator. Allows threads to end gracefully. Which is required because
        // we're about to join on them:
        std::mem::drop(self.output.take());
        propagate_panics(&mut self.worker_threads)
    }
}

/// Wait for all threads to close. Panics if any of them panicked.
pub(crate) fn propagate_panics(threads: &mut Vec<std::thread::JoinHandle<()>>) {
    let workers = std::mem::replace(threads, Vec::new());
    for joiner in workers {
        let panic_err = match joiner.join() {
            Ok(_) => continue, // no error
            Err(err) => err,
        };
        let orig_msg = panic_msg_from(panic_err.as_ref());
        panic!("Worker thread panicked with message: [{}]", orig_msg);
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

impl<I> std::iter::Iterator for PipelineIter<I> 
where I: Iterator, I::Item: ResultTrait {
    type Item = <I::Item as ResultTrait>::Ok;
    
    /// Iterates through executor results.
    /// 
    /// # Panics #
    /// Note, this call will panic if any of the worker threads panicked.
    /// This is because, in that case, you can't be sure you've received a result for
    /// each of your inputs.
    fn next(&mut self) -> Option<Self::Item> {
        
        // We may or may not have an output iterator. (it's an Option)
        // If not, we're done. If yes, grab the next value from it. (also an Option)
        let next = {
            // borrowing by ref from self, limit scope:
            let mut output = match self.output {
                None => return None,
                Some(ref mut output) => output,
            };
            output.next()
        };
        let next_result = match next {
            Some(result) => result,
            None => {
                // We've reached the end of our output:
                self.propagate_panics(); 
                return None
            },
        };
        let next_value = match next_result.result() {
            Ok(value) => value,
            // This indicates that one of our worker threads panicked.
            // That means its thread has died due to panic. We don't want to continue operating
            // in degraded mode for who knows how long. We'll just fail fast:
            Err(_) => {
                self.propagate_panics();
                return None;
            }
        };
        Some(next_value)
    }
}

/// An iterator which can be safely shared between threads.
struct SharedIterator<I: Iterator> {
    // Since we're going to be sharing among multiple threads, each thread will
    // need to get a None of its own to know we've reached the end of the input.
    // For that, we use a Fused iterator here:
    iterator: Arc<Mutex<std::iter::Fuse<I>>>,
}

impl<I: Iterator> SharedIterator<I> {
    fn wrap(iterator: I) -> Self {
        SharedIterator{
            iterator: Arc::new(
                Mutex::new(
                    iterator.fuse()
                )
            )
        }
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

// This lets me access a Result's Ok/Err types as associated types:
pub(crate) trait ResultTrait {
    type Ok;
    type Err;

    // Get access to the underlying result:
    fn result(self) -> Result<Self::Ok, Self::Err>;
}

impl<O, E> ResultTrait for Result<O, E> {
    type Ok = O;
    type Err = E;

    #[inline]
    fn result(self) -> Result<O, E> { self }
}
