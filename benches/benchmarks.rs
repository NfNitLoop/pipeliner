// Note: Benchmarks require nightly. -_-
// Run with: rustup run nightly cargo bench
#![feature(test)]

extern crate test;
use test::{black_box, Bencher};

extern crate pipeliner;
use pipeliner::Pipeline;

// This benchmark just tests passing through some data unmodified.
// We're really testing the underlying channel implementation.
// (And: currently, sync::Mutex I've added to simulate spmc. Sorry. :p)
#[bench]
fn sending_and_receiving(b: &mut Bencher) {
    let iterations = black_box(10_000);
    let threads = black_box(4);

    b.iter(|| {
        let mut count = 0;
        for _ in (0..iterations).with_threads(threads).map(|x| x) {
            count += 1;
        }

        assert_eq!(iterations, count);
        println!("Foobar {}", iterations);
    });
}