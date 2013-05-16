// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use option::*;
use cast;
use util;
use ops::Drop;
use kinds::Owned;
use rt::sched::Coroutine;
use rt::local_sched;
#[cfg(stage0)]
use unstable::intrinsics::{atomic_xchg};
#[cfg(not(stage0))]
use unstable::intrinsics::{atomic_xchg, atomic_load};
use util::Void;
use comm::{GenericChan, GenericSmartChan, GenericPort, Peekable};
use cell::Cell;

/// A combined refcount / ~Task pointer.
///
/// Can be equal to the following values:
///
/// * 2 - both endpoints are alive
/// * 1 - either the sender or the receiver is dead, determined by context
/// * <ptr> - A pointer to a Task that can be transmuted to ~Task
type State = int;

static STATE_BOTH: State = 2;
static STATE_ONE: State = 1;

struct Packet<T> {
    state: State,
    payload: Option<T>,
}

pub struct PortOne<T> {
    // XXX: Hack extra allocation to make by-val self work
    inner: ~PortOneHack<T>
}

pub struct ChanOne<T> {
    // XXX: Hack extra allocation to make by-val self work
    inner: ~ChanOneHack<T>
}

pub struct PortOneHack<T> {
    void_packet: *mut Void,
    suppress_finalize: bool
}

pub struct ChanOneHack<T> {
    void_packet: *mut Void,
    suppress_finalize: bool
}

pub fn oneshot<T: Owned>() -> (PortOne<T>, ChanOne<T>) {
    let packet: ~Packet<T> = ~Packet {
        state: STATE_BOTH,
        payload: None
    };

    unsafe {
        let packet: *mut Void = cast::transmute(packet);
        let port = PortOne {
            inner: ~PortOneHack {
                void_packet: packet,
                suppress_finalize: false
            }
        };
        let chan = ChanOne {
            inner: ~ChanOneHack {
                void_packet: packet,
                suppress_finalize: false
            }
        };
        return (port, chan);
    }
}

impl<T> PortOne<T> {
    pub fn recv(self) -> T {
        match self.try_recv() {
            Some(val) => val,
            None => {
                fail!("receiving on closed channel");
            }
        }
    }

    pub fn try_recv(self) -> Option<T> {
        let mut this = self;

        {
            let self_ptr: *mut PortOne<T> = &mut this;

            // XXX: Optimize this to not require the two context switches when data is available

            // Switch to the scheduler
            let sched = local_sched::take();
            do sched.deschedule_running_task_and_then |task| {
                unsafe {
                    let task_as_state: State = cast::transmute(task);
                    let oldstate = atomic_xchg(&mut (*(*self_ptr).inner.packet()).state, task_as_state);
                    match oldstate {
                        STATE_BOTH => {
                            // Data has not been sent. Now we're blocked.
                        }
                        STATE_ONE => {
                            // Channel is closed. Switch back and check the data.
                            let task: ~Coroutine = cast::transmute(task_as_state);
                            let sched = local_sched::take();
                            sched.resume_task_immediately(task);
                        }
                        _ => util::unreachable()
                    }
                }
            }
        }

        // Task resumes.

        // No further memory barrier is needed here to access the
        // payload. Some scenarios:
        //
        // 1) We encountered STATE_ONE above - the atomic_xchg was the acq barrier. We're fine.
        // 2) We encountered STATE_BOTH above and blocked. The sending task work-stole us
        //    and ran on its thread. The work stealing had a memory barrier.
        // 3) We encountered STATE_BOTH above and blocked, but the receiving task (this task)
        //    is pinned to some other scheduler, so the sending task had to give us to
        //    a different scheduler for resuming. That send synchronized memory.

        unsafe {
            let payload = util::replace(&mut (*this.inner.packet()).payload, None);

            // The sender has closed up shop. Drop the packet.
            let _packet: ~Packet<T> = cast::transmute(this.inner.void_packet);
            // Supress the finalizer. We're done here.
            this.inner.suppress_finalize = true;

            return payload;
        }
    }
}

