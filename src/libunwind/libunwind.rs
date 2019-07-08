#![allow(nonstandard_style)]

use libc::{c_int, c_void, uintptr_t};

#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum _Unwind_Reason_Code {
    _URC_NO_REASON = 0,
    _URC_FOREIGN_EXCEPTION_CAUGHT = 1,
    _URC_FATAL_PHASE2_ERROR = 2,
    _URC_FATAL_PHASE1_ERROR = 3,
    _URC_NORMAL_STOP = 4,
    _URC_END_OF_STACK = 5,
    _URC_HANDLER_FOUND = 6,
    _URC_INSTALL_CONTEXT = 7,
    _URC_CONTINUE_UNWIND = 8,
    _URC_FAILURE = 9, // used only by ARM EHABI
}
pub use _Unwind_Reason_Code::*;

pub type _Unwind_Exception_Class = u64;
pub type _Unwind_Word = uintptr_t;
pub type _Unwind_Ptr = uintptr_t;
pub type _Unwind_Trace_Fn = extern "C" fn(ctx: *mut _Unwind_Context, arg: *mut c_void)
                                          -> _Unwind_Reason_Code;
#[cfg(target_arch = "x86")]
pub const unwinder_private_data_size: usize = 5;

#[cfg(target_arch = "x86_64")]
pub const unwinder_private_data_size: usize = 6;

#[cfg(all(target_arch = "arm", not(target_os = "ios")))]
pub const unwinder_private_data_size: usize = 20;

#[cfg(all(target_arch = "arm", target_os = "ios"))]
pub const unwinder_private_data_size: usize = 5;

#[cfg(target_arch = "aarch64")]
pub const unwinder_private_data_size: usize = 2;

#[cfg(target_arch = "mips")]
pub const unwinder_private_data_size: usize = 2;

#[cfg(target_arch = "mips64")]
pub const unwinder_private_data_size: usize = 2;

#[cfg(any(target_arch = "powerpc", target_arch = "powerpc64"))]
pub const unwinder_private_data_size: usize = 2;

#[cfg(target_arch = "s390x")]
pub const unwinder_private_data_size: usize = 2;

#[cfg(target_arch = "sparc64")]
pub const unwinder_private_data_size: usize = 2;

#[cfg(target_os = "emscripten")]
pub const unwinder_private_data_size: usize = 20;

#[repr(C)]
pub struct _Unwind_Exception {
    pub exception_class: _Unwind_Exception_Class,
    pub exception_cleanup: _Unwind_Exception_Cleanup_Fn,
    pub private: [_Unwind_Word; unwinder_private_data_size],
}

pub enum _Unwind_Context {}

pub type _Unwind_Exception_Cleanup_Fn = extern "C" fn(unwind_code: _Unwind_Reason_Code,
                                                      exception: *mut _Unwind_Exception);
#[cfg_attr(all(not(bootstrap), feature = "llvm-libunwind",
               any(target_os = "fuchsia", target_os = "linux")),
           link(name = "unwind", kind = "static"))]
extern "C" {
    #[unwind(allowed)]
    pub fn _Unwind_Resume(exception: *mut _Unwind_Exception) -> !;
    pub fn _Unwind_DeleteException(exception: *mut _Unwind_Exception);
    pub fn _Unwind_GetLanguageSpecificData(ctx: *mut _Unwind_Context) -> *mut c_void;
    pub fn _Unwind_GetRegionStart(ctx: *mut _Unwind_Context) -> _Unwind_Ptr;
    pub fn _Unwind_GetTextRelBase(ctx: *mut _Unwind_Context) -> _Unwind_Ptr;
    pub fn _Unwind_GetDataRelBase(ctx: *mut _Unwind_Context) -> _Unwind_Ptr;
}

