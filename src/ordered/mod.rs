mod promise;

use std::sync::{Arc, Mutex};
use std::thread::spawn;

use PipelineBuilder;
use unordered::{propagate_panics};


/// Implements the ordered variant of PipelineIter.
pub(crate) struct PipelineIter<I, T>
where I: Iterator<Item=promise::Receiver<T>>
{
    // This is optional because we may want to drop it to cause our threads to
    // die gracefully:
    output: Option<I>,
    worker_threads: Vec<std::thread::JoinHandle<()>>,
}

impl<I, T> PipelineIter<I, T> 
where I: Iterator<Item=promise::Receiver<T>>
{
    fn propagate_panics(&mut self) {
        // Release the output channel so threads can die:
        std::mem::drop(self.output.take());
        propagate_panics(&mut self.worker_threads);
    }
}

pub fn new<I, F, Out>(builder: PipelineBuilder<I>, callable: F) -> impl Iterator<Item=Out>
where
    I: Iterator + Send + 'static,
    I::Item: Send + 'static,
    Out: Send + 'static,
    F: Fn(I::Item) -> Out + Send + Sync + 'static,
{
    let PipelineBuilder{input, num_threads, mut out_buffer} = builder;

    // The output buffer must be at least as large as our num_threads,
    // because it has to hold a promise per running thread so that we can
    // return results in-order via their promises.
    if out_buffer < num_threads {
        out_buffer = num_threads
    }
    
    let (output_tx, output_rx) = crossbeam_channel::bounded(out_buffer);
    let input = PromiseIterator::wrap(input, output_tx);
    let callable = Arc::new(callable);
            
    let mut iter = PipelineIter {
        output: Some(output_rx.into_iter()), 
        worker_threads: Vec::with_capacity(num_threads),
    };
    
    // Spawn N worker threads.
    for _ in 0..num_threads {
        let input = input.clone();
        let callable = callable.clone();
        let worker = move || {
            for (value, promise_tx) in input {
                let output = callable(value);
                promise_tx.send(output);
            } 
        }; 
        iter.worker_threads.push(spawn(worker)); // worker
    } // spawning threads
    iter
}

impl<I, T> std::iter::Iterator for PipelineIter<I,T> 
where I: Iterator<Item=promise::Receiver<T>>
 {
    type Item = T;
    
    /// Iterates through executor results.
    /// 
    /// # Panics #
    /// Note, this call will panic if any of the worker threads panicked.
    /// This is because, in that case, you can't be sure you've received a result for
    /// each of your inputs.
    fn next(&mut self) -> Option<Self::Item> {
        let next_promise = match self.output {
            None => return None,
            Some(ref mut iterator) => iterator,
        }.next();

        let next_promise = match next_promise {
            None => {
                // Finished cleanly, clean up thread handles:
                self.propagate_panics();
                return None;
            },
            Some(p) => p,
        };

        let value = match next_promise.receive() {
            Ok(value) => value,
            Err(_) => {
                // Sender dropped before sending value. Likely panicked:
                self.propagate_panics();
                panic!("self.propagate_panics() should have panicked.");
            },
        };

        Some(value)
    }
}

/// Wraps an iterator. *This* Iterator's next() will call the inner one,
/// create a promise, enqueue the promise_rx on an output channel,
/// and return the (next_value, promise_tx) pair so that a worker can do the
/// work and send the result to the corresponding promise.
struct PromiseIterator<I, Out> 
where I: Iterator
{
    // Since we're going to be sharing among multiple threads, each thread will
    // need to get a None of its own to know we've reached the end of the input.
    // For that, we use a Fused iterator here:
    iterator: Arc<Mutex<std::iter::Fuse<I>>>,
    promise_sink: crossbeam_channel::Sender<promise::Receiver<Out>>,
}

impl<I, Out> PromiseIterator<I, Out>
where I: Iterator
{
    fn wrap(iterator: I, chan: crossbeam_channel::Sender<promise::Receiver<Out>>) -> Self
    {
        PromiseIterator{
            iterator: Arc::new(Mutex::new(iterator.fuse())), 
            promise_sink: chan,
        }
    }
}

impl<I, Out> Iterator for PromiseIterator<I, Out>
where I: Iterator
{
    type Item = (I::Item, promise::Sender<Out>);

    fn next(&mut self) -> Option<Self::Item> {
        // Note, we MUST keep the lock on the iterator for the full body of the
        // function, otherwise the order that our promises get inserted into the
        // promise_sink is nondeterministic.
        let mut iterator = self.iterator.lock().expect(
            "There shoudln't be poisioning in PromiseIterator"
        );
        
        let next_value = match iterator.next() {
            None => return None,
            Some(value) => value,
        };

        let (tx, rx) = promise::new();
        
        // Send our promise rx into the output channel, to maintain the ordering.
        // This may block, which will give backpressure to our threads when
        // the output buffer is full:
        let result = self.promise_sink.send(rx);
        if result.is_err() {
            // The output channel has been dropped. The user isn't interested
            // in any more output, so there's no more work for us to do.
            // Tell our workers that the input is done to let them die.
            return None;
        }

        return Some((next_value, tx));
    }
}

impl<I, Out> Clone for PromiseIterator<I, Out>
where I: Iterator
{
    fn clone(&self) -> Self {
        PromiseIterator {
            iterator: self.iterator.clone(),
            promise_sink: self.promise_sink.clone(),
        }
    }
}




