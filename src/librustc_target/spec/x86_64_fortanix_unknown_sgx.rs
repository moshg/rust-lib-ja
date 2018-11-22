// Copyright 2018 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::iter;

use super::{LinkerFlavor, Target, TargetOptions, PanicStrategy};

pub fn target() -> Result<Target, String> {
    const PRE_LINK_ARGS: &[&str] = &[
        "-Wl,--as-needed",
        "-Wl,-z,noexecstack",
        "-m64",
         "-fuse-ld=gold",
         "-nostdlib",
         "-shared",
         "-Wl,-e,sgx_entry",
         "-Wl,-Bstatic",
         "-Wl,--gc-sections",
         "-Wl,-z,text",
         "-Wl,-z,norelro",
         "-Wl,--rosegment",
         "-Wl,--no-undefined",
         "-Wl,--error-unresolved-symbols",
         "-Wl,--no-undefined-version",
         "-Wl,-Bsymbolic",
         "-Wl,--export-dynamic",
    ];
    const EXPORT_SYMBOLS: &[&str] = &[
        "sgx_entry",
        "HEAP_BASE",
        "HEAP_SIZE",
        "RELA",
        "RELACOUNT",
        "ENCLAVE_SIZE",
        "CFGDATA_BASE",
        "DEBUG",
    ];
    let opts = TargetOptions {
        dynamic_linking: false,
        executables: true,
        linker_is_gnu: true,
        max_atomic_width: Some(64),
        panic_strategy: PanicStrategy::Abort,
        cpu: "x86-64".into(),
        position_independent_executables: true,
        pre_link_args: iter::once(
                (LinkerFlavor::Gcc, PRE_LINK_ARGS.iter().cloned().map(String::from).collect())
        ).collect(),
        override_export_symbols: Some(EXPORT_SYMBOLS.iter().cloned().map(String::from).collect()),
        ..Default::default()
    };
    Ok(Target {
        llvm_target: "x86_64-unknown-linux-gnu".into(),
        target_endian: "little".into(),
        target_pointer_width: "64".into(),
        target_c_int_width: "32".into(),
        target_os: "unknown".into(),
        target_env: "sgx".into(),
        target_vendor: "fortanix".into(),
        data_layout: "e-m:e-i64:64-f80:128-n8:16:32:64-S128".into(),
        arch: "x86_64".into(),
        linker_flavor: LinkerFlavor::Gcc,
        options: opts,
    })
}
