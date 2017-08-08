// Copyright 2017 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Test that builtin implementations of `Clone` cleanup everything
// in case of unwinding.

use std::thread;
use std::sync::Arc;

struct S(Arc<()>);

impl Clone for S {
    fn clone(&self) -> Self {
        if Arc::strong_count(&self.0) == 7 {
            panic!("oops");
        }

        S(self.0.clone())
    }
}

fn main() {
    let counter = Arc::new(());

    // Unwinding with tuples...
    let ccounter = counter.clone();
    let child = thread::spawn(move || {
        let _ = (
            S(ccounter.clone()),
            S(ccounter.clone()),
            S(ccounter.clone()),
            S(ccounter)
        ).clone();
    });

    assert!(child.join().is_err());
    assert_eq!(
        1,
        Arc::strong_count(&counter)
    );

    // ... and with arrays.
    let ccounter = counter.clone();
    let child = thread::spawn(move || {
        let _ = [
            S(ccounter.clone()),
            S(ccounter.clone()),
            S(ccounter.clone()),
            S(ccounter)
        ].clone();
    });

    assert!(child.join().is_err());
    assert_eq!(
        1,
        Arc::strong_count(&counter)
    );
}
