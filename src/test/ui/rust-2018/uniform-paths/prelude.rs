// compile-pass
// edition:2018

#![feature(uniform_paths)]

// Macro imported with `#[macro_use] extern crate`
use vec as imported_vec;

// Standard library prelude
use Vec as ImportedVec;

// Built-in type
use u8 as imported_u8;

type A = imported_u8;

fn main() {
    imported_vec![0];
    ImportedVec::<u8>::new();
}