cfg_if::cfg_if! {
if #[cfg(all(any(target_os = "ios", target_os = "netbsd", not(target_arch = "arm"))))] {
    // Not ARM EHABI
    #[repr(C)]
    #[derive(Copy, Clone, PartialEq)]
    pub enum _Unwind_Action {
        _UA_SEARCH_PHASE = 1,
        _UA_CLEANUP_PHASE = 2,
        _UA_HANDLER_FRAME = 4,
        _UA_FORCE_UNWIND = 8,
        _UA_END_OF_STACK = 16,
    }
    pub use _Unwind_Action::*;

    #[cfg_attr(all(not(bootstrap), feature = "llvm-libunwind",
                   any(target_os = "fuchsia", target_os = "linux")),
               link(name = "unwind", kind = "static"))]
    extern "C" {
        pub fn _Unwind_GetGR(ctx: *mut _Unwind_Context, reg_index: c_int) -> _Unwind_Word;
        pub fn _Unwind_SetGR(ctx: *mut _Unwind_Context, reg_index: c_int, value: _Unwind_Word);
        pub fn _Unwind_GetIP(ctx: *mut _Unwind_Context) -> _Unwind_Word;
        pub fn _Unwind_SetIP(ctx: *mut _Unwind_Context, value: _Unwind_Word);
        pub fn _Unwind_GetIPInfo(ctx: *mut _Unwind_Context, ip_before_insn: *mut c_int)
                                 -> _Unwind_Word;
        pub fn _Unwind_FindEnclosingFunction(pc: *mut c_void) -> *mut c_void;
    }

} else {
    // ARM EHABI
    #[repr(C)]
    #[derive(Copy, Clone, PartialEq)]
    pub enum _Unwind_State {
        _US_VIRTUAL_UNWIND_FRAME = 0,
        _US_UNWIND_FRAME_STARTING = 1,
        _US_UNWIND_FRAME_RESUME = 2,
        _US_ACTION_MASK = 3,
        _US_FORCE_UNWIND = 8,
        _US_END_OF_STACK = 16,
    }
    pub use _Unwind_State::*;

    #[repr(C)]
    enum _Unwind_VRS_Result {
        _UVRSR_OK = 0,
        _UVRSR_NOT_IMPLEMENTED = 1,
        _UVRSR_FAILED = 2,
    }
    #[repr(C)]
    enum _Unwind_VRS_RegClass {
        _UVRSC_CORE = 0,
        _UVRSC_VFP = 1,
        _UVRSC_FPA = 2,
        _UVRSC_WMMXD = 3,
        _UVRSC_WMMXC = 4,
    }
    use _Unwind_VRS_RegClass::*;
    #[repr(C)]
    enum _Unwind_VRS_DataRepresentation {
        _UVRSD_UINT32 = 0,
        _UVRSD_VFPX = 1,
        _UVRSD_FPAX = 2,
        _UVRSD_UINT64 = 3,
        _UVRSD_FLOAT = 4,
        _UVRSD_DOUBLE = 5,
    }
    use _Unwind_VRS_DataRepresentation::*;

    pub const UNWIND_POINTER_REG: c_int = 12;
    pub const UNWIND_IP_REG: c_int = 15;

    #[cfg_attr(all(not(bootstrap), feature = "llvm-libunwind",
                   any(target_os = "fuchsia", target_os = "linux")),
               link(name = "unwind", kind = "static"))]
    extern "C" {
        fn _Unwind_VRS_Get(ctx: *mut _Unwind_Context,
                           regclass: _Unwind_VRS_RegClass,
                           regno: _Unwind_Word,
                           repr: _Unwind_VRS_DataRepresentation,
                           data: *mut c_void)
                           -> _Unwind_VRS_Result;

        fn _Unwind_VRS_Set(ctx: *mut _Unwind_Context,
                           regclass: _Unwind_VRS_RegClass,
                           regno: _Unwind_Word,
                           repr: _Unwind_VRS_DataRepresentation,
                           data: *mut c_void)
                           -> _Unwind_VRS_Result;
    }

    // On Android or ARM/Linux, these are implemented as macros:

    pub unsafe fn _Unwind_GetGR(ctx: *mut _Unwind_Context, reg_index: c_int) -> _Unwind_Word {
        let mut val: _Unwind_Word = 0;
        _Unwind_VRS_Get(ctx, _UVRSC_CORE, reg_index as _Unwind_Word, _UVRSD_UINT32,
                        &mut val as *mut _ as *mut c_void);
        val
    }

    pub unsafe fn _Unwind_SetGR(ctx: *mut _Unwind_Context, reg_index: c_int, value: _Unwind_Word) {
        let mut value = value;
        _Unwind_VRS_Set(ctx, _UVRSC_CORE, reg_index as _Unwind_Word, _UVRSD_UINT32,
                        &mut value as *mut _ as *mut c_void);
    }

    pub unsafe fn _Unwind_GetIP(ctx: *mut _Unwind_Context)
                                -> _Unwind_Word {
        let val = _Unwind_GetGR(ctx, UNWIND_IP_REG);
        (val & !1) as _Unwind_Word
    }

    pub unsafe fn _Unwind_SetIP(ctx: *mut _Unwind_Context,
                                value: _Unwind_Word) {
        // Propagate thumb bit to instruction pointer
        let thumb_state = _Unwind_GetGR(ctx, UNWIND_IP_REG) & 1;
        let value = value | thumb_state;
        _Unwind_SetGR(ctx, UNWIND_IP_REG, value);
    }

    pub unsafe fn _Unwind_GetIPInfo(ctx: *mut _Unwind_Context,
                                    ip_before_insn: *mut c_int)
                                    -> _Unwind_Word {
        *ip_before_insn = 0;
        _Unwind_GetIP(ctx)
    }

    // This function also doesn't exist on Android or ARM/Linux, so make it a no-op
    pub unsafe fn _Unwind_FindEnclosingFunction(pc: *mut c_void) -> *mut c_void {
        pc
    }
}
} // cfg_if!

cfg_if::cfg_if! {
if #[cfg(not(all(target_os = "ios", target_arch = "arm")))] {
    // Not 32-bit iOS
    #[cfg_attr(all(not(bootstrap), feature = "llvm-libunwind",
                   any(target_os = "fuchsia", target_os = "linux")),
               link(name = "unwind", kind = "static"))]
    extern "C" {
        #[unwind(allowed)]
        pub fn _Unwind_RaiseException(exception: *mut _Unwind_Exception) -> _Unwind_Reason_Code;
        pub fn _Unwind_Backtrace(trace: _Unwind_Trace_Fn,
                                 trace_argument: *mut c_void)
                                 -> _Unwind_Reason_Code;
    }
} else {
    // 32-bit iOS uses SjLj and does not provide _Unwind_Backtrace()
    #[cfg_attr(all(not(bootstrap), feature = "llvm-libunwind",
                   any(target_os = "fuchsia", target_os = "linux")),
               link(name = "unwind", kind = "static"))]
    extern "C" {
        #[unwind(allowed)]
        pub fn _Unwind_SjLj_RaiseException(e: *mut _Unwind_Exception) -> _Unwind_Reason_Code;
    }

    #[inline]
    pub unsafe fn _Unwind_RaiseException(exc: *mut _Unwind_Exception) -> _Unwind_Reason_Code {
        _Unwind_SjLj_RaiseException(exc)
    }
}
} // cfg_if!
