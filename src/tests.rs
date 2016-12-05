use super::*;

use std::{thread, time};

#[test]
fn dumb_test() {
    
    let input = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 42];
    let results = input.with_threads(10).map(|x| x + 1); 
    
    // Collect back into a vec: 
    let mut results: Vec<_> = results.collect();
    results.sort();
    for result in &results {
        println!("Got result: {}", result);
    }
    
    assert!(results[0] == 2);
    assert!(results[results.len()-1] == 43);
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