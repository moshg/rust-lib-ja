// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![feature(libc, exit_status)]

extern crate libc;

#[link(name="foo")]
extern {
    fn should_return_one() -> libc::c_int;
}

fn main() {
    let result = unsafe {
        should_return_one()
    };

    if result != 1 {
        std::env::set_exit_status(255);
    }
}
