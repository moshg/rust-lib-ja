// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Language-level runtime services that should reasonably expected
//! to be available 'everywhere'. Local heaps, GC, unwinding,
//! local storage, and logging. Even a 'freestanding' Rust would likely want
//! to implement this.

//! Local services may exist in at least three different contexts:
//! when running as a task, when running in the scheduler's context,
//! or when running outside of a scheduler but with local services
//! (freestanding rust with local services?).

use prelude::*;
use libc::{c_void, uintptr_t};
use cast::transmute;
use super::sched::local_sched;
use super::local_heap::LocalHeap;
use rt::logging::StdErrLogger;

pub struct LocalServices {
    heap: LocalHeap,
    gc: GarbageCollector,
    storage: LocalStorage,
    logger: StdErrLogger,
    unwinder: Option<Unwinder>,
    destroyed: bool
}

pub struct GarbageCollector;
pub struct LocalStorage(*c_void, Option<~fn(*c_void)>);

pub struct Unwinder {
    unwinding: bool,
}

impl LocalServices {
    pub fn new() -> LocalServices {
        LocalServices {
            heap: LocalHeap::new(),
            gc: GarbageCollector,
            storage: LocalStorage(ptr::null(), None),
            logger: StdErrLogger,
            unwinder: Some(Unwinder { unwinding: false }),
            destroyed: false
        }
    }

    pub fn without_unwinding() -> LocalServices {
        LocalServices {
            heap: LocalHeap::new(),
            gc: GarbageCollector,
            storage: LocalStorage(ptr::null(), None),
            logger: StdErrLogger,
            unwinder: None,
            destroyed: false
        }
    }

    pub fn run(&mut self, f: &fn()) {
        // This is just an assertion that `run` was called unsafely
        // and this instance of LocalServices is still accessible.
        do borrow_local_services |sched| {
            assert!(ptr::ref_eq(sched, self));
        }

        match self.unwinder {
            Some(ref mut unwinder) => {
                // If there's an unwinder then set up the catch block
                unwinder.try(f);
            }
            None => {
                // Otherwise, just run the body
                f()
            }
        }
        self.destroy();
    }

    /// Must be called manually before finalization to clean up
    /// thread-local resources. Some of the routines here expect
    /// LocalServices to be available recursively so this must be
    /// called unsafely, without removing LocalServices from
    /// thread-local-storage.
    fn destroy(&mut self) {
        // This is just an assertion that `destroy` was called unsafely
        // and this instance of LocalServices is still accessible.
        do borrow_local_services |sched| {
            assert!(ptr::ref_eq(sched, self));
        }
        match self.storage {
            LocalStorage(ptr, Some(ref dtor)) => {
                (*dtor)(ptr)
            }
            _ => ()
        }
        self.destroyed = true;
    }
}

impl Drop for LocalServices {
    fn finalize(&self) { assert!(self.destroyed) }
}

// Just a sanity check to make sure we are catching a Rust-thrown exception
static UNWIND_TOKEN: uintptr_t = 839147;

impl Unwinder {
    pub fn try(&mut self, f: &fn()) {
        use sys::Closure;

        unsafe {
            let closure: Closure = transmute(f);
            let code = transmute(closure.code);
            let env = transmute(closure.env);

            let token = rust_try(try_fn, code, env);
            assert!(token == 0 || token == UNWIND_TOKEN);
        }

        extern fn try_fn(code: *c_void, env: *c_void) {
            unsafe {
                let closure: Closure = Closure {
                    code: transmute(code),
                    env: transmute(env),
                };
                let closure: &fn() = transmute(closure);
                closure();
            }
        }

        extern {
            #[rust_stack]
            fn rust_try(f: *u8, code: *c_void, data: *c_void) -> uintptr_t;
        }
    }

    pub fn begin_unwind(&mut self) -> ! {
        self.unwinding = true;
        unsafe {
            rust_begin_unwind(UNWIND_TOKEN);
            return transmute(());
        }
        extern {
            fn rust_begin_unwind(token: uintptr_t);
        }
    }
}

/// Borrow a pointer to the installed local services.
/// Fails (likely aborting the process) if local services are not available.
pub fn borrow_local_services(f: &fn(&mut LocalServices)) {
    do local_sched::borrow |sched| {
        match sched.current_task {
            Some(~ref mut task) => {
                f(&mut task.local_services)
            }
            None => {
                fail!(~"no local services for schedulers yet")
            }
        }
    }
}

pub unsafe fn unsafe_borrow_local_services() -> *mut LocalServices {
    match (*local_sched::unsafe_borrow()).current_task {
        Some(~ref mut task) => {
            let s: *mut LocalServices = &mut task.local_services;
            return s;
        }
        None => {
            // Don't fail. Infinite recursion
            abort!("no local services for schedulers yet")
        }
    }
}

pub unsafe fn unsafe_try_borrow_local_services() -> Option<*mut LocalServices> {
    if local_sched::exists() {
        Some(unsafe_borrow_local_services())
    } else {
        None
    }
}

#[cfg(test)]
mod test {
    use rt::test::*;

    #[test]
    fn local_heap() {
        do run_in_newsched_task() {
            let a = @5;
            let b = a;
            assert!(*a == 5);
            assert!(*b == 5);
        }
    }

    #[test]
    fn tls() {
        use local_data::*;
        do run_in_newsched_task() {
            unsafe {
                fn key(_x: @~str) { }
                local_data_set(key, @~"data");
                assert!(*local_data_get(key).get() == ~"data");
                fn key2(_x: @~str) { }
                local_data_set(key2, @~"data");
                assert!(*local_data_get(key2).get() == ~"data");
            }
        }
    }

    #[test]
    fn unwind() {
        do run_in_newsched_task() {
            let result = spawntask_try(||());
            assert!(result.is_ok());
            let result = spawntask_try(|| fail!());
            assert!(result.is_err());
        }
    }

    #[test]
    fn rng() {
        do run_in_newsched_task() {
            use rand::{rng, Rng};
            let mut r = rng();
            let _ = r.next();
        }
    }

    #[test]
    fn logging() {
        do run_in_newsched_task() {
            info!("here i am. logging in a newsched task");
        }
    }
}

