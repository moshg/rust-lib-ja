// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*!
 * A simple map based on a vector for small integer keys. Space requirements
 * are O(highest integer key).
 */

#[allow(missing_doc)];

use std::iterator::{Iterator, IteratorUtil, Enumerate, FilterMap, Invert};
use std::uint;
use std::util::replace;
use std::vec::{VecIterator, VecMutIterator};
use std::vec;

#[allow(missing_doc)]
pub struct SmallIntMap<T> {
    priv v: ~[Option<T>],
}

impl<V> Container for SmallIntMap<V> {
    /// Return the number of elements in the map
    fn len(&self) -> uint {
        let mut sz = 0;
        foreach i in range(0u, self.v.len()) {
            match self.v[i] {
                Some(_) => sz += 1,
                None => {}
            }
        }
        sz
    }
}

impl<V> Mutable for SmallIntMap<V> {
    /// Clear the map, removing all key-value pairs.
    fn clear(&mut self) { self.v.clear() }
}

impl<V> Map<uint, V> for SmallIntMap<V> {
    /// Return true if the map contains a value for the specified key
    fn contains_key(&self, key: &uint) -> bool {
        self.find(key).is_some()
    }

    /// Return a reference to the value corresponding to the key
    fn find<'a>(&'a self, key: &uint) -> Option<&'a V> {
        if *key < self.v.len() {
            match self.v[*key] {
              Some(ref value) => Some(value),
              None => None
            }
        } else {
            None
        }
    }
}

impl<V> MutableMap<uint, V> for SmallIntMap<V> {
    /// Return a mutable reference to the value corresponding to the key
    fn find_mut<'a>(&'a mut self, key: &uint) -> Option<&'a mut V> {
        if *key < self.v.len() {
            match self.v[*key] {
              Some(ref mut value) => Some(value),
              None => None
            }
        } else {
            None
        }
    }

    /// Insert a key-value pair into the map. An existing value for a
    /// key is replaced by the new value. Return true if the key did
    /// not already exist in the map.
    fn insert(&mut self, key: uint, value: V) -> bool {
        let exists = self.contains_key(&key);
        let len = self.v.len();
        if len <= key {
            self.v.grow_fn(key - len + 1, |_| None);
        }
        self.v[key] = Some(value);
        !exists
    }

    /// Remove a key-value pair from the map. Return true if the key
    /// was present in the map, otherwise false.
    fn remove(&mut self, key: &uint) -> bool {
        self.pop(key).is_some()
    }

    /// Insert a key-value pair from the map. If the key already had a value
    /// present in the map, that value is returned. Otherwise None is returned.
    fn swap(&mut self, key: uint, value: V) -> Option<V> {
        match self.find_mut(&key) {
            Some(loc) => { return Some(replace(loc, value)); }
            None => ()
        }
        self.insert(key, value);
        return None;
    }

    /// Removes a key from the map, returning the value at the key if the key
    /// was previously in the map.
    fn pop(&mut self, key: &uint) -> Option<V> {
        if *key >= self.v.len() {
            return None;
        }
        self.v[*key].take()
    }
}

impl<V> SmallIntMap<V> {
    /// Create an empty SmallIntMap
    pub fn new() -> SmallIntMap<V> { SmallIntMap{v: ~[]} }

    /// Visit all key-value pairs in order
    pub fn each<'a>(&'a self, it: &fn(&uint, &'a V) -> bool) -> bool {
        foreach i in range(0u, self.v.len()) {
            match self.v[i] {
              Some(ref elt) => if !it(&i, elt) { return false; },
              None => ()
            }
        }
        true
    }

    /// Visit all keys in order
    pub fn each_key(&self, blk: &fn(key: &uint) -> bool) -> bool {
        self.each(|k, _| blk(k))
    }

    /// Visit all values in order
    pub fn each_value<'a>(&'a self, blk: &fn(value: &'a V) -> bool) -> bool {
        self.each(|_, v| blk(v))
    }

    /// Iterate over the map and mutate the contained values
    pub fn mutate_values(&mut self, it: &fn(&uint, &mut V) -> bool) -> bool {
        foreach i in range(0, self.v.len()) {
            match self.v[i] {
              Some(ref mut elt) => if !it(&i, elt) { return false; },
              None => ()
            }
        }
        true
    }

    /// Visit all key-value pairs in reverse order
    pub fn each_reverse<'a>(&'a self, it: &fn(uint, &'a V) -> bool) -> bool {
        do uint::range_rev(self.v.len(), 0) |i| {
            match self.v[i] {
              Some(ref elt) => it(i, elt),
              None => true
            }
        }
    }

