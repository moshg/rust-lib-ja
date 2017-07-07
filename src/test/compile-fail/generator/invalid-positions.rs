// Copyright 2017 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![feature(generators)]

const A: u8 = { yield 3u8; gen arg; 3u8};
//~^ ERROR yield statement outside
//~| ERROR gen arg expression outside

static B: u8 = { yield 3u8; gen arg; 3u8};
//~^ ERROR yield statement outside
//~| ERROR gen arg expression outside

fn main() { yield; gen arg; }
//~^ ERROR yield statement outside
//~| ERROR gen arg expression outside
