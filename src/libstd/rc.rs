// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*! Task-local reference counted boxes

The `Rc` type provides shared ownership of an immutable value. Destruction is deterministic, and
will occur as soon as the last owner is gone. It is marked as non-sendable because it avoids the
overhead of atomic reference counting.

*/

use ptr::RawPtr;
use unstable::intrinsics::transmute;
use ops::Drop;
use kinds::{Freeze, Send};
use clone::{Clone, DeepClone};

struct RcBox<T> {
    value: T,
    count: uint
}

/// Immutable reference counted pointer type
#[unsafe_no_drop_flag]
#[no_send]
pub struct Rc<T> {
    priv ptr: *mut RcBox<T>
}

impl<T: Freeze> Rc<T> {
    /// Construct a new reference-counted box from a `Freeze` value
    #[inline]
    pub fn new(value: T) -> Rc<T> {
        unsafe {
            Rc::new_unchecked(value)
        }
    }
}

impl<T: Send> Rc<T> {
    /// Construct a new reference-counted box from a `Send` value
    #[inline]
    pub fn from_send(value: T) -> Rc<T> {
        unsafe {
            Rc::new_unchecked(value)
        }
    }
}

impl<T> Rc<T> {
    /// Unsafety construct a new reference-counted box from any value.
    ///
    /// It is possible to create cycles, which will leak, and may interact
    /// poorly with managed pointers.
    #[inline]
    pub unsafe fn new_unchecked(value: T) -> Rc<T> {
        Rc{ptr: transmute(~RcBox{value: value, count: 1})}
    }

    /// Borrow the value contained in the reference-counted box
    #[inline]
    pub fn borrow<'r>(&'r self) -> &'r T {
        unsafe { &(*self.ptr).value }
    }
}

impl<T> Clone for Rc<T> {
    #[inline]
    fn clone(&self) -> Rc<T> {
        unsafe {
            (*self.ptr).count += 1;
            Rc{ptr: self.ptr}
        }
    }
}

impl<T: DeepClone> DeepClone for Rc<T> {
    #[inline]
    fn deep_clone(&self) -> Rc<T> {
        unsafe { Rc::new_unchecked(self.borrow().deep_clone()) }
    }
}

#[unsafe_destructor]
impl<T> Drop for Rc<T> {
    fn drop(&mut self) {
        unsafe {
            if self.ptr.is_not_null() {
                (*self.ptr).count -= 1;
                if (*self.ptr).count == 0 {
                    let _: ~RcBox<T> = transmute(self.ptr);
                }
            }
        }
    }
}

#[cfg(test)]
mod test_rc {
    use super::*;
    use cell::Cell;

    #[test]
    fn test_clone() {
        let x = Rc::from_send(Cell::new(5));
        let y = x.clone();
        do x.borrow().with_mut_ref |inner| {
            *inner = 20;
        }
        assert_eq!(y.borrow().take(), 20);
    }

    #[test]
    fn test_deep_clone() {
        let x = Rc::from_send(Cell::new(5));
        let y = x.deep_clone();
        do x.borrow().with_mut_ref |inner| {
            *inner = 20;
        }
        assert_eq!(y.borrow().take(), 5);
    }

    #[test]
    fn test_simple() {
        let x = Rc::new(5);
        assert_eq!(*x.borrow(), 5);
    }

    #[test]
    fn test_simple_clone() {
        let x = Rc::new(5);
        let y = x.clone();
        assert_eq!(*x.borrow(), 5);
        assert_eq!(*y.borrow(), 5);
    }

    #[test]
    fn test_destructor() {
        let x = Rc::from_send(~5);
        assert_eq!(**x.borrow(), 5);
    }
}
