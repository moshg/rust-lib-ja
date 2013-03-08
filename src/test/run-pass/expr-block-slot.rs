// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Regression test for issue #377

struct A { a: int }
struct V { v: int }

pub fn main() {
    let a = { let b = A {a: 3}; b };
    fail_unless!((a.a == 3));
    let c = { let d = V {v: 3}; d };
    fail_unless!((c.v == 3));
}
