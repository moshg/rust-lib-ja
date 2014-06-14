// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![feature(managed_boxes)]

use std::cell::RefCell;
use std::gc::{Gc, GC};

enum taggy {
    cons(Gc<RefCell<taggy>>),
    nil,
}

fn f() {
    let a_box = box(GC) RefCell::new(nil);
    *a_box.borrow_mut() = cons(a_box);
}

pub fn main() {
    f();
}
