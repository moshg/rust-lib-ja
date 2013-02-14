// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Tests that auto-ref can't create mutable aliases to immutable memory.

struct Foo {
    x: int
}

trait Stuff {
    fn printme();
}

impl Stuff for &mut Foo {
    fn printme() {
        io::println(fmt!("%d", self.x));
    }
}

fn main() {
    let x = Foo { x: 3 };
    x.printme();    //~ ERROR illegal borrow
}