    pub fn get<'a>(&'a self, key: &uint) -> &'a V {
        self.find(key).expect("key not present")
    }

    /// An iterator visiting all key-value pairs in ascending order by the keys.
    /// Iterator element type is (uint, &'r V)
    pub fn iter<'r>(&'r self) -> SmallIntMapIterator<'r, V> {
        SmallIntMapIterator {
            front: 0,
            back: self.v.len(),
            iter: self.v.iter()
        }
    }

    /// An iterator visiting all key-value pairs in ascending order by the keys,
    /// with mutable references to the values
    /// Iterator element type is (uint, &'r mut V)
    pub fn mut_iter<'r>(&'r mut self) -> SmallIntMapMutIterator<'r, V> {
        SmallIntMapMutIterator {
            front: 0,
            back: self.v.len(),
            iter: self.v.mut_iter()
        }
    }

    /// An iterator visiting all key-value pairs in descending order by the keys.
    /// Iterator element type is (uint, &'r V)
    pub fn rev_iter<'r>(&'r self) -> SmallIntMapRevIterator<'r, V> {
        self.iter().invert()
    }

    /// An iterator visiting all key-value pairs in descending order by the keys,
    /// with mutable references to the values
    /// Iterator element type is (uint, &'r mut V)
    pub fn mut_rev_iter<'r>(&'r mut self) -> SmallIntMapMutRevIterator <'r, V> {
        self.mut_iter().invert()
    }

    /// Empties the hash map, moving all values into the specified closure
    pub fn consume(&mut self)
        -> FilterMap<(uint, Option<V>), (uint, V),
                Enumerate<vec::ConsumeIterator<Option<V>>>>
    {
        let values = replace(&mut self.v, ~[]);
        values.consume_iter().enumerate().filter_map(|(i, v)| {
            v.map_consume(|v| (i, v))
        })
    }
}

impl<V:Clone> SmallIntMap<V> {
    pub fn update_with_key(&mut self, key: uint, val: V,
                           ff: &fn(uint, V, V) -> V) -> bool {
        let new_val = match self.find(&key) {
            None => val,
            Some(orig) => ff(key, (*orig).clone(), val)
        };
        self.insert(key, new_val)
    }

    pub fn update(&mut self, key: uint, newval: V, ff: &fn(V, V) -> V)
                  -> bool {
        self.update_with_key(key, newval, |_k, v, v1| ff(v,v1))
    }
}


macro_rules! iterator {
    (impl $name:ident -> $elem:ty, $getter:ident) => {
        impl<'self, T> Iterator<$elem> for $name<'self, T> {
            #[inline]
            fn next(&mut self) -> Option<$elem> {
                while self.front < self.back {
                    match self.iter.next() {
                        Some(elem) => {
                            if elem.is_some() {
                                let index = self.front;
                                self.front += 1;
                                return Some((index, elem. $getter ()));
                            }
                        }
                        _ => ()
                    }
                    self.front += 1;
                }
                None
            }

            #[inline]
            fn size_hint(&self) -> (uint, Option<uint>) {
                (0, Some(self.back - self.front))
            }
        }
    }
}

macro_rules! double_ended_iterator {
    (impl $name:ident -> $elem:ty, $getter:ident) => {
        impl<'self, T> DoubleEndedIterator<$elem> for $name<'self, T> {
            #[inline]
            fn next_back(&mut self) -> Option<$elem> {
                while self.front < self.back {
                    match self.iter.next_back() {
                        Some(elem) => {
                            if elem.is_some() {
                                self.back -= 1;
                                return Some((self.back, elem. $getter ()));
                            }
                        }
                        _ => ()
                    }
                    self.back -= 1;
                }
                None
            }
        }
    }
}

pub struct SmallIntMapIterator<'self, T> {
    priv front: uint,
    priv back: uint,
    priv iter: VecIterator<'self, Option<T>>
}

