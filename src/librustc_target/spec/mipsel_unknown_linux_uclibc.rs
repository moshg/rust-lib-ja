use crate::spec::{LinkerFlavor, Target, TargetOptions, TargetResult};

pub fn target() -> TargetResult {
    Ok(Target {
        llvm_target: "mipsel-unknown-linux-uclibc".to_string(),
        target_endian: "little".to_string(),
        target_pointer_width: "32".to_string(),
        target_c_int_width: "32".to_string(),
        data_layout: "e-m:m-p:32:32-i8:8:32-i16:16:32-i64:64-n32-S64".to_string(),
        arch: "mips".to_string(),
        target_os: "linux".to_string(),
        target_env: "uclibc".to_string(),
        target_vendor: "unknown".to_string(),
        linker_flavor: LinkerFlavor::Gcc,

        options: TargetOptions {
            cpu: "mips32r2".to_string(),
            features: "+mips32r2,+soft-float".to_string(),
            max_atomic_width: Some(32),

            ..super::linux_base::opts()
        },
    })
}
