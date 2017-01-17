// Copyright 2013-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! This module implements the `Any` trait, which enables dynamic typing
//! of any `'static` type through runtime reflection.
//!
//! `Any` itself can be used to get a `TypeId`, and has more features when used
//! as a trait object. As `&Any` (a borrowed trait object), it has the `is` and
//! `downcast_ref` methods, to test if the contained value is of a given type,
//! and to get a reference to the inner value as a type. As `&mut Any`, there
//! is also the `downcast_mut` method, for getting a mutable reference to the
//! inner value. `Box<Any>` adds the `downcast` method, which attempts to
//! convert to a `Box<T>`. See the [`Box`] documentation for the full details.
//!
//! Note that &Any is limited to testing whether a value is of a specified
//! concrete type, and cannot be used to test whether a type implements a trait.
//!
//! [`Box`]: ../../std/boxed/struct.Box.html
//!
//! # Examples
//!
//! Consider a situation where we want to log out a value passed to a function.
//! We know the value we're working on implements Debug, but we don't know its
//! concrete type.  We want to give special treatment to certain types: in this
//! case printing out the length of String values prior to their value.
//! We don't know the concrete type of our value at compile time, so we need to
//! use runtime reflection instead.
//!
//! ```rust
//! use std::fmt::Debug;
//! use std::any::Any;
//!
//! // Logger function for any type that implements Debug.
//! fn log<T: Any + Debug>(value: &T) {
//!     let value_any = value as &Any;
//!
//!     // try to convert our value to a String.  If successful, we want to
//!     // output the String's length as well as its value.  If not, it's a
//!     // different type: just print it out unadorned.
//!     match value_any.downcast_ref::<String>() {
//!         Some(as_string) => {
//!             println!("String ({}): {}", as_string.len(), as_string);
//!         }
//!         None => {
//!             println!("{:?}", value);
//!         }
//!     }
//! }
//!
//! // This function wants to log its parameter out prior to doing work with it.
//! fn do_work<T: Any + Debug>(value: &T) {
//!     log(value);
//!     // ...do some other work
//! }
//!
//! fn main() {
//!     let my_string = "Hello World".to_string();
//!     do_work(&my_string);
//!
//!     let my_i8: i8 = 100;
//!     do_work(&my_i8);
//! }
//! ```

#![stable(feature = "rust1", since = "1.0.0")]

use fmt;
use intrinsics;

///////////////////////////////////////////////////////////////////////////////
// Any trait
///////////////////////////////////////////////////////////////////////////////

/// A type to emulate dynamic typing.
///
/// Most types implement `Any`. However, any type which contains a non-`'static` reference does not.
/// See the [module-level documentation][mod] for more details.
///
/// [mod]: index.html
#[stable(feature = "rust1", since = "1.0.0")]
pub trait Any: 'static {
    /// Gets the `TypeId` of `self`.
    ///
    /// # Examples
    ///
    /// ```
    /// #![feature(get_type_id)]
    ///
    /// use std::any::{Any, TypeId};
    ///
    /// fn is_string(s: &Any) -> bool {
    ///     TypeId::of::<String>() == s.get_type_id()
    /// }
    ///
    /// fn main() {
    ///     assert_eq!(is_string(&0), false);
    ///     assert_eq!(is_string(&"cookie monster".to_owned()), true);
    /// }
    /// ```
    #[unstable(feature = "get_type_id",
               reason = "this method will likely be replaced by an associated static",
               issue = "27745")]
    fn get_type_id(&self) -> TypeId;
}

#[stable(feature = "rust1", since = "1.0.0")]
impl<T: 'static + ?Sized > Any for T {
    fn get_type_id(&self) -> TypeId { TypeId::of::<T>() }
}

///////////////////////////////////////////////////////////////////////////////
// Extension methods for Any trait objects.
///////////////////////////////////////////////////////////////////////////////

#[stable(feature = "rust1", since = "1.0.0")]
impl fmt::Debug for Any {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.pad("Any")
    }
}

// Ensure that the result of e.g. joining a thread can be printed and
// hence used with `unwrap`. May eventually no longer be needed if
// dispatch works with upcasting.
#[stable(feature = "rust1", since = "1.0.0")]
impl fmt::Debug for Any + Send {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.pad("Any")
    }
}

impl Any {
    /// Returns true if the boxed type is the same as `T`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::any::Any;
    ///
    /// fn is_string(s: &Any) {
    ///     if s.is::<String>() {
    ///         println!("It's a string!");
    ///     } else {
    ///         println!("Not a string...");
    ///     }
    /// }
    ///
    /// fn main() {
    ///     is_string(&0);
    ///     is_string(&"cookie monster".to_owned());
    /// }
    /// ```
    #[stable(feature = "rust1", since = "1.0.0")]
    #[inline]
    pub fn is<T: Any>(&self) -> bool {
        // Get TypeId of the type this function is instantiated with
        let t = TypeId::of::<T>();

        // Get TypeId of the type in the trait object
        let boxed = self.get_type_id();

        // Compare both TypeIds on equality
        t == boxed
    }

