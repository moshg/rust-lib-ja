// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

extern mod std;
use std::bitv::*;

fn bitv_test() -> bool {
    let v1 = ~Bitv(31, false);
    let v2 = ~Bitv(31, true);
    v1.union(v2);
    true
}

fn main() {
    do iter::repeat(10000) || {bitv_test()};
}
