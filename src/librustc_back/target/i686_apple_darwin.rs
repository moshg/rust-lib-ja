// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use target::Target;

pub fn target() -> Target {
    let mut base = super::apple_base::opts();
    base.cpu = "yonah".to_string();
    base.max_atomic_width = 64;
    base.pre_link_args.push("-m32".to_string());

    Target {
        llvm_target: "i686-apple-darwin".to_string(),
        target_endian: "little".to_string(),
        target_pointer_width: "32".to_string(),
        data_layout: "e-m:o-p:32:32-f64:32:64-f80:128-n8:16:32-S128".to_string(),
        arch: "x86".to_string(),
        target_os: "macos".to_string(),
        target_env: "".to_string(),
        target_vendor: "apple".to_string(),
        options: base,
    }
}
