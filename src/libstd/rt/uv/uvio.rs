// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use c_str::ToCStr;
use cast::transmute;
use cast;
use cell::Cell;
use clone::Clone;
use libc::{c_int, c_uint, c_void};
use ops::Drop;
use option::*;
use ptr;
use result::*;
use rt::io::IoError;
use rt::io::net::ip::{SocketAddr, IpAddr};
use rt::io::{standard_error, OtherIoError};
use rt::local::Local;
use rt::rtio::*;
use rt::sched::{Scheduler, SchedHandle};
use rt::tube::Tube;
use rt::uv::*;
use rt::uv::idle::IdleWatcher;
use rt::uv::net::{UvIpv4SocketAddr, UvIpv6SocketAddr};
use unstable::sync::Exclusive;

#[cfg(test)] use container::Container;
#[cfg(test)] use unstable::run_in_bare_thread;
#[cfg(test)] use rt::test::{spawntask,
                            next_test_ip4,
                            run_in_newsched_task};
#[cfg(test)] use iterator::{Iterator, range};

// XXX we should not be calling uvll functions in here.

trait HomingIO {
    fn home<'r>(&'r mut self) -> &'r mut SchedHandle;
    /* XXX This will move pinned tasks to do IO on the proper scheduler
     * and then move them back to their home.
     */
    fn home_for_io<A>(&mut self, io: &fn(&mut Self) -> A) -> A {
        use rt::sched::{PinnedTask, TaskFromFriend};
        // go home
        let old_home = Cell::new_empty();
        let old_home_ptr = &old_home;
        let scheduler = Local::take::<Scheduler>();
        do scheduler.deschedule_running_task_and_then |_, task| {
            // get the old home first
            do task.wake().map_move |mut task| {
                old_home_ptr.put_back(task.take_unwrap_home());
                self.home().send(PinnedTask(task));
            };
        }

        // do IO
        let a = io(self);

        // unhome home
        let scheduler = Local::take::<Scheduler>();
        do scheduler.deschedule_running_task_and_then |scheduler, task| {
            do task.wake().map_move |mut task| {
                task.give_home(old_home.take());
                scheduler.make_handle().send(TaskFromFriend(task));
            };
        }

        // return the result of the IO
        a
    }
}

// get a handle for the current scheduler
macro_rules! get_handle_to_current_scheduler(
    () => (do Local::borrow::<Scheduler, SchedHandle> |sched| { sched.make_handle() })
)

enum SocketNameKind {
    TcpPeer,
    Tcp,
    Udp
}

fn socket_name<T, U: Watcher + NativeHandle<*T>>(sk: SocketNameKind,
                                                 handle: U) -> Result<SocketAddr, IoError> {
    let getsockname = match sk {
        TcpPeer => uvll::tcp_getpeername,
        Tcp     => uvll::tcp_getsockname,
        Udp     => uvll::udp_getsockname,
    };

    // Allocate a sockaddr_storage
    // since we don't know if it's ipv4 or ipv6
    let r_addr = unsafe { uvll::malloc_sockaddr_storage() };

    let r = unsafe {
        getsockname(handle.native_handle() as *c_void, r_addr as *uvll::sockaddr_storage)
    };

    if r != 0 {
        let status = status_to_maybe_uv_error(handle, r);
        return Err(uv_error_to_io_error(status.unwrap()));
    }

    let addr = unsafe {
        if uvll::is_ip6_addr(r_addr as *uvll::sockaddr) {
            net::uv_socket_addr_to_socket_addr(UvIpv6SocketAddr(r_addr as *uvll::sockaddr_in6))
        } else {
            net::uv_socket_addr_to_socket_addr(UvIpv4SocketAddr(r_addr as *uvll::sockaddr_in))
        }
    };

    unsafe { uvll::free_sockaddr_storage(r_addr); }

    Ok(addr)

}

// Obviously an Event Loop is always home.
pub struct UvEventLoop {
    uvio: UvIoFactory
}

impl UvEventLoop {
    pub fn new() -> UvEventLoop {
        UvEventLoop {
            uvio: UvIoFactory(Loop::new())
        }
    }
}

impl Drop for UvEventLoop {
    fn drop(&self) {
        // XXX: Need mutable finalizer
        let this = unsafe {
            transmute::<&UvEventLoop, &mut UvEventLoop>(self)
        };
        this.uvio.uv_loop().close();
    }
}

impl EventLoop for UvEventLoop {
    fn run(&mut self) {
        self.uvio.uv_loop().run();
    }

    fn callback(&mut self, f: ~fn()) {
        let mut idle_watcher =  IdleWatcher::new(self.uvio.uv_loop());
        do idle_watcher.start |mut idle_watcher, status| {
            assert!(status.is_none());
            idle_watcher.stop();
            idle_watcher.close(||());
            f();
        }
    }

    fn pausible_idle_callback(&mut self) -> ~PausibleIdleCallback {
        let idle_watcher = IdleWatcher::new(self.uvio.uv_loop());
        return ~UvPausibleIdleCallback {
            watcher: idle_watcher,
            idle_flag: false,
            closed: false
        };
    }

    fn callback_ms(&mut self, ms: u64, f: ~fn()) {
        let mut timer =  TimerWatcher::new(self.uvio.uv_loop());
        do timer.start(ms, 0) |timer, status| {
            assert!(status.is_none());
            timer.close(||());
            f();
        }
    }

    fn remote_callback(&mut self, f: ~fn()) -> ~RemoteCallbackObject {
        ~UvRemoteCallback::new(self.uvio.uv_loop(), f)
    }

    fn io<'a>(&'a mut self) -> Option<&'a mut IoFactoryObject> {
        Some(&mut self.uvio)
    }
}

pub struct UvPausibleIdleCallback {
    watcher: IdleWatcher,
    idle_flag: bool,
    closed: bool
}

