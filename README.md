Pipeliner
=========

Pipeliner is a Rust library to help you create multithreaded work pipelines. You
can choose how many threads each step of the pipeline uses to tune performance
for I/O- or CPU-bound workloads.

The [API docs] contain code examples.

Links
-----

 * [API docs]
 * [Changelog]
 * [Pipeliner] crate on crates.io

[API Docs]: https://docs.rs/pipeliner/
[Changelog]: ./CHANGELOG.md
[Pipeliner]: https://crates.io/crates/pipeliner

Comparison with Rayon
---------------------

[Rayon] is another Rust library for parallel computation. If you're doing purely
CPU-bound work, you may want to try that out to see if it offers better
performance.

Pipeliner, IMHO, offers a simpler interface. That simpler interface makes it
easier to combine parts of a data pipeline that may be I/O-bound and CPU-bound.
Usually in those cases, your bottleneck is I/O, not the speed of your parallel
execution library, so having a nice API may be preferable.

[Rayon]: https://crates.io/crates/rayon