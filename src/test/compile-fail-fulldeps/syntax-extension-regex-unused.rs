// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// FIXME(#13725) windows needs fixing.
// ignore-win32
// ignore-stage1

#![feature(phase)]

extern crate regex;
#[phase(syntax)] extern crate regex_macros;

#[deny(unused_variable)]
#[deny(dead_code)]

// Tests to make sure that extraneous dead code warnings aren't emitted from
// the code generated by regex!.

fn main() {
    let fubar = regex!("abc"); //~ ERROR unused variable: `fubar`
}