impl UvPausibleIdleCallback {
    #[inline]
    pub fn start(&mut self, f: ~fn()) {
        do self.watcher.start |_idle_watcher, _status| {
            f();
        };
        self.idle_flag = true;
    }
    #[inline]
    pub fn pause(&mut self) {
        if self.idle_flag == true {
            self.watcher.stop();
            self.idle_flag = false;
        }
    }
    #[inline]
    pub fn resume(&mut self) {
        if self.idle_flag == false {
            self.watcher.restart();
            self.idle_flag = true;
        }
    }
    #[inline]
    pub fn close(&mut self) {
        self.pause();
        if !self.closed {
            self.closed = true;
            self.watcher.close(||{});
        }
    }
}

#[test]
fn test_callback_run_once() {
    do run_in_bare_thread {
        let mut event_loop = UvEventLoop::new();
        let mut count = 0;
        let count_ptr: *mut int = &mut count;
        do event_loop.callback {
            unsafe { *count_ptr += 1 }
        }
        event_loop.run();
        assert_eq!(count, 1);
    }
}

// The entire point of async is to call into a loop from other threads so it does not need to home.
pub struct UvRemoteCallback {
    // The uv async handle for triggering the callback
    async: AsyncWatcher,
    // A flag to tell the callback to exit, set from the dtor. This is
    // almost never contested - only in rare races with the dtor.
    exit_flag: Exclusive<bool>
}

impl UvRemoteCallback {
    pub fn new(loop_: &mut Loop, f: ~fn()) -> UvRemoteCallback {
        let exit_flag = Exclusive::new(false);
        let exit_flag_clone = exit_flag.clone();
        let async = do AsyncWatcher::new(loop_) |watcher, status| {
            assert!(status.is_none());

            // The synchronization logic here is subtle. To review,
            // the uv async handle type promises that, after it is
            // triggered the remote callback is definitely called at
            // least once. UvRemoteCallback needs to maintain those
            // semantics while also shutting down cleanly from the
            // dtor. In our case that means that, when the
            // UvRemoteCallback dtor calls `async.send()`, here `f` is
            // always called later.

            // In the dtor both the exit flag is set and the async
            // callback fired under a lock.  Here, before calling `f`,
            // we take the lock and check the flag. Because we are
            // checking the flag before calling `f`, and the flag is
            // set under the same lock as the send, then if the flag
            // is set then we're guaranteed to call `f` after the
            // final send.

            // If the check was done after `f()` then there would be a
            // period between that call and the check where the dtor
            // could be called in the other thread, missing the final
            // callback while still destroying the handle.

            let should_exit = unsafe {
                exit_flag_clone.with_imm(|&should_exit| should_exit)
            };

            f();

            if should_exit {
                watcher.close(||());
            }

        };
        UvRemoteCallback {
            async: async,
            exit_flag: exit_flag
        }
    }
}

impl RemoteCallback for UvRemoteCallback {
    fn fire(&mut self) { self.async.send() }
}

impl Drop for UvRemoteCallback {
    fn drop(&self) {
        unsafe {
            let this: &mut UvRemoteCallback = cast::transmute_mut(self);
            do this.exit_flag.with |should_exit| {
                // NB: These two things need to happen atomically. Otherwise
                // the event handler could wake up due to a *previous*
                // signal and see the exit flag, destroying the handle
                // before the final send.
                *should_exit = true;
                this.async.send();
            }
        }
    }
}

#[cfg(test)]
mod test_remote {
    use cell::Cell;
    use rt::test::*;
    use rt::thread::Thread;
    use rt::tube::Tube;
    use rt::rtio::EventLoop;
    use rt::local::Local;
    use rt::sched::Scheduler;

    #[test]
    fn test_uv_remote() {
        do run_in_newsched_task {
            let mut tube = Tube::new();
            let tube_clone = tube.clone();
            let remote_cell = Cell::new_empty();
            do Local::borrow::<Scheduler, ()>() |sched| {
                let tube_clone = tube_clone.clone();
                let tube_clone_cell = Cell::new(tube_clone);
                let remote = do sched.event_loop.remote_callback {
                    // This could be called multiple times
                    if !tube_clone_cell.is_empty() {
                        tube_clone_cell.take().send(1);
                    }
                };
                remote_cell.put_back(remote);
            }
            let thread = do Thread::start {
                remote_cell.take().fire();
            };

            assert!(tube.recv() == 1);
            thread.join();
        }
    }
}

pub struct UvIoFactory(Loop);

impl UvIoFactory {
    pub fn uv_loop<'a>(&'a mut self) -> &'a mut Loop {
        match self { &UvIoFactory(ref mut ptr) => ptr }
    }
}

impl IoFactory for UvIoFactory {
    // Connect to an address and return a new stream
    // NB: This blocks the task waiting on the connection.
    // It would probably be better to return a future
    fn tcp_connect(&mut self, addr: SocketAddr) -> Result<~RtioTcpStreamObject, IoError> {
        // Create a cell in the task to hold the result. We will fill
        // the cell before resuming the task.
        let result_cell = Cell::new_empty();
        let result_cell_ptr: *Cell<Result<~RtioTcpStreamObject, IoError>> = &result_cell;

        // Block this task and take ownership, switch to scheduler context
        let scheduler = Local::take::<Scheduler>();
        do scheduler.deschedule_running_task_and_then |_, task| {

            let mut tcp = TcpWatcher::new(self.uv_loop());
            let task_cell = Cell::new(task);

            // Wait for a connection
            do tcp.connect(addr) |stream, status| {
                match status {
                    None => {
                        let tcp = NativeHandle::from_native_handle(stream.native_handle());
                        let home = get_handle_to_current_scheduler!();
                        let res = Ok(~UvTcpStream { watcher: tcp, home: home });

                        // Store the stream in the task's stack
                        unsafe { (*result_cell_ptr).put_back(res); }

                        // Context switch
                        let scheduler = Local::take::<Scheduler>();
                        scheduler.resume_blocked_task_immediately(task_cell.take());
                    }
                    Some(_) => {
                        let task_cell = Cell::new(task_cell.take());
                        do stream.close {
                            let res = Err(uv_error_to_io_error(status.unwrap()));
                            unsafe { (*result_cell_ptr).put_back(res); }
                            let scheduler = Local::take::<Scheduler>();
                            scheduler.resume_blocked_task_immediately(task_cell.take());
                        }
                    }
                }
            }
        }

        assert!(!result_cell.is_empty());
        return result_cell.take();
    }

