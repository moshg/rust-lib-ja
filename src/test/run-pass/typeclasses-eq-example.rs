// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.


// Example from lkuper's intern talk, August 2012.

trait Equal {
    fn isEq(&self, a: &Self) -> bool;
}

enum Color { cyan, magenta, yellow, black }

impl Equal for Color {
    fn isEq(&self, a: &Color) -> bool {
        match (*self, *a) {
          (cyan, cyan)       => { true  }
          (magenta, magenta) => { true  }
          (yellow, yellow)   => { true  }
          (black, black)     => { true  }
          _                  => { false }
        }
    }
}

enum ColorTree {
    leaf(Color),
    branch(Box<ColorTree>, Box<ColorTree>)
}

impl Equal for ColorTree {
    fn isEq(&self, a: &ColorTree) -> bool {
        match (self, a) {
          (&leaf(x), &leaf(y)) => { x.isEq(&y) }
          (&branch(ref l1, ref r1), &branch(ref l2, ref r2)) => {
            (&**l1).isEq(&**l2) && (&**r1).isEq(&**r2)
          }
          _ => { false }
        }
    }
}

pub fn main() {
    assert!(cyan.isEq(&cyan));
    assert!(magenta.isEq(&magenta));
    assert!(!cyan.isEq(&yellow));
    assert!(!magenta.isEq(&cyan));

    assert!(leaf(cyan).isEq(&leaf(cyan)));
    assert!(!leaf(cyan).isEq(&leaf(yellow)));

    assert!(branch(box leaf(magenta), box leaf(cyan))
        .isEq(&branch(box leaf(magenta), box leaf(cyan))));

    assert!(!branch(box leaf(magenta), box leaf(cyan))
        .isEq(&branch(box leaf(magenta), box leaf(magenta))));

    println!("Assertions all succeeded!");
}
