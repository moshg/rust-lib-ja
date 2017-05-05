// Copyright 2017 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::{mem, ptr};
use std::__rand::{Rng, thread_rng};

use test::{Bencher, black_box};

#[bench]
fn iterator(b: &mut Bencher) {
    // peculiar numbers to stop LLVM from optimising the summation
    // out.
    let v: Vec<_> = (0..100).map(|i| i ^ (i << 1) ^ (i >> 1)).collect();

    b.iter(|| {
        let mut sum = 0;
        for x in &v {
            sum += *x;
        }
        // sum == 11806, to stop dead code elimination.
        if sum == 0 {
            panic!()
        }
    })
}

#[bench]
fn mut_iterator(b: &mut Bencher) {
    let mut v = vec![0; 100];

    b.iter(|| {
        let mut i = 0;
        for x in &mut v {
            *x = i;
            i += 1;
        }
    })
}

#[bench]
fn concat(b: &mut Bencher) {
    let xss: Vec<Vec<i32>> = (0..100).map(|i| (0..i).collect()).collect();
    b.iter(|| {
        xss.concat();
    });
}

#[bench]
fn join(b: &mut Bencher) {
    let xss: Vec<Vec<i32>> = (0..100).map(|i| (0..i).collect()).collect();
    b.iter(|| xss.join(&0));
}

#[bench]
fn push(b: &mut Bencher) {
    let mut vec = Vec::<i32>::new();
    b.iter(|| {
        vec.push(0);
        black_box(&vec);
    });
}

#[bench]
fn starts_with_same_vector(b: &mut Bencher) {
    let vec: Vec<_> = (0..100).collect();
    b.iter(|| vec.starts_with(&vec))
}

#[bench]
fn starts_with_single_element(b: &mut Bencher) {
    let vec: Vec<_> = vec![0];
    b.iter(|| vec.starts_with(&vec))
}

#[bench]
fn starts_with_diff_one_element_at_end(b: &mut Bencher) {
    let vec: Vec<_> = (0..100).collect();
    let mut match_vec: Vec<_> = (0..99).collect();
    match_vec.push(0);
    b.iter(|| vec.starts_with(&match_vec))
}

#[bench]
fn ends_with_same_vector(b: &mut Bencher) {
    let vec: Vec<_> = (0..100).collect();
    b.iter(|| vec.ends_with(&vec))
}

#[bench]
fn ends_with_single_element(b: &mut Bencher) {
    let vec: Vec<_> = vec![0];
    b.iter(|| vec.ends_with(&vec))
}

#[bench]
fn ends_with_diff_one_element_at_beginning(b: &mut Bencher) {
    let vec: Vec<_> = (0..100).collect();
    let mut match_vec: Vec<_> = (0..100).collect();
    match_vec[0] = 200;
    b.iter(|| vec.starts_with(&match_vec))
}

#[bench]
fn contains_last_element(b: &mut Bencher) {
    let vec: Vec<_> = (0..100).collect();
    b.iter(|| vec.contains(&99))
}

#[bench]
fn zero_1kb_from_elem(b: &mut Bencher) {
    b.iter(|| vec![0u8; 1024]);
}

#[bench]
fn zero_1kb_set_memory(b: &mut Bencher) {
    b.iter(|| {
        let mut v = Vec::<u8>::with_capacity(1024);
        unsafe {
            let vp = v.as_mut_ptr();
            ptr::write_bytes(vp, 0, 1024);
            v.set_len(1024);
        }
        v
    });
}

#[bench]
fn zero_1kb_loop_set(b: &mut Bencher) {
    b.iter(|| {
        let mut v = Vec::<u8>::with_capacity(1024);
        unsafe {
            v.set_len(1024);
        }
        for i in 0..1024 {
            v[i] = 0;
        }
    });
}

#[bench]
fn zero_1kb_mut_iter(b: &mut Bencher) {
    b.iter(|| {
        let mut v = Vec::<u8>::with_capacity(1024);
        unsafe {
            v.set_len(1024);
        }
        for x in &mut v {
            *x = 0;
        }
        v
    });
}

#[bench]
fn random_inserts(b: &mut Bencher) {
    let mut rng = thread_rng();
    b.iter(|| {
        let mut v = vec![(0, 0); 30];
        for _ in 0..100 {
            let l = v.len();
            v.insert(rng.gen::<usize>() % (l + 1), (1, 1));
        }
    })
}

#[bench]
fn random_removes(b: &mut Bencher) {
    let mut rng = thread_rng();
    b.iter(|| {
        let mut v = vec![(0, 0); 130];
        for _ in 0..100 {
            let l = v.len();
            v.remove(rng.gen::<usize>() % l);
        }
    })
}

fn gen_ascending(len: usize) -> Vec<u64> {
    (0..len as u64).collect()
}

fn gen_descending(len: usize) -> Vec<u64> {
    (0..len as u64).rev().collect()
}