    fn tcp_bind(&mut self, addr: SocketAddr) -> Result<~RtioTcpListenerObject, IoError> {
        let mut watcher = TcpWatcher::new(self.uv_loop());
        match watcher.bind(addr) {
            Ok(_) => {
                let home = get_handle_to_current_scheduler!();
                Ok(~UvTcpListener::new(watcher, home))
            }
            Err(uverr) => {
                let scheduler = Local::take::<Scheduler>();
                do scheduler.deschedule_running_task_and_then |_, task| {
                    let task_cell = Cell::new(task);
                    do watcher.as_stream().close {
                        let scheduler = Local::take::<Scheduler>();
                        scheduler.resume_blocked_task_immediately(task_cell.take());
                    }
                }
                Err(uv_error_to_io_error(uverr))
            }
        }
    }

    fn udp_bind(&mut self, addr: SocketAddr) -> Result<~RtioUdpSocketObject, IoError> {
        let mut watcher = UdpWatcher::new(self.uv_loop());
        match watcher.bind(addr) {
            Ok(_) => {
                let home = get_handle_to_current_scheduler!();
                Ok(~UvUdpSocket { watcher: watcher, home: home })
            }
            Err(uverr) => {
                let scheduler = Local::take::<Scheduler>();
                do scheduler.deschedule_running_task_and_then |_, task| {
                    let task_cell = Cell::new(task);
                    do watcher.close {
                        let scheduler = Local::take::<Scheduler>();
                        scheduler.resume_blocked_task_immediately(task_cell.take());
                    }
                }
                Err(uv_error_to_io_error(uverr))
            }
        }
    }

    fn timer_init(&mut self) -> Result<~RtioTimerObject, IoError> {
        let watcher = TimerWatcher::new(self.uv_loop());
        let home = get_handle_to_current_scheduler!();
        Ok(~UvTimer::new(watcher, home))
    }
}

pub struct UvTcpListener {
    watcher: TcpWatcher,
    listening: bool,
    incoming_streams: Tube<Result<~RtioTcpStreamObject, IoError>>,
    home: SchedHandle,
}

impl HomingIO for UvTcpListener {
    fn home<'r>(&'r mut self) -> &'r mut SchedHandle { &mut self.home }
}

impl UvTcpListener {
    fn new(watcher: TcpWatcher, home: SchedHandle) -> UvTcpListener {
        UvTcpListener {
            watcher: watcher,
            listening: false,
            incoming_streams: Tube::new(),
            home: home,
        }
    }

    fn watcher(&self) -> TcpWatcher { self.watcher }
}

impl Drop for UvTcpListener {
    fn drop(&self) {
        // XXX need mutable finalizer
        let self_ = unsafe { transmute::<&UvTcpListener, &mut UvTcpListener>(self) };
        do self_.home_for_io |self_| {
            let scheduler = Local::take::<Scheduler>();
            do scheduler.deschedule_running_task_and_then |_, task| {
                let task_cell = Cell::new(task);
                do self_.watcher().as_stream().close {
                    let scheduler = Local::take::<Scheduler>();
                    scheduler.resume_blocked_task_immediately(task_cell.take());
                }
            }
        }
    }
}

impl RtioSocket for UvTcpListener {
    fn socket_name(&mut self) -> Result<SocketAddr, IoError> {
        do self.home_for_io |self_| {
          socket_name(Tcp, self_.watcher)
        }
    }
}

impl RtioTcpListener for UvTcpListener {

    fn accept(&mut self) -> Result<~RtioTcpStreamObject, IoError> {
        do self.home_for_io |self_| {

            if !self_.listening {
                self_.listening = true;

                let incoming_streams_cell = Cell::new(self_.incoming_streams.clone());

                do self_.watcher().listen |mut server, status| {
                    let stream = match status {
                        Some(_) => Err(standard_error(OtherIoError)),
                        None => {
                            let client = TcpWatcher::new(&server.event_loop());
                            // XXX: needs to be surfaced in interface
                            server.accept(client.as_stream());
                            let home = get_handle_to_current_scheduler!();
                            Ok(~UvTcpStream { watcher: client, home: home })
                        }
                    };

                    let mut incoming_streams = incoming_streams_cell.take();
                    incoming_streams.send(stream);
                    incoming_streams_cell.put_back(incoming_streams);
                }

            }
            self_.incoming_streams.recv()
        }
    }

    fn accept_simultaneously(&mut self) -> Result<(), IoError> {
        do self.home_for_io |self_| {
            let r = unsafe {
                uvll::tcp_simultaneous_accepts(self_.watcher().native_handle(), 1 as c_int)
            };

            match status_to_maybe_uv_error(self_.watcher(), r) {
                Some(err) => Err(uv_error_to_io_error(err)),
                None => Ok(())
            }
        }
    }

    fn dont_accept_simultaneously(&mut self) -> Result<(), IoError> {
        do self.home_for_io |self_| {
            let r = unsafe {
                uvll::tcp_simultaneous_accepts(self_.watcher().native_handle(), 0 as c_int)
            };

            match status_to_maybe_uv_error(self_.watcher(), r) {
                Some(err) => Err(uv_error_to_io_error(err)),
                None => Ok(())
            }
        }
    }
}

pub struct UvTcpStream {
    watcher: TcpWatcher,
    home: SchedHandle,
}

impl HomingIO for UvTcpStream {
    fn home<'r>(&'r mut self) -> &'r mut SchedHandle { &mut self.home }
}

impl Drop for UvTcpStream {
    fn drop(&self) {
        // XXX need mutable finalizer
        let this = unsafe { transmute::<&UvTcpStream, &mut UvTcpStream>(self) };
        do this.home_for_io |self_| {
            let scheduler = Local::take::<Scheduler>();
            do scheduler.deschedule_running_task_and_then |_, task| {
                let task_cell = Cell::new(task);
                do self_.watcher.as_stream().close {
                    let scheduler = Local::take::<Scheduler>();
                    scheduler.resume_blocked_task_immediately(task_cell.take());
                }
            }
        }
    }
}