impl<T> Peekable<T> for PortOne<T> {
    #[cfg(stage0)]
    fn peek(&self) -> bool { fail!() }

    #[cfg(not(stage0))]
    fn peek(&self) -> bool {
        unsafe {
            let packet: *mut Packet<T> = self.inner.packet();
            let oldstate = atomic_load(&mut (*packet).state);
            match oldstate {
                STATE_BOTH => false,
                STATE_ONE => (*packet).payload.is_some(),
                _ => util::unreachable()
            }
        }
    }
}

impl<T> ChanOne<T> {

    pub fn send(self, val: T) {
        self.try_send(val);
    }

    pub fn try_send(self, val: T) -> bool {
        let mut this = self;
        let mut recvr_active = true;

        unsafe {
            assert!((*this.inner.packet()).payload.is_none());
            (*this.inner.packet()).payload = Some(val);

            let oldstate = atomic_xchg(&mut (*this.inner.packet()).state, STATE_ONE);
            match oldstate {
                STATE_BOTH => {
                    // Port is not recving yet. Nothing to do
                }
                STATE_ONE => {
                    // Port has closed. Need to clean up.
                    let _packet: ~Packet<T> = cast::transmute(this.inner.void_packet);
                    recvr_active = false;
                }
                _ => {
                    // Port is blocked. Wake it up.
                    let recvr: ~Coroutine = cast::transmute(oldstate);
                    let sched = local_sched::take();
                    sched.schedule_task(recvr);
                }
            }
        }

        // Suppress the finalizer. We're done here.
        this.inner.suppress_finalize = true;
        return recvr_active;
    }
}

#[unsafe_destructor]
impl<T> Drop for PortOneHack<T> {
    fn finalize(&self) {
        if self.suppress_finalize { return }

        unsafe {
            let this = cast::transmute_mut(self);
            let oldstate = atomic_xchg(&mut (*this.packet()).state, STATE_ONE);
            match oldstate {
                STATE_BOTH => {
                    /* cleanup is the chan's responsibility */
                },
                STATE_ONE => {
                    let _packet: ~Packet<T> = cast::transmute(this.void_packet);
                }
                _ => {
                    util::unreachable()
                }
            }
        }
    }
}

#[unsafe_destructor]
impl<T> Drop for ChanOneHack<T> {
    fn finalize(&self) {
        if self.suppress_finalize { return }

        unsafe {
            let this = cast::transmute_mut(self);
            let oldstate = atomic_xchg(&mut (*this.packet()).state, STATE_ONE);
            match oldstate {
                STATE_BOTH => {
                    /* cleanup is the port's responsibility */
                },
                STATE_ONE => {
                    let _packet: ~Packet<T> = cast::transmute(this.void_packet);
                },
                _ => {
                    // The port is blocked recving for a message we will never send. Wake it.
                    assert!((*this.packet()).payload.is_none());
                    let recvr: ~Coroutine = cast::transmute(oldstate);
                    let sched = local_sched::take();
                    sched.schedule_task(recvr);
                }
            }
        }
    }
}

impl<T> PortOneHack<T> {
    fn packet(&self) -> *mut Packet<T> {
        unsafe {
            let p: *mut ~Packet<T> = cast::transmute(&self.void_packet);
            let p: *mut Packet<T> = &mut **p;
            return p;
        }
    }
}

impl<T> ChanOneHack<T> {
    fn packet(&self) -> *mut Packet<T> {
        unsafe {
            let p: *mut ~Packet<T> = cast::transmute(&self.void_packet);
            let p: *mut Packet<T> = &mut **p;
            return p;
        }
    }
}

struct StreamPayload<T>(T, PortOne<StreamPayload<T>>);

pub struct Port<T> {
    // FIXME #5372. Using Cell because we don't take &mut self
    next: Cell<PortOne<StreamPayload<T>>>
}

