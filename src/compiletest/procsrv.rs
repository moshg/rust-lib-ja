// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use core::prelude::*;

use core::comm;
use core::io;
use core::libc::c_int;
use core::os;
use core::run;
use core::str;
use core::task;
use core::vec;

#[cfg(target_os = "win32")]
fn target_env(lib_path: &str, prog: &str) -> ~[(~str,~str)] {

    let mut env = os::env();

    // Make sure we include the aux directory in the path
    assert!(prog.ends_with(".exe"));
    let aux_path = prog.slice(0u, prog.len() - 4u).to_owned() + ".libaux";

    env = do vec::map(env) |pair| {
        let (k,v) = copy *pair;
        if k == ~"PATH" { (~"PATH", v + ";" + lib_path + ";" + aux_path) }
        else { (k,v) }
    };
    if str::ends_with(prog, "rustc.exe") {
        env.push((~"RUST_THREADS", ~"1"));
    }
    return env;
}

#[cfg(target_os = "linux")]
#[cfg(target_os = "macos")]
#[cfg(target_os = "freebsd")]
fn target_env(_lib_path: &str, _prog: &str) -> ~[(~str,~str)] {
    os::env()
}

pub struct Result {status: int, out: ~str, err: ~str}

pub fn run(lib_path: &str,
           prog: &str,
           args: &[~str],
           env: ~[(~str, ~str)],
           input: Option<~str>) -> Result {

    let env = env + target_env(lib_path, prog);
    let mut proc = run::Process::new(prog, args, run::ProcessOptions {
        env: Some(env.slice(0, env.len())),
        dir: None,
        in_fd: None,
        out_fd: None,
        err_fd: None
    });

    for input.each |input| {
        proc.input().write_str(*input);
    }
    let output = proc.finish_with_output();

    Result {
        status: output.status,
        out: str::from_bytes(output.output),
        err: str::from_bytes(output.error)
    }
}