impl RtioSocket for UvTcpStream {
    fn socket_name(&mut self) -> Result<SocketAddr, IoError> {
        do self.home_for_io |self_| {
            socket_name(Tcp, self_.watcher)
        }
    }
}

impl RtioTcpStream for UvTcpStream {
    fn read(&mut self, buf: &mut [u8]) -> Result<uint, IoError> {
        do self.home_for_io |self_| {
            let result_cell = Cell::new_empty();
            let result_cell_ptr: *Cell<Result<uint, IoError>> = &result_cell;

            let scheduler = Local::take::<Scheduler>();
            let buf_ptr: *&mut [u8] = &buf;
            do scheduler.deschedule_running_task_and_then |_sched, task| {
                let task_cell = Cell::new(task);
                // XXX: We shouldn't reallocate these callbacks every
                // call to read
                let alloc: AllocCallback = |_| unsafe {
                    slice_to_uv_buf(*buf_ptr)
                };
                let mut watcher = self_.watcher.as_stream();
                do watcher.read_start(alloc) |mut watcher, nread, _buf, status| {

                    // Stop reading so that no read callbacks are
                    // triggered before the user calls `read` again.
                    // XXX: Is there a performance impact to calling
                    // stop here?
                    watcher.read_stop();

                    let result = if status.is_none() {
                        assert!(nread >= 0);
                        Ok(nread as uint)
                    } else {
                        Err(uv_error_to_io_error(status.unwrap()))
                    };

                    unsafe { (*result_cell_ptr).put_back(result); }

                    let scheduler = Local::take::<Scheduler>();
                    scheduler.resume_blocked_task_immediately(task_cell.take());
                }
            }

            assert!(!result_cell.is_empty());
            result_cell.take()
        }
    }

    fn write(&mut self, buf: &[u8]) -> Result<(), IoError> {
        do self.home_for_io |self_| {
            let result_cell = Cell::new_empty();
            let result_cell_ptr: *Cell<Result<(), IoError>> = &result_cell;
            let scheduler = Local::take::<Scheduler>();
            let buf_ptr: *&[u8] = &buf;
            do scheduler.deschedule_running_task_and_then |_, task| {
                let task_cell = Cell::new(task);
                let buf = unsafe { slice_to_uv_buf(*buf_ptr) };
                let mut watcher = self_.watcher.as_stream();
                do watcher.write(buf) |_watcher, status| {
                    let result = if status.is_none() {
                        Ok(())
                    } else {
                        Err(uv_error_to_io_error(status.unwrap()))
                    };

                    unsafe { (*result_cell_ptr).put_back(result); }

                    let scheduler = Local::take::<Scheduler>();
                    scheduler.resume_blocked_task_immediately(task_cell.take());
                }
            }

            assert!(!result_cell.is_empty());
            result_cell.take()
        }
    }

    fn peer_name(&mut self) -> Result<SocketAddr, IoError> {
        do self.home_for_io |self_| {
            socket_name(TcpPeer, self_.watcher)
        }
    }

    fn control_congestion(&mut self) -> Result<(), IoError> {
        do self.home_for_io |self_| {
            let r = unsafe { uvll::tcp_nodelay(self_.watcher.native_handle(), 0 as c_int) };

            match status_to_maybe_uv_error(self_.watcher, r) {
                Some(err) => Err(uv_error_to_io_error(err)),
                None => Ok(())
            }
        }
    }

    fn nodelay(&mut self) -> Result<(), IoError> {
        do self.home_for_io |self_| {
            let r = unsafe { uvll::tcp_nodelay(self_.watcher.native_handle(), 1 as c_int) };

            match status_to_maybe_uv_error(self_.watcher, r) {
                Some(err) => Err(uv_error_to_io_error(err)),
                None => Ok(())
            }
        }
    }

    fn keepalive(&mut self, delay_in_seconds: uint) -> Result<(), IoError> {
        do self.home_for_io |self_| {
            let r = unsafe {
                uvll::tcp_keepalive(self_.watcher.native_handle(), 1 as c_int,
                                    delay_in_seconds as c_uint)
            };

            match status_to_maybe_uv_error(self_.watcher, r) {
                Some(err) => Err(uv_error_to_io_error(err)),
                None => Ok(())
            }
        }
    }

    fn letdie(&mut self) -> Result<(), IoError> {
        do self.home_for_io |self_| {
            let r = unsafe {
                uvll::tcp_keepalive(self_.watcher.native_handle(), 0 as c_int, 0 as c_uint)
            };

            match status_to_maybe_uv_error(self_.watcher, r) {
                Some(err) => Err(uv_error_to_io_error(err)),
                None => Ok(())
            }
        }
    }
}

pub struct UvUdpSocket {
    watcher: UdpWatcher,
    home: SchedHandle,
}

impl HomingIO for UvUdpSocket {
    fn home<'r>(&'r mut self) -> &'r mut SchedHandle { &mut self.home }
}

impl Drop for UvUdpSocket {
    fn drop(&self) {
        // XXX need mutable finalizer
        let this = unsafe { transmute::<&UvUdpSocket, &mut UvUdpSocket>(self) };
        do this.home_for_io |_| {
            let scheduler = Local::take::<Scheduler>();
            do scheduler.deschedule_running_task_and_then |_, task| {
                let task_cell = Cell::new(task);
                do this.watcher.close {
                    let scheduler = Local::take::<Scheduler>();
                    scheduler.resume_blocked_task_immediately(task_cell.take());
                }
            }
        }
    }
}

impl RtioSocket for UvUdpSocket {
    fn socket_name(&mut self) -> Result<SocketAddr, IoError> {
        do self.home_for_io |self_| {
            socket_name(Udp, self_.watcher)
        }
    }
}

