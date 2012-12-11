// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

fn foo<T: Copy>(x: &T) -> T{
    match x {
        &a => a
    }
}

fn main() {
    assert foo(&3) == 3;
    assert foo(&'a') == 'a';
    assert foo(&@"Dogs rule, cats drool") == @"Dogs rule, cats drool";
}
