// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![allow(non_snake_case)]

register_long_diagnostics! {

E0445: r##"
A private trait was used on a "public" type. Erroneous code example:

```
trait Foo {
    fn dummy(&self) { }
}

pub trait Bar : Foo {} // error: private trait in exported type parameter bound
```

To solve this error, please ensure the trait is accessible at the same level of
the type(s) on which it's implemented. Example:

```
pub trait Foo { // we set the Foo trait public
    fn dummy(&self) { }
}

pub trait Bar : Foo {} // ok!
```
"##,

}