impl RtioUdpSocket for UvUdpSocket {
    fn recvfrom(&mut self, buf: &mut [u8]) -> Result<(uint, SocketAddr), IoError> {
        do self.home_for_io |self_| {
            let result_cell = Cell::new_empty();
            let result_cell_ptr: *Cell<Result<(uint, SocketAddr), IoError>> = &result_cell;

            let scheduler = Local::take::<Scheduler>();
            let buf_ptr: *&mut [u8] = &buf;
            do scheduler.deschedule_running_task_and_then |_, task| {
                let task_cell = Cell::new(task);
                let alloc: AllocCallback = |_| unsafe { slice_to_uv_buf(*buf_ptr) };
                do self_.watcher.recv_start(alloc) |mut watcher, nread, _buf, addr, flags, status| {
                    let _ = flags; // /XXX add handling for partials?

                    watcher.recv_stop();

                    let result = match status {
                        None => {
                            assert!(nread >= 0);
                            Ok((nread as uint, addr))
                        }
                        Some(err) => Err(uv_error_to_io_error(err)),
                    };

                    unsafe { (*result_cell_ptr).put_back(result); }

                    let scheduler = Local::take::<Scheduler>();
                    scheduler.resume_blocked_task_immediately(task_cell.take());
                }
            }

            assert!(!result_cell.is_empty());
            result_cell.take()
        }
    }

    fn sendto(&mut self, buf: &[u8], dst: SocketAddr) -> Result<(), IoError> {
        do self.home_for_io |self_| {
            let result_cell = Cell::new_empty();
            let result_cell_ptr: *Cell<Result<(), IoError>> = &result_cell;
            let scheduler = Local::take::<Scheduler>();
            let buf_ptr: *&[u8] = &buf;
            do scheduler.deschedule_running_task_and_then |_, task| {
                let task_cell = Cell::new(task);
                let buf = unsafe { slice_to_uv_buf(*buf_ptr) };
                do self_.watcher.send(buf, dst) |_watcher, status| {

                    let result = match status {
                        None => Ok(()),
                        Some(err) => Err(uv_error_to_io_error(err)),
                    };

                    unsafe { (*result_cell_ptr).put_back(result); }

                    let scheduler = Local::take::<Scheduler>();
                    scheduler.resume_blocked_task_immediately(task_cell.take());
                }
            }

            assert!(!result_cell.is_empty());
            result_cell.take()
        }
    }

    fn join_multicast(&mut self, multi: IpAddr) -> Result<(), IoError> {
        do self.home_for_io |self_| {
            let r = unsafe {
                do multi.to_str().with_c_str |m_addr| {
                    uvll::udp_set_membership(self_.watcher.native_handle(), m_addr,
                                             ptr::null(), uvll::UV_JOIN_GROUP)
                }
            };

            match status_to_maybe_uv_error(self_.watcher, r) {
                Some(err) => Err(uv_error_to_io_error(err)),
                None => Ok(())
            }
        }
    }

    fn leave_multicast(&mut self, multi: IpAddr) -> Result<(), IoError> {
        do self.home_for_io |self_| {
            let r = unsafe {
                do multi.to_str().with_c_str |m_addr| {
                    uvll::udp_set_membership(self_.watcher.native_handle(), m_addr,
                                             ptr::null(), uvll::UV_LEAVE_GROUP)
                }
            };

            match status_to_maybe_uv_error(self_.watcher, r) {
                Some(err) => Err(uv_error_to_io_error(err)),
                None => Ok(())
            }
        }
    }

    fn loop_multicast_locally(&mut self) -> Result<(), IoError> {
        do self.home_for_io |self_| {

            let r = unsafe {
                uvll::udp_set_multicast_loop(self_.watcher.native_handle(), 1 as c_int)
            };

            match status_to_maybe_uv_error(self_.watcher, r) {
                Some(err) => Err(uv_error_to_io_error(err)),
                None => Ok(())
            }
        }
    }

    fn dont_loop_multicast_locally(&mut self) -> Result<(), IoError> {
        do self.home_for_io |self_| {

            let r = unsafe {
                uvll::udp_set_multicast_loop(self_.watcher.native_handle(), 0 as c_int)
            };

            match status_to_maybe_uv_error(self_.watcher, r) {
                Some(err) => Err(uv_error_to_io_error(err)),
                None => Ok(())
            }
        }
    }

    fn multicast_time_to_live(&mut self, ttl: int) -> Result<(), IoError> {
        do self.home_for_io |self_| {

            let r = unsafe {
                uvll::udp_set_multicast_ttl(self_.watcher.native_handle(), ttl as c_int)
            };

            match status_to_maybe_uv_error(self_.watcher, r) {
                Some(err) => Err(uv_error_to_io_error(err)),
                None => Ok(())
            }
        }
    }

    fn time_to_live(&mut self, ttl: int) -> Result<(), IoError> {
        do self.home_for_io |self_| {

            let r = unsafe {
                uvll::udp_set_ttl(self_.watcher.native_handle(), ttl as c_int)
            };

            match status_to_maybe_uv_error(self_.watcher, r) {
                Some(err) => Err(uv_error_to_io_error(err)),
                None => Ok(())
            }
        }
    }

    fn hear_broadcasts(&mut self) -> Result<(), IoError> {
        do self.home_for_io |self_| {

            let r = unsafe {
                uvll::udp_set_broadcast(self_.watcher.native_handle(), 1 as c_int)
            };

            match status_to_maybe_uv_error(self_.watcher, r) {
                Some(err) => Err(uv_error_to_io_error(err)),
                None => Ok(())
            }
        }
    }

    fn ignore_broadcasts(&mut self) -> Result<(), IoError> {
        do self.home_for_io |self_| {

            let r = unsafe {
                uvll::udp_set_broadcast(self_.watcher.native_handle(), 0 as c_int)
            };

            match status_to_maybe_uv_error(self_.watcher, r) {
                Some(err) => Err(uv_error_to_io_error(err)),
                None => Ok(())
            }
        }
    }
}

pub struct UvTimer {
    watcher: timer::TimerWatcher,
    home: SchedHandle,
}

impl HomingIO for UvTimer {
    fn home<'r>(&'r mut self) -> &'r mut SchedHandle { &mut self.home }
}

impl UvTimer {
    fn new(w: timer::TimerWatcher, home: SchedHandle) -> UvTimer {
        UvTimer { watcher: w, home: home }
    }
}

