// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use zed::bar;
use zed::baz; //~ ERROR unresolved import `zed::baz` [E0432]
              //~^ no `baz` in `zed`. Did you mean to use `bar`?


mod zed {
    pub fn bar() { println!("bar"); }
    use foo; //~ ERROR unresolved import `foo` [E0432]
             //~^ no `foo` in the root
}

fn main() {
    zed::foo(); //~ ERROR unresolved name
    bar();
}
