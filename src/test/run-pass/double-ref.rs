// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

fn check_expr() {
    let _:         & uint =     &1u;
    let _:       & & uint =    &&1u;
    let _:     & & & uint =   &&&1u;
    let _:     & & & uint =  & &&1u;
    let _:   & & & & uint =  &&&&1u;
    let _:   & & & & uint = & &&&1u;
    let _: & & & & & uint = &&&&&1u;
}

fn check_ty() {
    let _:     &uint =         & 1u;
    let _:    &&uint =       & & 1u;
    let _:   &&&uint =     & & & 1u;
    let _:  & &&uint =     & & & 1u;
    let _:  &&&&uint =   & & & & 1u;
    let _: & &&&uint =   & & & & 1u;
    let _: &&&&&uint = & & & & & 1u;
}

fn check_pat() {
    let     &_ =         & 1u;
    let    &&_ =       & & 1u;
    let   &&&_ =     & & & 1u;
    let  & &&_ =     & & & 1u;
    let  &&&&_ =   & & & & 1u;
    let & &&&_ =   & & & & 1u;
    let &&&&&_ = & & & & & 1u;
}

pub fn main() {}
