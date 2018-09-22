// Copyright 2018 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// aux-build:issue_54059.rs
// ignore-stage1
// ignore-wasm32-bare no libc for ffi testing
// ignore-windows - dealing with weird symbols issues on dylibs isn't worth it
// revisions: rpass1

extern crate issue_54059;

fn main() {}
