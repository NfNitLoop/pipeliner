#![cfg(feature = "benchmark")]

use test::Bencher;
use super::Pipeline;

// This benchmark just tests passing through some data unmodified.
// We're really testing the underlying channel implementation.
// (And: currently, sync::Mutex I've added to simulate spmc. Sorry. :p)
#[bench]
fn foobar(b: &mut Bencher) {
    let iterations = 10_000;
    let threads = 4;

    b.iter(|| {
        let mut count = 0;
        for _ in (0..iterations).with_threads(threads).map(|x| x) {
            count += 1;
        }

        assert_eq!(iterations, count);
    });
}