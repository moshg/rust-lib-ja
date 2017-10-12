// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// error-pattern: the trait bound `MyStruct: Foo` is not satisfied

#![feature(specialization)]

trait Foo {
    fn foo_one(&self) -> &'static str;
    fn foo_two(&self) -> &'static str;
}

default impl<T> Foo for T {
    fn foo_one(&self) -> &'static str {
        "generic"
    }
}

fn foo<T: Foo>(x: T) -> &'static str {
    x.foo_one()
}

struct MyStruct;

fn main() {
    println!("{:?}", foo(MyStruct));
}
