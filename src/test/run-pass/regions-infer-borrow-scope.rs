// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

type point = {x: int, y: int};

fn x_coord(p: &r/point) -> &r/int {
    return &p.x;
}

fn main() {
    let p = @{x: 3, y: 4};
    let xc = x_coord(p);
    assert *xc == 3;
}

