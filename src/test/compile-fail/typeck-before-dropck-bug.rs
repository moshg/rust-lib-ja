// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

fn main() { }

// Before these errors would ICE as "cat_expr Errd" because the errors
// were unknown when the bug was triggered.

fn unconstrained_type() {
    [];
    //~^ ERROR cannot determine a type for this expression: unconstrained type
}

fn invalid_impl_within_scope() {
    {
        use std::ops::Deref;
        struct Something;

        impl Deref for Something {}
        //~^ ERROR not all trait items implemented, missing: `Target`, `deref`

        *Something
    };
}
