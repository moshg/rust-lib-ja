// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// aux-build:typeck-default-trait-impl-cross-crate-coherence-lib.rs

// Test that we do not consider associated types to be sendable without
// some applicable trait bound (and we don't ICE).

#![feature(optin_builtin_traits)]

extern crate "typeck-default-trait-impl-cross-crate-coherence-lib" as lib;

use lib::DefaultedTrait;

struct A;
impl DefaultedTrait for (A,) { }
//~^ ERROR can only be implemented for a struct or enum type

struct B;
impl !DefaultedTrait for (B,) { }
//~^ ERROR can only be implemented for a struct or enum type

struct C;
impl DefaultedTrait for Box<C> { }
//~^ ERROR can only be implemented for a struct or enum type

fn main() { }
