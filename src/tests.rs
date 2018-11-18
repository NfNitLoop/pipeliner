#![cfg(test)]
use super::*;

use std::{thread, time};

#[test]
fn basic_test() {
    
    let input = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 42];
    let results = input.with_threads(10).map(|x| x + 1); 
    let mut results: Vec<_> = results.collect();

    // We have to sort these, because this is unordered:
    results.sort();
    let expected = vec![2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 43];
    assert_eq!(expected, results);
}

// TODO: This test is mostly the same. I spent some time trying to paraemterize
// the test so that I could test map() and ordered_map() using the same code,
// but because they return different implementations of Iterator, things got
// hairy. So, here's some largely copy/paste test code. -_- 
#[test]
fn basic_test_ordered() {
    let input = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 42];
    let results = input.with_threads(10).ordered_map(|x| x + 1); 
    let results: Vec<_> = results.collect();

    // We shouldn't need to sort these:
    let expected = vec![2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 43];
    assert_eq!(expected, results);

}

#[test]
#[should_panic(expected="Worker thread panicked with message: [I don't like the number 14]")]
fn test_panic() {
    // I'm not quite sure how to test that the panic gets propagated out of 'results' as soon
    // as item 14 panics, but you can observe the output by running: 
    // cargo test  test_panic -- --nocapture
    // Output should stop (almost) as soon as 14 panics:
    let results = (1..1000).with_threads(10).map(|x| {
        if x == 14 {
            panic!("I don't like the number {}", x);
        }
        thread::sleep(time::Duration::from_millis(10));
        return x * 2;
    });
    for result in results {
        println!("Got result: {}", result);
    }
}

#[test]
#[should_panic(expected="Worker thread panicked with message: [I don't like the number 14]")]
fn test_panic_ordered() {
    // I'm not quite sure how to test that the panic gets propagated out of 'results' as soon
    // as item 14 panics, but you can observe the output by running: 
    // cargo test  test_panic -- --nocapture
    // Output should stop (almost) as soon as 14 panics:
    let results = (1..1000).with_threads(10).ordered_map(|x| {
        if x == 14 {
            panic!("I don't like the number {}", x);
        }
        thread::sleep(time::Duration::from_millis(10));
        return x * 2;
    });
    for result in results {
        println!("Got result: {}", result);
    }
}

#[test]
fn parallel_test() {
    let input = 0..100;
    let results = input.with_threads(10).map(pipe_a).with_threads(5).map(pipe_b);
    for result in results {
        println!("result: {}", result);
    }
}

#[test]
fn serial_test() {
        let input = 0..100;
        let results = input.map(pipe_a).map(pipe_b);
        for result in results {
            println!("result: {}", result);
        }
}


fn pipe_a(x: i32) -> i32 {
    // Simulate some work:
    thread::sleep(time::Duration::from_millis(20));
    x * 2
}

fn pipe_b(x: i32) -> i32 {
    // Simulate some work:
    thread::sleep(time::Duration::from_millis(10));
    x - 1
}