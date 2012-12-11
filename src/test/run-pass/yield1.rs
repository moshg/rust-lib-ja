// -*- rust -*-
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
use task::*;

fn main() {
    let mut result = None;
    task::task().future_result(|+r| { result = Some(move r); }).spawn(child);
    error!("1");
    yield();
    option::unwrap(move result).recv();
}

fn child() { error!("2"); }
