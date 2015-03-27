// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// aux-build:issue-12133-rlib.rs
// aux-build:issue-12133-dylib.rs
// no-prefer-dynamic

// pretty-expanded FIXME #23616

extern crate issue_12133_rlib as a;
extern crate issue_12133_dylib as b;

fn main() {}