impl Drop for UvTimer {
    fn drop(&self) {
        let self_ = unsafe { transmute::<&UvTimer, &mut UvTimer>(self) };
        do self_.home_for_io |self_| {
            rtdebug!("closing UvTimer");
            let scheduler = Local::take::<Scheduler>();
            do scheduler.deschedule_running_task_and_then |_, task| {
                let task_cell = Cell::new(task);
                do self_.watcher.close {
                    let scheduler = Local::take::<Scheduler>();
                    scheduler.resume_blocked_task_immediately(task_cell.take());
                }
            }
        }
    }
}

impl RtioTimer for UvTimer {
    fn sleep(&mut self, msecs: u64) {
        do self.home_for_io |self_| {
            let scheduler = Local::take::<Scheduler>();
            do scheduler.deschedule_running_task_and_then |_sched, task| {
                rtdebug!("sleep: entered scheduler context");
                let task_cell = Cell::new(task);
                do self_.watcher.start(msecs, 0) |_, status| {
                    assert!(status.is_none());
                    let scheduler = Local::take::<Scheduler>();
                    scheduler.resume_blocked_task_immediately(task_cell.take());
                }
            }
            self_.watcher.stop();
        }
    }
}

#[test]
fn test_simple_io_no_connect() {
    do run_in_newsched_task {
        unsafe {
            let io = Local::unsafe_borrow::<IoFactoryObject>();
            let addr = next_test_ip4();
            let maybe_chan = (*io).tcp_connect(addr);
            assert!(maybe_chan.is_err());
        }
    }
}

#[test]
fn test_simple_udp_io_bind_only() {
    do run_in_newsched_task {
        unsafe {
            let io = Local::unsafe_borrow::<IoFactoryObject>();
            let addr = next_test_ip4();
            let maybe_socket = (*io).udp_bind(addr);
            assert!(maybe_socket.is_ok());
        }
    }
}

#[test]
fn test_simple_homed_udp_io_bind_then_move_task_then_home_and_close() {
    use rt::sleeper_list::SleeperList;
    use rt::work_queue::WorkQueue;
    use rt::thread::Thread;
    use rt::task::Task;
    use rt::sched::{Shutdown, TaskFromFriend};
    do run_in_bare_thread {
        let sleepers = SleeperList::new();
        let work_queue1 = WorkQueue::new();
        let work_queue2 = WorkQueue::new();
        let queues = ~[work_queue1.clone(), work_queue2.clone()];

        let mut sched1 = ~Scheduler::new(~UvEventLoop::new(), work_queue1, queues.clone(),
                                         sleepers.clone());
        let mut sched2 = ~Scheduler::new(~UvEventLoop::new(), work_queue2, queues.clone(),
                                         sleepers.clone());

        let handle1 = Cell::new(sched1.make_handle());
        let handle2 = Cell::new(sched2.make_handle());
        let tasksFriendHandle = Cell::new(sched2.make_handle());

        let on_exit: ~fn(bool) = |exit_status| {
            handle1.take().send(Shutdown);
            handle2.take().send(Shutdown);
            rtassert!(exit_status);
        };

        let test_function: ~fn() = || {
            let io = unsafe { Local::unsafe_borrow::<IoFactoryObject>() };
            let addr = next_test_ip4();
            let maybe_socket = unsafe { (*io).udp_bind(addr) };
            // this socket is bound to this event loop
            assert!(maybe_socket.is_ok());

            // block self on sched1
            let scheduler = Local::take::<Scheduler>();
            do scheduler.deschedule_running_task_and_then |_, task| {
                // unblock task
                do task.wake().map_move |task| {
                  // send self to sched2
                  tasksFriendHandle.take().send(TaskFromFriend(task));
                };
                // sched1 should now sleep since it has nothing else to do
            }
            // sched2 will wake up and get the task
            // as we do nothing else, the function ends and the socket goes out of scope
            // sched2 will start to run the destructor
            // the destructor will first block the task, set it's home as sched1, then enqueue it
            // sched2 will dequeue the task, see that it has a home, and send it to sched1
            // sched1 will wake up, exec the close function on the correct loop, and then we're done
        };

        let mut main_task = ~Task::new_root(&mut sched1.stack_pool, None, test_function);
        main_task.death.on_exit = Some(on_exit);
        let main_task = Cell::new(main_task);

        let null_task = Cell::new(~do Task::new_root(&mut sched2.stack_pool, None) || {});

        let sched1 = Cell::new(sched1);
        let sched2 = Cell::new(sched2);

        let thread1 = do Thread::start {
            sched1.take().bootstrap(main_task.take());
        };
        let thread2 = do Thread::start {
            sched2.take().bootstrap(null_task.take());
        };

        thread1.join();
        thread2.join();
    }
}

#[test]
fn test_simple_homed_udp_io_bind_then_move_handle_then_home_and_close() {
    use rt::sleeper_list::SleeperList;
    use rt::work_queue::WorkQueue;
    use rt::thread::Thread;
    use rt::task::Task;
    use rt::comm::oneshot;
    use rt::sched::Shutdown;
    do run_in_bare_thread {
        let sleepers = SleeperList::new();
        let work_queue1 = WorkQueue::new();
        let work_queue2 = WorkQueue::new();
        let queues = ~[work_queue1.clone(), work_queue2.clone()];

        let mut sched1 = ~Scheduler::new(~UvEventLoop::new(), work_queue1, queues.clone(),
                                         sleepers.clone());
        let mut sched2 = ~Scheduler::new(~UvEventLoop::new(), work_queue2, queues.clone(),
                                         sleepers.clone());

        let handle1 = Cell::new(sched1.make_handle());
        let handle2 = Cell::new(sched2.make_handle());

        let (port, chan) = oneshot();
        let port = Cell::new(port);
        let chan = Cell::new(chan);

        let body1: ~fn() = || {
            let io = unsafe { Local::unsafe_borrow::<IoFactoryObject>() };
            let addr = next_test_ip4();
            let socket = unsafe { (*io).udp_bind(addr) };
            assert!(socket.is_ok());
            chan.take().send(socket);
        };

        let body2: ~fn() = || {
            let socket = port.take().recv();
            assert!(socket.is_ok());
            /* The socket goes out of scope and the destructor is called.
             * The destructor:
             *  - sends itself back to sched1
             *  - frees the socket
             *  - resets the home of the task to whatever it was previously
             */
        };

        let on_exit: ~fn(bool) = |exit| {
            handle1.take().send(Shutdown);
            handle2.take().send(Shutdown);
            rtassert!(exit);
        };

        let task1 = Cell::new(~Task::new_root(&mut sched1.stack_pool, None, body1));

        let mut task2 = ~Task::new_root(&mut sched2.stack_pool, None, body2);
        task2.death.on_exit = Some(on_exit);
        let task2 = Cell::new(task2);

        let sched1 = Cell::new(sched1);
        let sched2 = Cell::new(sched2);

        let thread1 = do Thread::start {
            sched1.take().bootstrap(task1.take());
        };
        let thread2 = do Thread::start {
            sched2.take().bootstrap(task2.take());
        };

        thread1.join();
        thread2.join();
    }
}