fn gen_random(len: usize) -> Vec<u64> {
    let mut rng = thread_rng();
    rng.gen_iter::<u64>().take(len).collect()
}

fn gen_mostly_ascending(len: usize) -> Vec<u64> {
    let mut rng = thread_rng();
    let mut v = gen_ascending(len);
    for _ in (0usize..).take_while(|x| x * x <= len) {
        let x = rng.gen::<usize>() % len;
        let y = rng.gen::<usize>() % len;
        v.swap(x, y);
    }
    v
}

fn gen_mostly_descending(len: usize) -> Vec<u64> {
    let mut rng = thread_rng();
    let mut v = gen_descending(len);
    for _ in (0usize..).take_while(|x| x * x <= len) {
        let x = rng.gen::<usize>() % len;
        let y = rng.gen::<usize>() % len;
        v.swap(x, y);
    }
    v
}

fn gen_strings(len: usize) -> Vec<String> {
    let mut rng = thread_rng();
    let mut v = vec![];
    for _ in 0..len {
        let n = rng.gen::<usize>() % 20 + 1;
        v.push(rng.gen_ascii_chars().take(n).collect());
    }
    v
}

fn gen_big_random(len: usize) -> Vec<[u64; 16]> {
    let mut rng = thread_rng();
    rng.gen_iter().map(|x| [x; 16]).take(len).collect()
}

macro_rules! sort {
    ($f:ident, $name:ident, $gen:expr, $len:expr) => {
        #[bench]
        fn $name(b: &mut Bencher) {
            b.iter(|| $gen($len).$f());
            b.bytes = $len * mem::size_of_val(&$gen(1)[0]) as u64;
        }
    }
}

macro_rules! sort_expensive {
    ($f:ident, $name:ident, $gen:expr, $len:expr) => {
        #[bench]
        fn $name(b: &mut Bencher) {
            b.iter(|| {
                let mut v = $gen($len);
                let mut count = 0;
                v.$f(|a: &u64, b: &u64| {
                    count += 1;
                    if count % 1_000_000_000 == 0 {
                        panic!("should not happen");
                    }
                    (*a as f64).cos().partial_cmp(&(*b as f64).cos()).unwrap()
                });
                black_box(count);
            });
            b.bytes = $len as u64 * mem::size_of::<u64>() as u64;
        }
    }
}

sort!(sort, sort_small_ascending, gen_ascending, 10);
sort!(sort, sort_small_descending, gen_descending, 10);
sort!(sort, sort_small_random, gen_random, 10);
sort!(sort, sort_small_big_random, gen_big_random, 10);
sort!(sort, sort_medium_random, gen_random, 100);
sort!(sort, sort_large_ascending, gen_ascending, 10000);
sort!(sort, sort_large_descending, gen_descending, 10000);
sort!(sort, sort_large_mostly_ascending, gen_mostly_ascending, 10000);
sort!(sort, sort_large_mostly_descending, gen_mostly_descending, 10000);
sort!(sort, sort_large_random, gen_random, 10000);
sort!(sort, sort_large_big_random, gen_big_random, 10000);
sort!(sort, sort_large_strings, gen_strings, 10000);
sort_expensive!(sort_by, sort_large_random_expensive, gen_random, 10000);

sort!(sort_unstable, sort_unstable_small_ascending, gen_ascending, 10);
sort!(sort_unstable, sort_unstable_small_descending, gen_descending, 10);
sort!(sort_unstable, sort_unstable_small_random, gen_random, 10);
sort!(sort_unstable, sort_unstable_small_big_random, gen_big_random, 10);
sort!(sort_unstable, sort_unstable_medium_random, gen_random, 100);
sort!(sort_unstable, sort_unstable_large_ascending, gen_ascending, 10000);
sort!(sort_unstable, sort_unstable_large_descending, gen_descending, 10000);
sort!(sort_unstable, sort_unstable_large_mostly_ascending, gen_mostly_ascending, 10000);
sort!(sort_unstable, sort_unstable_large_mostly_descending, gen_mostly_descending, 10000);
sort!(sort_unstable, sort_unstable_large_random, gen_random, 10000);
sort!(sort_unstable, sort_unstable_large_big_random, gen_big_random, 10000);
sort!(sort_unstable, sort_unstable_large_strings, gen_strings, 10000);
sort_expensive!(sort_unstable_by, sort_unstable_large_random_expensive, gen_random, 10000);

macro_rules! reverse {
    ($name:ident, $ty:ident) => {
        #[bench]
        fn $name(b: &mut Bencher) {
            // odd length and offset by 1 to be as unaligned as possible
            let n = 0xFFFFF;
            let mut v: Vec<_> =
                (0..1+(n / mem::size_of::<$ty>() as u64))
                .map(|x| x as $ty)
                .collect();
            b.iter(|| black_box(&mut v[1..]).reverse());
            b.bytes = n;
        }
    }
}

reverse!(reverse_u8, u8);
reverse!(reverse_u16, u16);
reverse!(reverse_u32, u32);
reverse!(reverse_u64, u64);
