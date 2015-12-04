// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Test use of const fn from another crate without a feature gate.

// aux-build:const_fn_lib.rs

extern crate const_fn_lib;

use const_fn_lib::foo;

fn main() {
    let x: [usize; foo()] = [];
    //~^ ERROR unimplemented constant expression: calling non-local const fn [E0250]
}