pub struct Chan<T> {
    // FIXME #5372. Using Cell because we don't take &mut self
    next: Cell<ChanOne<StreamPayload<T>>>
}

pub fn stream<T: Owned>() -> (Port<T>, Chan<T>) {
    let (pone, cone) = oneshot();
    let port = Port { next: Cell(pone) };
    let chan = Chan { next: Cell(cone) };
    return (port, chan);
}

impl<T> GenericPort<T> for Port<T> {
    fn recv(&self) -> T {
        match self.try_recv() {
            Some(val) => val,
            None => {
                fail!("receiving on closed channel");
            }
        }
    }

    fn try_recv(&self) -> Option<T> {
        let pone = self.next.take();
        match pone.try_recv() {
            Some(StreamPayload(val, next)) => {
                self.next.put_back(next);
                Some(val)
            }
            None => None
        }
    }
}

impl<T> Peekable<T> for Port<T> {
    fn peek(&self) -> bool {
        self.next.with_mut_ref(|p| p.peek())
    }
}

impl<T: Owned> GenericChan<T> for Chan<T> {
    fn send(&self, val: T) {
        self.try_send(val);
    }
}

impl<T: Owned> GenericSmartChan<T> for Chan<T> {
    fn try_send(&self, val: T) -> bool {
        let (next_pone, next_cone) = oneshot();
        let cone = self.next.take();
        self.next.put_back(next_cone);
        cone.try_send(StreamPayload(val, next_pone))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use option::*;
    use rt::test::*;
    use cell::Cell;
    use iter::Times;

    #[test]
    fn oneshot_single_thread_close_port_first() {
        // Simple test of closing without sending
        do run_in_newsched_task {
            let (port, _chan) = oneshot::<int>();
            { let _p = port; }
        }
    }

    #[test]
    fn oneshot_single_thread_close_chan_first() {
        // Simple test of closing without sending
        do run_in_newsched_task {
            let (_port, chan) = oneshot::<int>();
            { let _c = chan; }
        }
    }

    #[test]
    fn oneshot_single_thread_send_port_close() {
        // Testing that the sender cleans up the payload if receiver is closed
        do run_in_newsched_task {
            let (port, chan) = oneshot::<~int>();
            { let _p = port; }
            chan.send(~0);
        }
    }

    #[test]
    fn oneshot_single_thread_recv_chan_close() {
        // Receiving on a closed chan will fail
        do run_in_newsched_task {
            let res = do spawntask_try {
                let (port, chan) = oneshot::<~int>();
                { let _c = chan; }
                port.recv();
            };
            assert!(res.is_err());
        }
    }

    #[test]
    fn oneshot_single_thread_send_then_recv() {
        do run_in_newsched_task {
            let (port, chan) = oneshot::<~int>();
            chan.send(~10);
            assert!(port.recv() == ~10);
        }
    }

    #[test]
    fn oneshot_single_thread_try_send_open() {
        do run_in_newsched_task {
            let (port, chan) = oneshot::<int>();
            assert!(chan.try_send(10));
            assert!(port.recv() == 10);
        }
    }

    #[test]
    fn oneshot_single_thread_try_send_closed() {
        do run_in_newsched_task {
            let (port, chan) = oneshot::<int>();
            { let _p = port; }
            assert!(!chan.try_send(10));
        }
    }

    #[test]
    fn oneshot_single_thread_try_recv_open() {
        do run_in_newsched_task {
            let (port, chan) = oneshot::<int>();
            chan.send(10);
            assert!(port.try_recv() == Some(10));
        }
    }

    #[test]
    fn oneshot_single_thread_try_recv_closed() {
        do run_in_newsched_task {
            let (port, chan) = oneshot::<int>();
            { let _c = chan; }
            assert!(port.try_recv() == None);
        }
    }

    #[test]
    fn oneshot_single_thread_peek_data() {
        do run_in_newsched_task {
            let (port, chan) = oneshot::<int>();
            assert!(!port.peek());
            chan.send(10);
            assert!(port.peek());
        }
    }

    #[test]
    fn oneshot_single_thread_peek_close() {
        do run_in_newsched_task {
            let (port, chan) = oneshot::<int>();
            { let _c = chan; }
            assert!(!port.peek());
            assert!(!port.peek());
        }
    }

    #[test]
    fn oneshot_single_thread_peek_open() {
        do run_in_newsched_task {
            let (port, chan) = oneshot::<int>();
            assert!(!port.peek());
        }
    }

    #[test]
    fn oneshot_multi_task_recv_then_send() {
        do run_in_newsched_task {
            let (port, chan) = oneshot::<~int>();
            let port_cell = Cell(port);
            do spawntask_immediately {
                assert!(port_cell.take().recv() == ~10);
            }

            chan.send(~10);
        }
    }

    #[test]
    fn oneshot_multi_task_recv_then_close() {
        do run_in_newsched_task {
            let (port, chan) = oneshot::<~int>();
            let port_cell = Cell(port);
            let chan_cell = Cell(chan);
            do spawntask_later {
                let _cell = chan_cell.take();
            }
            let res = do spawntask_try {
                assert!(port_cell.take().recv() == ~10);
            };
            assert!(res.is_err());
        }
    }

    #[test]
    fn oneshot_multi_thread_close_stress() {
        for stress_factor().times {
            do run_in_newsched_task {
                let (port, chan) = oneshot::<int>();
                let port_cell = Cell(port);
                let _thread = do spawntask_thread {
                    let _p = port_cell.take();
                };
                let _chan = chan;
            }
        }
    }

    #[test]
    fn oneshot_multi_thread_send_close_stress() {
        for stress_factor().times {
            do run_in_newsched_task {
                let (port, chan) = oneshot::<int>();
                let chan_cell = Cell(chan);
                let port_cell = Cell(port);
                let _thread1 = do spawntask_thread {
                    let _p = port_cell.take();
                };
                let _thread2 = do spawntask_thread {
                    let c = chan_cell.take();
                    c.send(1);
                };
            }
        }
    }

    #[test]
    fn oneshot_multi_thread_recv_close_stress() {
        for stress_factor().times {
            do run_in_newsched_task {
                let (port, chan) = oneshot::<int>();
                let chan_cell = Cell(chan);
                let port_cell = Cell(port);
                let _thread1 = do spawntask_thread {
                    let port_cell = Cell(port_cell.take());
                    let res = do spawntask_try {
                        port_cell.take().recv();
                    };
                    assert!(res.is_err());
                };
                let _thread2 = do spawntask_thread {
                    let chan_cell = Cell(chan_cell.take());
                    do spawntask {
                        chan_cell.take();
                    }
                };
            }
        }
    }

    #[test]
    fn oneshot_multi_thread_send_recv_stress() {
        for stress_factor().times {
            do run_in_newsched_task {
                let (port, chan) = oneshot::<~int>();
                let chan_cell = Cell(chan);
                let port_cell = Cell(port);
                let _thread1 = do spawntask_thread {
                    chan_cell.take().send(~10);
                };
                let _thread2 = do spawntask_thread {
                    assert!(port_cell.take().recv() == ~10);
                };
            }
        }
    }

    #[test]
    fn stream_send_recv() {
        for stress_factor().times {
            do run_in_newsched_task {
                let (port, chan) = stream::<~int>();

                send(chan, 0);
                recv(port, 0);

                fn send(chan: Chan<~int>, i: int) {
                    if i == 10 { return }

                    let chan_cell = Cell(chan);
                    let _thread = do spawntask_thread {
                        let chan = chan_cell.take();
                        chan.send(~i);
                        send(chan, i + 1);
                    };
                }

                fn recv(port: Port<~int>, i: int) {
                    if i == 10 { return }

                    let port_cell = Cell(port);
                    let _thread = do spawntask_thread {
                        let port = port_cell.take();
                        assert!(port.recv() == ~i);
                        recv(port, i + 1);
                    };
                }
            }
        }
    }
}