iterator!(impl SmallIntMapIterator -> (uint, &'self T), get_ref)
double_ended_iterator!(impl SmallIntMapIterator -> (uint, &'self T), get_ref)
pub type SmallIntMapRevIterator<'self, T> = Invert<SmallIntMapIterator<'self, T>>;

pub struct SmallIntMapMutIterator<'self, T> {
    priv front: uint,
    priv back: uint,
    priv iter: VecMutIterator<'self, Option<T>>
}

iterator!(impl SmallIntMapMutIterator -> (uint, &'self mut T), get_mut_ref)
double_ended_iterator!(impl SmallIntMapMutIterator -> (uint, &'self mut T), get_mut_ref)
pub type SmallIntMapMutRevIterator<'self, T> = Invert<SmallIntMapMutIterator<'self, T>>;

#[cfg(test)]
mod test_map {

    use super::SmallIntMap;

    #[test]
    fn test_find_mut() {
        let mut m = SmallIntMap::new();
        assert!(m.insert(1, 12));
        assert!(m.insert(2, 8));
        assert!(m.insert(5, 14));
        let new = 100;
        match m.find_mut(&5) {
            None => fail!(), Some(x) => *x = new
        }
        assert_eq!(m.find(&5), Some(&new));
    }

    #[test]
    fn test_len() {
        let mut map = SmallIntMap::new();
        assert_eq!(map.len(), 0);
        assert!(map.is_empty());
        assert!(map.insert(5, 20));
        assert_eq!(map.len(), 1);
        assert!(!map.is_empty());
        assert!(map.insert(11, 12));
        assert_eq!(map.len(), 2);
        assert!(!map.is_empty());
        assert!(map.insert(14, 22));
        assert_eq!(map.len(), 3);
        assert!(!map.is_empty());
    }

    #[test]
    fn test_clear() {
        let mut map = SmallIntMap::new();
        assert!(map.insert(5, 20));
        assert!(map.insert(11, 12));
        assert!(map.insert(14, 22));
        map.clear();
        assert!(map.is_empty());
        assert!(map.find(&5).is_none());
        assert!(map.find(&11).is_none());
        assert!(map.find(&14).is_none());
    }

    #[test]
    fn test_insert_with_key() {
        let mut map = SmallIntMap::new();

        // given a new key, initialize it with this new count, given
        // given an existing key, add more to its count
        fn addMoreToCount(_k: uint, v0: uint, v1: uint) -> uint {
            v0 + v1
        }

        fn addMoreToCount_simple(v0: uint, v1: uint) -> uint {
            v0 + v1
        }

        // count integers
        map.update(3, 1, addMoreToCount_simple);
        map.update_with_key(9, 1, addMoreToCount);
        map.update(3, 7, addMoreToCount_simple);
        map.update_with_key(5, 3, addMoreToCount);
        map.update_with_key(3, 2, addMoreToCount);

        // check the total counts
        assert_eq!(map.find(&3).get(), &10);
        assert_eq!(map.find(&5).get(), &3);
        assert_eq!(map.find(&9).get(), &1);

        // sadly, no sevens were counted
        assert!(map.find(&7).is_none());
    }

    #[test]
    fn test_swap() {
        let mut m = SmallIntMap::new();
        assert_eq!(m.swap(1, 2), None);
        assert_eq!(m.swap(1, 3), Some(2));
        assert_eq!(m.swap(1, 4), Some(3));
    }

    #[test]
    fn test_pop() {
        let mut m = SmallIntMap::new();
        m.insert(1, 2);
        assert_eq!(m.pop(&1), Some(2));
        assert_eq!(m.pop(&1), None);
    }

    #[test]
    fn test_iterator() {
        let mut m = SmallIntMap::new();

        assert!(m.insert(0, 1));
        assert!(m.insert(1, 2));
        assert!(m.insert(3, 5));
        assert!(m.insert(6, 10));
        assert!(m.insert(10, 11));

        let mut it = m.iter();
        assert_eq!(it.size_hint(), (0, Some(11)));
        assert_eq!(it.next().unwrap(), (0, &1));
        assert_eq!(it.size_hint(), (0, Some(10)));
        assert_eq!(it.next().unwrap(), (1, &2));
        assert_eq!(it.size_hint(), (0, Some(9)));
        assert_eq!(it.next().unwrap(), (3, &5));
        assert_eq!(it.size_hint(), (0, Some(7)));
        assert_eq!(it.next().unwrap(), (6, &10));
        assert_eq!(it.size_hint(), (0, Some(4)));
        assert_eq!(it.next().unwrap(), (10, &11));
        assert_eq!(it.size_hint(), (0, Some(0)));
        assert!(it.next().is_none());
    }

    #[test]
    fn test_iterator_size_hints() {
        let mut m = SmallIntMap::new();

        assert!(m.insert(0, 1));
        assert!(m.insert(1, 2));
        assert!(m.insert(3, 5));
        assert!(m.insert(6, 10));
        assert!(m.insert(10, 11));

        assert_eq!(m.iter().size_hint(), (0, Some(11)));
        assert_eq!(m.rev_iter().size_hint(), (0, Some(11)));
        assert_eq!(m.mut_iter().size_hint(), (0, Some(11)));
        assert_eq!(m.mut_rev_iter().size_hint(), (0, Some(11)));
    }

    #[test]
    fn test_mut_iterator() {
        let mut m = SmallIntMap::new();

        assert!(m.insert(0, 1));
        assert!(m.insert(1, 2));
        assert!(m.insert(3, 5));
        assert!(m.insert(6, 10));
        assert!(m.insert(10, 11));

        foreach (k, v) in m.mut_iter() {
            *v += k as int;
        }

        let mut it = m.iter();
        assert_eq!(it.next().unwrap(), (0, &1));
        assert_eq!(it.next().unwrap(), (1, &3));
        assert_eq!(it.next().unwrap(), (3, &8));
        assert_eq!(it.next().unwrap(), (6, &16));
        assert_eq!(it.next().unwrap(), (10, &21));
        assert!(it.next().is_none());
    }

    #[test]
    fn test_rev_iterator() {
        let mut m = SmallIntMap::new();

        assert!(m.insert(0, 1));
        assert!(m.insert(1, 2));
        assert!(m.insert(3, 5));
        assert!(m.insert(6, 10));
        assert!(m.insert(10, 11));

        let mut it = m.rev_iter();
        assert_eq!(it.next().unwrap(), (10, &11));
        assert_eq!(it.next().unwrap(), (6, &10));
        assert_eq!(it.next().unwrap(), (3, &5));
        assert_eq!(it.next().unwrap(), (1, &2));
        assert_eq!(it.next().unwrap(), (0, &1));
        assert!(it.next().is_none());
    }

    #[test]
    fn test_mut_rev_iterator() {
        let mut m = SmallIntMap::new();

        assert!(m.insert(0, 1));
        assert!(m.insert(1, 2));
        assert!(m.insert(3, 5));
        assert!(m.insert(6, 10));
        assert!(m.insert(10, 11));

        foreach (k, v) in m.mut_rev_iter() {
            *v += k as int;
        }

        let mut it = m.iter();
        assert_eq!(it.next().unwrap(), (0, &1));
        assert_eq!(it.next().unwrap(), (1, &3));
        assert_eq!(it.next().unwrap(), (3, &8));
        assert_eq!(it.next().unwrap(), (6, &16));
        assert_eq!(it.next().unwrap(), (10, &21));
        assert!(it.next().is_none());
    }

    #[test]
    fn test_consume() {
        let mut m = SmallIntMap::new();
        m.insert(1, ~2);
        let mut called = false;
        foreach (k, v) in m.consume() {
            assert!(!called);
            called = true;
            assert_eq!(k, 1);
            assert_eq!(v, ~2);
        }
        assert!(called);
        m.insert(2, ~1);
    }
}

#[cfg(test)]
mod bench {

    use super::*;
    use test::BenchHarness;
    use container::bench::*;

    // Find seq
    #[bench]
    pub fn insert_rand_100(bh: &mut BenchHarness) {
        let mut m : SmallIntMap<uint> = SmallIntMap::new();
        insert_rand_n(100, &mut m, bh);
    }

    #[bench]
    pub fn insert_rand_10_000(bh: &mut BenchHarness) {
        let mut m : SmallIntMap<uint> = SmallIntMap::new();
        insert_rand_n(10_000, &mut m, bh);
    }

    // Insert seq
    #[bench]
    pub fn insert_seq_100(bh: &mut BenchHarness) {
        let mut m : SmallIntMap<uint> = SmallIntMap::new();
        insert_seq_n(100, &mut m, bh);
    }

    #[bench]
    pub fn insert_seq_10_000(bh: &mut BenchHarness) {
        let mut m : SmallIntMap<uint> = SmallIntMap::new();
        insert_seq_n(10_000, &mut m, bh);
    }

    // Find rand
    #[bench]
    pub fn find_rand_100(bh: &mut BenchHarness) {
        let mut m : SmallIntMap<uint> = SmallIntMap::new();
        find_rand_n(100, &mut m, bh);
    }

    #[bench]
    pub fn find_rand_10_000(bh: &mut BenchHarness) {
        let mut m : SmallIntMap<uint> = SmallIntMap::new();
        find_rand_n(10_000, &mut m, bh);
    }

    // Find seq
    #[bench]
    pub fn find_seq_100(bh: &mut BenchHarness) {
        let mut m : SmallIntMap<uint> = SmallIntMap::new();
        find_seq_n(100, &mut m, bh);
    }

    #[bench]
    pub fn find_seq_10_000(bh: &mut BenchHarness) {
        let mut m : SmallIntMap<uint> = SmallIntMap::new();
        find_seq_n(10_000, &mut m, bh);
    }
}
