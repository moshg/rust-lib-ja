// Copyright 2016 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![allow(dead_code)]
#![deny(extra_requirement_in_impl)]

// Test that we elaborate `Type: 'region` constraints and infer various important things.

trait Master<'a, 'b> {
    fn foo();
}

// `U: 'a` does not imply `V: 'a`
impl<'a, 'b> Master<'a, 'b> for () {
    fn foo() where 'a: 'b { }
    //~^ ERROR parameter type `V` may not live long enough
}

fn main() {
    println!("Hello, world!");
}
