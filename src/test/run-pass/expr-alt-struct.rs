// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.




// -*- rust -*-

// Tests for match as expressions resulting in structural types
fn test_rec() {
    let rs = match true { true => {i: 100}, _ => fail };
    assert (rs.i == 100);
}

enum mood { happy, sad, }

impl mood : cmp::Eq {
    pure fn eq(&self, other: &mood) -> bool {
        ((*self) as uint) == ((*other) as uint)
    }
    pure fn ne(&self, other: &mood) -> bool { !(*self).eq(other) }
}

fn test_tag() {
    let rs = match true { true => { happy } false => { sad } };
    assert (rs == happy);
}

fn main() { test_rec(); test_tag(); }
