// Copyright 2016 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use spec::{LinkerFlavor, Target, TargetOptions, TargetResult};

// This target is for glibc Linux on ARMv7 with thumb mode enabled
// (for consistency with Android and Debian-based distributions)
// and with NEON unconditionally enabled and, therefore, with 32 FPU
// registers enabled as well. See section A2.6.2 on page A2-56 in
// https://static.docs.arm.com/ddi0406/cd/DDI0406C_d_armv7ar_arm.pdf

pub fn target() -> TargetResult {
    let base = super::linux_base::opts();
    Ok(Target {
        llvm_target: "armv7-unknown-linux-gnueabihf".to_string(),
        target_endian: "little".to_string(),
        target_pointer_width: "32".to_string(),
        target_c_int_width: "32".to_string(),
        data_layout: "e-m:e-p:32:32-i64:64-v128:64:128-a:0:32-n32-S64".to_string(),
        arch: "arm".to_string(),
        target_os: "linux".to_string(),
        target_env: "gnu".to_string(),
        target_vendor: "unknown".to_string(),
        linker_flavor: LinkerFlavor::Gcc,

        options: TargetOptions {
            // Info about features at https://wiki.debian.org/ArmHardFloatPort
            features: "+v7,+thumb-mode,+thumb2,+vfp3,+neon".to_string(),
            cpu: "generic".to_string(),
            max_atomic_width: Some(64),
            abi_blacklist: super::arm_base::abi_blacklist(),
            .. base
        }
    })
}
