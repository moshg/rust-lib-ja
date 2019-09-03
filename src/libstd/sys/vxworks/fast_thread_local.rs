// Copyright (c) 2019 Wind River Systems, Inc.

#![cfg(target_thread_local)]
#![unstable(feature = "thread_local_internals", issue = "0")]

pub unsafe fn register_dtor(t: *mut u8, dtor: unsafe extern fn(*mut u8)) {
    use crate::sys_common::thread_local::register_dtor_fallback;
    register_dtor_fallback(t, dtor);
}

pub fn requires_move_before_drop() -> bool {
    false
}
