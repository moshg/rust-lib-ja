// run-pass
// aux-build:re_rebalance_coherence_lib.rs

#![allow(dead_code)]
#![feature(re_rebalance_coherence)]
// check that a generic type with a default value from an associated type can be used without
// specifying the value, and without invoking coherence errors.

extern crate re_rebalance_coherence_lib as lib;
use lib::*;

struct MyString {}

impl LibToOwned for MyString {
    type Owned = String;
}

impl PartialEq<MyString> for LibCow<MyString> {
    fn eq(&self, _other: &MyString) -> bool {
        // Test that the default type is used.
        let _s: &String = &self.o;

        false
    }
}

fn main() {}
