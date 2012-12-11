// -*- rust -*-
// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.


// error-pattern:fail
extern mod std;
use comm::Port;
use comm::recv;

fn grandchild() { fail ~"grandchild dies"; }

fn child() {
    let p = Port::<int>();
    task::spawn(|| grandchild() );
    let x = recv(p);
}

fn main() {
    let p = Port::<int>();
    task::spawn(|| child() );
    let x = recv(p);
}
