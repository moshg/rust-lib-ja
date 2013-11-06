// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::libc;
use std::rt::io::IoError;
use std::rt::local::Local;
use std::rt::rtio::RtioTTY;
use std::rt::sched::{Scheduler, SchedHandle};

use stream::StreamWatcher;
use super::{Loop, UvError, UvHandle, uv_error_to_io_error};
use uvio::HomingIO;
use uvll;

pub struct TtyWatcher{
    tty: *uvll::uv_tty_t,
    stream: StreamWatcher,
    home: SchedHandle,
    fd: libc::c_int,
}

impl TtyWatcher {
    pub fn new(loop_: &Loop, fd: libc::c_int, readable: bool)
        -> Result<TtyWatcher, UvError>
    {
        let handle = UvHandle::alloc(None::<TtyWatcher>, uvll::UV_TTY);

        match unsafe {
            uvll::uv_tty_init(loop_.handle, handle, fd as libc::c_int,
                              readable as libc::c_int)
        } {
            0 => {
                Ok(TtyWatcher {
                    tty: handle,
                    stream: StreamWatcher::new(handle),
                    home: get_handle_to_current_scheduler!(),
                    fd: fd,
                })
            }
            n => {
                unsafe { uvll::free_handle(handle) }
                Err(UvError(n))
            }
        }
    }
}

impl RtioTTY for TtyWatcher {
    fn read(&mut self, buf: &mut [u8]) -> Result<uint, IoError> {
        let _m = self.fire_missiles();
        self.stream.read(buf).map_err(uv_error_to_io_error)
    }

    fn write(&mut self, buf: &[u8]) -> Result<(), IoError> {
        let _m = self.fire_missiles();
        self.stream.write(buf).map_err(uv_error_to_io_error)
    }

    fn set_raw(&mut self, raw: bool) -> Result<(), IoError> {
        let raw = raw as libc::c_int;
        let _m = self.fire_missiles();
        match unsafe { uvll::uv_tty_set_mode(self.tty, raw) } {
            0 => Ok(()),
            n => Err(uv_error_to_io_error(UvError(n)))
        }
    }

    #[allow(unused_mut)]
    fn get_winsize(&mut self) -> Result<(int, int), IoError> {
        let mut width: libc::c_int = 0;
        let mut height: libc::c_int = 0;
        let widthptr: *libc::c_int = &width;
        let heightptr: *libc::c_int = &width;

        let _m = self.fire_missiles();
        match unsafe { uvll::uv_tty_get_winsize(self.tty,
                                                widthptr, heightptr) } {
            0 => Ok((width as int, height as int)),
            n => Err(uv_error_to_io_error(UvError(n)))
        }
    }

    fn isatty(&self) -> bool {
        unsafe { uvll::uv_guess_handle(self.fd) == uvll::UV_TTY }
    }
}

impl UvHandle<uvll::uv_tty_t> for TtyWatcher {
    fn uv_handle(&self) -> *uvll::uv_tty_t { self.tty }
}

impl HomingIO for TtyWatcher {
    fn home<'a>(&'a mut self) -> &'a mut SchedHandle { &mut self.home }
}

impl Drop for TtyWatcher {
    fn drop(&mut self) {
        let _m = self.fire_missiles();
        self.stream.close();
    }
}
