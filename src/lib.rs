//! This crate provides a high-level framework for parallel processing.
//!
//! Main features:
//!
//!  * Accept input lazily from an Iterator.
//!  * Performs work in a user-specified number of threads.
//!  * Return all output via an Iterator.
//!  * Optionally buffer output.
//!  * `panic`s in your worker threads are propagated out of the output Iterator. (No silent
//!     loss of data.)
//!  * No `unsafe` code.
//!
//! Since `IntoIterator`s implement [Pipeline], you can, for example:
//! 
//! ```
//! use pipeliner::Pipeline;
//! for result in (0..100).with_threads(10).map(|x| x + 1) {
//!     println!("result: {}", result);
//! }
//! ```
//! 
//! And, since the output is also an iterator, you can easily create a pipeline
//! with varying number of threads for each step of work:
//!
//! ```
//! use pipeliner::Pipeline;
//! // You might want a high number of threads for high-latency work:
//! let results = (0..100).with_threads(50).map(|x| {
//!     x + 1 // Let's pretend this is high latency. (ex: network access)
//! })
//! // But you might want lower thread usage for cpu-bound work:
//! .with_threads(4).out_buffer(100).map(|x| {
//!     x * x // ow my CPUs :p
//! }); 
//! for result in results {
//!     println!("result: {}", result);
//! }
//! ```
//!
//! [Pipeline]: trait.Pipeline.html

extern crate crossbeam_channel;

mod tests;
mod panic_guard;
mod unordered;


use unordered::PipelineIter;

/// Things which implement this can be used with the Pipeliner library.
pub trait Pipeline<I>
where I: Iterator + Send + 'static, I::Item: Send + 'static
{
    /// Returns an PipelineBuilder that will execute using this many threads, and 0 buffering.
    fn with_threads(self, num_threads: usize) -> PipelineBuilder<I>;
}

/// IntoIterators (and Iterators!) can be used as a Pipeline.
impl<Ii> Pipeline<Ii::IntoIter> for Ii
where Ii: IntoIterator,
      Ii::IntoIter: Send + 'static,
      Ii::Item: Send + 'static
{
    fn with_threads(self, num_threads: usize) -> PipelineBuilder<Ii::IntoIter> {
        PipelineBuilder::new(self.into_iter()).num_threads(num_threads) 
    }
}

/// This is an intermediate data structure which allows you to configure how your pipeline
/// should run.
pub struct PipelineBuilder<I>
where I: Iterator, I::Item: Send + 'static
{
    // The inner iterator which yields the input values
    input: I,
    
    // Options:
    num_threads: usize,
    out_buffer: usize,
}

impl<I> PipelineBuilder<I>
where I: Iterator + Send + 'static, I::Item: Send + 'static
{
    fn new(iterator: I) -> Self {
        PipelineBuilder {
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
    
    /// Perform work on the input, and make the results available via the PipelineIter.
    /// Note that unlike in `Iterator`s, this map does not preserve the ordering of the input.
    /// This allows results to be consumed as soon as they become available.
    pub fn map<F, Out>(self, callable: F) -> impl Iterator<Item=Out>
    where Out: Send + 'static, F: Fn(I::Item) -> Out + Send + Sync + 'static
    {
        // TODO: E0282: Why do I have to declare the type here?
        // Isn't it obvoius from the type that new() returns?
        PipelineIter::<crossbeam_channel::IntoIter<Result<Out, ()>>>::new(self, callable)
    }
}