#[test]
fn test_simple_tcp_server_and_client() {
    do run_in_newsched_task {
        let addr = next_test_ip4();

        // Start the server first so it's listening when we connect
        do spawntask {
            unsafe {
                let io = Local::unsafe_borrow::<IoFactoryObject>();
                let mut listener = (*io).tcp_bind(addr).unwrap();
                let mut stream = listener.accept().unwrap();
                let mut buf = [0, .. 2048];
                let nread = stream.read(buf).unwrap();
                assert_eq!(nread, 8);
                for i in range(0u, nread) {
                    rtdebug!("%u", buf[i] as uint);
                    assert_eq!(buf[i], i as u8);
                }
            }
        }

        do spawntask {
            unsafe {
                let io = Local::unsafe_borrow::<IoFactoryObject>();
                let mut stream = (*io).tcp_connect(addr).unwrap();
                stream.write([0, 1, 2, 3, 4, 5, 6, 7]);
            }
        }
    }
}

#[test]
fn test_simple_tcp_server_and_client_on_diff_threads() {
    use rt::sleeper_list::SleeperList;
    use rt::work_queue::WorkQueue;
    use rt::thread::Thread;
    use rt::task::Task;
    use rt::sched::{Shutdown};
    do run_in_bare_thread {
        let sleepers = SleeperList::new();

        let server_addr = next_test_ip4();
        let client_addr = server_addr.clone();

        let server_work_queue = WorkQueue::new();
        let client_work_queue = WorkQueue::new();
        let queues = ~[server_work_queue.clone(), client_work_queue.clone()];

        let mut server_sched = ~Scheduler::new(~UvEventLoop::new(), server_work_queue,
                                               queues.clone(), sleepers.clone());
        let mut client_sched = ~Scheduler::new(~UvEventLoop::new(), client_work_queue,
                                               queues.clone(), sleepers.clone());

        let server_handle = Cell::new(server_sched.make_handle());
        let client_handle = Cell::new(client_sched.make_handle());

        let server_on_exit: ~fn(bool) = |exit_status| {
            server_handle.take().send(Shutdown);
            rtassert!(exit_status);
        };

        let client_on_exit: ~fn(bool) = |exit_status| {
            client_handle.take().send(Shutdown);
            rtassert!(exit_status);
        };

        let server_fn: ~fn() = || {
            let io = unsafe { Local::unsafe_borrow::<IoFactoryObject>() };
            let mut listener = unsafe { (*io).tcp_bind(server_addr).unwrap() };
            let mut stream = listener.accept().unwrap();
            let mut buf = [0, .. 2048];
            let nread = stream.read(buf).unwrap();
            assert_eq!(nread, 8);
            for i in range(0u, nread) {
                assert_eq!(buf[i], i as u8);
            }
        };

        let client_fn: ~fn() = || {
            let io = unsafe { Local::unsafe_borrow::<IoFactoryObject>() };
            let mut stream = unsafe { (*io).tcp_connect(client_addr) };
            while stream.is_err() {
                stream = unsafe { (*io).tcp_connect(client_addr) };
            }
            stream.unwrap().write([0, 1, 2, 3, 4, 5, 6, 7]);
        };

        let mut server_task = ~Task::new_root(&mut server_sched.stack_pool, None, server_fn);
        server_task.death.on_exit = Some(server_on_exit);
        let server_task = Cell::new(server_task);

        let mut client_task = ~Task::new_root(&mut client_sched.stack_pool, None, client_fn);
        client_task.death.on_exit = Some(client_on_exit);
        let client_task = Cell::new(client_task);

        let server_sched = Cell::new(server_sched);
        let client_sched = Cell::new(client_sched);

        let server_thread = do Thread::start {
            server_sched.take().bootstrap(server_task.take());
        };
        let client_thread = do Thread::start {
            client_sched.take().bootstrap(client_task.take());
        };

        server_thread.join();
        client_thread.join();
    }
}

#[test]
fn test_simple_udp_server_and_client() {
    do run_in_newsched_task {
        let server_addr = next_test_ip4();
        let client_addr = next_test_ip4();

        do spawntask {
            unsafe {
                let io = Local::unsafe_borrow::<IoFactoryObject>();
                let mut server_socket = (*io).udp_bind(server_addr).unwrap();
                let mut buf = [0, .. 2048];
                let (nread,src) = server_socket.recvfrom(buf).unwrap();
                assert_eq!(nread, 8);
                for i in range(0u, nread) {
                    rtdebug!("%u", buf[i] as uint);
                    assert_eq!(buf[i], i as u8);
                }
                assert_eq!(src, client_addr);
            }
        }

        do spawntask {
            unsafe {
                let io = Local::unsafe_borrow::<IoFactoryObject>();
                let mut client_socket = (*io).udp_bind(client_addr).unwrap();
                client_socket.sendto([0, 1, 2, 3, 4, 5, 6, 7], server_addr);
            }
        }
    }
}