    /// Returns some reference to the boxed value if it is of type `T`, or
    /// `None` if it isn't.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::any::Any;
    ///
    /// fn print_if_string(s: &Any) {
    ///     if let Some(string) = s.downcast_ref::<String>() {
    ///         println!("It's a string({}): '{}'", string.len(), string);
    ///     } else {
    ///         println!("Not a string...");
    ///     }
    /// }
    ///
    /// fn main() {
    ///     print_if_string(&0);
    ///     print_if_string(&"cookie monster".to_owned());
    /// }
    /// ```
    #[stable(feature = "rust1", since = "1.0.0")]
    #[inline]
    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        if self.is::<T>() {
            unsafe {
                Some(&*(self as *const Any as *const T))
            }
        } else {
            None
        }
    }

    /// Returns some mutable reference to the boxed value if it is of type `T`, or
    /// `None` if it isn't.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::any::Any;
    ///
    /// fn modify_if_u32(s: &mut Any) {
    ///     if let Some(num) = s.downcast_mut::<u32>() {
    ///         *num = 42;
    ///     }
    /// }
    ///
    /// fn main() {
    ///     let mut x = 10u32;
    ///     let mut s = "starlord".to_owned();
    ///
    ///     modify_if_u32(&mut x);
    ///     modify_if_u32(&mut s);
    ///
    ///     assert_eq!(x, 42);
    ///     assert_eq!(&s, "starlord");
    /// }
    /// ```
    #[stable(feature = "rust1", since = "1.0.0")]
    #[inline]
    pub fn downcast_mut<T: Any>(&mut self) -> Option<&mut T> {
        if self.is::<T>() {
            unsafe {
                Some(&mut *(self as *mut Any as *mut T))
            }
        } else {
            None
        }
    }
}

impl Any+Send {
    /// Forwards to the method defined on the type `Any`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::any::Any;
    ///
    /// fn is_string(s: &(Any + Send)) {
    ///     if s.is::<String>() {
    ///         println!("It's a string!");
    ///     } else {
    ///         println!("Not a string...");
    ///     }
    /// }
    ///
    /// fn main() {
    ///     is_string(&0);
    ///     is_string(&"cookie monster".to_owned());
    /// }
    /// ```
    #[stable(feature = "rust1", since = "1.0.0")]
    #[inline]
    pub fn is<T: Any>(&self) -> bool {
        Any::is::<T>(self)
    }

    /// Forwards to the method defined on the type `Any`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::any::Any;
    ///
    /// fn print_if_string(s: &(Any + Send)) {
    ///     if let Some(string) = s.downcast_ref::<String>() {
    ///         println!("It's a string({}): '{}'", string.len(), string);
    ///     } else {
    ///         println!("Not a string...");
    ///     }
    /// }
    ///
    /// fn main() {
    ///     print_if_string(&0);
    ///     print_if_string(&"cookie monster".to_owned());
    /// }
    /// ```
    #[stable(feature = "rust1", since = "1.0.0")]
    #[inline]
    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        Any::downcast_ref::<T>(self)
    }

    /// Forwards to the method defined on the type `Any`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::any::Any;
    ///
    /// fn modify_if_u32(s: &mut (Any+ Send)) {
    ///     if let Some(num) = s.downcast_mut::<u32>() {
    ///         *num = 42;
    ///     }
    /// }
    ///
    /// fn main() {
    ///     let mut x = 10u32;
    ///     let mut s = "starlord".to_owned();
    ///
    ///     modify_if_u32(&mut x);
    ///     modify_if_u32(&mut s);
    ///
    ///     assert_eq!(x, 42);
    ///     assert_eq!(&s, "starlord");
    /// }
    /// ```
    #[stable(feature = "rust1", since = "1.0.0")]
    #[inline]
    pub fn downcast_mut<T: Any>(&mut self) -> Option<&mut T> {
        Any::downcast_mut::<T>(self)
    }
}


///////////////////////////////////////////////////////////////////////////////
// TypeID and its methods
///////////////////////////////////////////////////////////////////////////////

/// A `TypeId` represents a globally unique identifier for a type.
///
/// Each `TypeId` is an opaque object which does not allow inspection of what's
/// inside but does allow basic operations such as cloning, comparison,
/// printing, and showing.
///
/// A `TypeId` is currently only available for types which ascribe to `'static`,
/// but this limitation may be removed in the future.
///
/// While `TypeId` implements `Hash`, `PartialOrd`, and `Ord`, it is worth
/// noting that the hashes and ordering will vary between Rust releases. Beware
/// of relying on them outside of your code!
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
#[stable(feature = "rust1", since = "1.0.0")]
pub struct TypeId {
    t: u64,
}

impl TypeId {
    /// Returns the `TypeId` of the type this generic function has been
    /// instantiated with.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::any::{Any, TypeId};
    ///
    /// fn is_string<T: ?Sized + Any>(_s: &T) -> bool {
    ///     TypeId::of::<String>() == TypeId::of::<T>()
    /// }
    ///
    /// fn main() {
    ///     assert_eq!(is_string(&0), false);
    ///     assert_eq!(is_string(&"cookie monster".to_owned()), true);
    /// }
    /// ```
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn of<T: ?Sized + 'static>() -> TypeId {
        TypeId {
            t: unsafe { intrinsics::type_id::<T>() },
        }
    }
}
