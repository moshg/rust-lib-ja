// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

extern crate collections;

use std::str::{SendStr, Owned, Slice};
use std::to_string::ToString;
use self::collections::TreeMap;
use std::option::Some;

pub fn main() {
    let mut map: TreeMap<SendStr, uint> = TreeMap::new();
    assert!(map.insert(Slice("foo"), 42).is_none());
    assert!(map.insert(Owned("foo".to_string()), 42).is_some());
    assert!(map.insert(Slice("foo"), 42).is_some());
    assert!(map.insert(Owned("foo".to_string()), 42).is_some());

    assert!(map.insert(Slice("foo"), 43).is_some());
    assert!(map.insert(Owned("foo".to_string()), 44).is_some());
    assert!(map.insert(Slice("foo"), 45).is_some());
    assert!(map.insert(Owned("foo".to_string()), 46).is_some());

    let v = 46;

    assert_eq!(map.get(&Owned("foo".to_string())), Some(&v));
    assert_eq!(map.get(&Slice("foo")), Some(&v));

    let (a, b, c, d) = (50, 51, 52, 53);

    assert!(map.insert(Slice("abc"), a).is_none());
    assert!(map.insert(Owned("bcd".to_string()), b).is_none());
    assert!(map.insert(Slice("cde"), c).is_none());
    assert!(map.insert(Owned("def".to_string()), d).is_none());

    assert!(map.insert(Slice("abc"), a).is_some());
    assert!(map.insert(Owned("bcd".to_string()), b).is_some());
    assert!(map.insert(Slice("cde"), c).is_some());
    assert!(map.insert(Owned("def".to_string()), d).is_some());

    assert!(map.insert(Owned("abc".to_string()), a).is_some());
    assert!(map.insert(Slice("bcd"), b).is_some());
    assert!(map.insert(Owned("cde".to_string()), c).is_some());
    assert!(map.insert(Slice("def"), d).is_some());

    assert_eq!(map.get(&Slice("abc")), Some(&a));
    assert_eq!(map.get(&Slice("bcd")), Some(&b));
    assert_eq!(map.get(&Slice("cde")), Some(&c));
    assert_eq!(map.get(&Slice("def")), Some(&d));

    assert_eq!(map.get(&Owned("abc".to_string())), Some(&a));
    assert_eq!(map.get(&Owned("bcd".to_string())), Some(&b));
    assert_eq!(map.get(&Owned("cde".to_string())), Some(&c));
    assert_eq!(map.get(&Owned("def".to_string())), Some(&d));

    assert!(map.remove(&Slice("foo")).is_some());
    assert_eq!(map.into_iter().map(|(k, v)| format!("{}{}", k, v))
                              .collect::<Vec<String>>()
                              .concat(),
               "abc50bcd51cde52def53".to_string());
}