#[test] #[ignore(reason = "busted")]
fn test_read_and_block() {
    do run_in_newsched_task {
        let addr = next_test_ip4();

        do spawntask {
            let io = unsafe { Local::unsafe_borrow::<IoFactoryObject>() };
            let mut listener = unsafe { (*io).tcp_bind(addr).unwrap() };
            let mut stream = listener.accept().unwrap();
            let mut buf = [0, .. 2048];

            let expected = 32;
            let mut current = 0;
            let mut reads = 0;

            while current < expected {
                let nread = stream.read(buf).unwrap();
                for i in range(0u, nread) {
                    let val = buf[i] as uint;
                    assert_eq!(val, current % 8);
                    current += 1;
                }
                reads += 1;

                let scheduler = Local::take::<Scheduler>();
                // Yield to the other task in hopes that it
                // will trigger a read callback while we are
                // not ready for it
                do scheduler.deschedule_running_task_and_then |sched, task| {
                    let task = Cell::new(task);
                    sched.enqueue_blocked_task(task.take());
                }
            }

            // Make sure we had multiple reads
            assert!(reads > 1);
        }

        do spawntask {
            unsafe {
                let io = Local::unsafe_borrow::<IoFactoryObject>();
                let mut stream = (*io).tcp_connect(addr).unwrap();
                stream.write([0, 1, 2, 3, 4, 5, 6, 7]);
                stream.write([0, 1, 2, 3, 4, 5, 6, 7]);
                stream.write([0, 1, 2, 3, 4, 5, 6, 7]);
                stream.write([0, 1, 2, 3, 4, 5, 6, 7]);
            }
        }

    }
}

#[test]
fn test_read_read_read() {
    do run_in_newsched_task {
        let addr = next_test_ip4();
        static MAX: uint = 500000;

        do spawntask {
            unsafe {
                let io = Local::unsafe_borrow::<IoFactoryObject>();
                let mut listener = (*io).tcp_bind(addr).unwrap();
                let mut stream = listener.accept().unwrap();
                let buf = [1, .. 2048];
                let mut total_bytes_written = 0;
                while total_bytes_written < MAX {
                    stream.write(buf);
                    total_bytes_written += buf.len();
                }
            }
        }

        do spawntask {
            unsafe {
                let io = Local::unsafe_borrow::<IoFactoryObject>();
                let mut stream = (*io).tcp_connect(addr).unwrap();
                let mut buf = [0, .. 2048];
                let mut total_bytes_read = 0;
                while total_bytes_read < MAX {
                    let nread = stream.read(buf).unwrap();
                    rtdebug!("read %u bytes", nread as uint);
                    total_bytes_read += nread;
                    for i in range(0u, nread) {
                        assert_eq!(buf[i], 1);
                    }
                }
                rtdebug!("read %u bytes total", total_bytes_read as uint);
            }
        }
    }
}

#[test]
fn test_udp_twice() {
    do run_in_newsched_task {
        let server_addr = next_test_ip4();
        let client_addr = next_test_ip4();

        do spawntask {
            unsafe {
                let io = Local::unsafe_borrow::<IoFactoryObject>();
                let mut client = (*io).udp_bind(client_addr).unwrap();
                assert!(client.sendto([1], server_addr).is_ok());
                assert!(client.sendto([2], server_addr).is_ok());
            }
        }

        do spawntask {
            unsafe {
                let io = Local::unsafe_borrow::<IoFactoryObject>();
                let mut server = (*io).udp_bind(server_addr).unwrap();
                let mut buf1 = [0];
                let mut buf2 = [0];
                let (nread1, src1) = server.recvfrom(buf1).unwrap();
                let (nread2, src2) = server.recvfrom(buf2).unwrap();
                assert_eq!(nread1, 1);
                assert_eq!(nread2, 1);
                assert_eq!(src1, client_addr);
                assert_eq!(src2, client_addr);
                assert_eq!(buf1[0], 1);
                assert_eq!(buf2[0], 2);
            }
        }
    }
}

#[test]
fn test_udp_many_read() {
    do run_in_newsched_task {
        let server_out_addr = next_test_ip4();
        let server_in_addr = next_test_ip4();
        let client_out_addr = next_test_ip4();
        let client_in_addr = next_test_ip4();
        static MAX: uint = 500_000;

        do spawntask {
            unsafe {
                let io = Local::unsafe_borrow::<IoFactoryObject>();
                let mut server_out = (*io).udp_bind(server_out_addr).unwrap();
                let mut server_in = (*io).udp_bind(server_in_addr).unwrap();
                let msg = [1, .. 2048];
                let mut total_bytes_sent = 0;
                let mut buf = [1];
                while buf[0] == 1 {
                    // send more data
                    assert!(server_out.sendto(msg, client_in_addr).is_ok());
                    total_bytes_sent += msg.len();
                    // check if the client has received enough
                    let res = server_in.recvfrom(buf);
                    assert!(res.is_ok());
                    let (nread, src) = res.unwrap();
                    assert_eq!(nread, 1);
                    assert_eq!(src, client_out_addr);
                }
                assert!(total_bytes_sent >= MAX);
            }
        }

        do spawntask {
            unsafe {
                let io = Local::unsafe_borrow::<IoFactoryObject>();
                let mut client_out = (*io).udp_bind(client_out_addr).unwrap();
                let mut client_in = (*io).udp_bind(client_in_addr).unwrap();
                let mut total_bytes_recv = 0;
                let mut buf = [0, .. 2048];
                while total_bytes_recv < MAX {
                    // ask for more
                    assert!(client_out.sendto([1], server_in_addr).is_ok());
                    // wait for data
                    let res = client_in.recvfrom(buf);
                    assert!(res.is_ok());
                    let (nread, src) = res.unwrap();
                    assert_eq!(src, server_out_addr);
                    total_bytes_recv += nread;
                    for i in range(0u, nread) {
                        assert_eq!(buf[i], 1);
                    }
                }
                // tell the server we're done
                assert!(client_out.sendto([0], server_in_addr).is_ok());
            }
        }
    }
}

#[test]
fn test_timer_sleep_simple() {
    do run_in_newsched_task {
        unsafe {
            let io = Local::unsafe_borrow::<IoFactoryObject>();
            let timer = (*io).timer_init();
            do timer.map_move |mut t| { t.sleep(1) };
        }
    }
}
