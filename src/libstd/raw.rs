// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![allow(missing_docs)]
#![unstable(feature = "raw", issue = "27751")]

//! Contains struct definitions for the layout of compiler built-in types.
//!
//! They can be used as targets of transmutes in unsafe code for manipulating
//! the raw representations directly.
//!
//! Their definition should always match the ABI defined in `rustc::back::abi`.

use core::cell::Cell;
use core::future::Future;
use core::marker::Unpin;
use core::mem::PinMut;
use core::option::Option;
use core::ptr::NonNull;
use core::task::{self, Poll};
use core::ops::{Drop, Generator, GeneratorState};

#[stable(feature = "rust1", since = "1.0.0")]
pub use core::raw::*;

/// Wrap a future in a generator.
///
/// This function returns a `GenFuture` underneath, but hides it in `impl Trait` to give
/// better error messages (`impl Future` rather than `GenFuture<[closure.....]>`).
#[unstable(feature = "gen_future", issue = "50547")]
pub fn future_from_generator<T: Generator<Yield = ()>>(x: T) -> impl Future<Output = T::Return> {
    GenFuture(x)
}

/// A wrapper around generators used to implement `Future` for `async`/`await` code.
#[unstable(feature = "gen_future", issue = "50547")]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
struct GenFuture<T: Generator<Yield = ()>>(T);

// We rely on the fact that async/await futures are immovable in order to create
// self-referential borrows in the underlying generator.
impl<T: Generator<Yield = ()>> !Unpin for GenFuture<T> {}

#[unstable(feature = "gen_future", issue = "50547")]
impl<T: Generator<Yield = ()>> Future for GenFuture<T> {
    type Output = T::Return;
    fn poll(self: PinMut<Self>, cx: &mut task::Context) -> Poll<Self::Output> {
        with_set_cx(cx, || match unsafe { PinMut::get_mut(self).0.resume() } {
            GeneratorState::Yielded(()) => Poll::Pending,
            GeneratorState::Complete(x) => Poll::Ready(x),
        })
    }
}

thread_local! {
    static TLS_CX: Cell<Option<NonNull<task::Context<'static>>>> = Cell::new(None);
}

struct SetOnDrop(Option<NonNull<task::Context<'static>>>);

impl Drop for SetOnDrop {
    fn drop(&mut self) {
        TLS_CX.with(|tls_cx| {
            tls_cx.set(self.0.take());
        });
    }
}

#[unstable(feature = "gen_future", issue = "50547")]
pub fn with_set_cx<F, R>(cx: &mut task::Context, f: F) -> R
where
    F: FnOnce() -> R
{
    let old_cx = TLS_CX.with(|tls_cx| {
        let old_cx = tls_cx.get();
        tls_cx.set(NonNull::new(
                cx as *mut task::Context as *mut () as *mut task::Context<'static>));
        old_cx
    });
    let _reset_cx = SetOnDrop(old_cx);
    let res = f();
    res
}

#[unstable(feature = "gen_future", issue = "50547")]
pub fn with_get_cx<F, R>(f: F) -> R
where
    F: FnOnce(&mut task::Context) -> R
{
    let cx_ptr = TLS_CX.with(|tls_cx| {
        let cx_ptr = tls_cx.get();
        // Clear the entry so that nested `with_get_cx` calls
        // will fail or set their own value.
        tls_cx.set(None);
        cx_ptr
    });
    let _reset_cx = SetOnDrop(cx_ptr);

    let mut cx_ptr = cx_ptr.expect(
        "TLS task::Context not set. This is a rustc bug. \
        Please file an issue on https://github.com/rust-lang/rust.");
    unsafe { f(cx_ptr.as_mut()) }
}
