// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.


fn reverse(v: &mut [uint]) {
    v.reverse();
}

fn bar(v: &mut [uint]) {
    reverse(v);
    reverse(v);
    reverse(v);
}

pub fn main() {
    let mut the_vec = vec!(1, 2, 3, 100);
    bar(&mut the_vec);
    assert_eq!(the_vec, vec!(100, 3, 2, 1));
}
