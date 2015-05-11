# x86_64-pc-windows-msvc configuration
CC_x86_64-pc-windows-msvc="$(CFG_MSVC_CL)" -nologo
LINK_x86_64-pc-windows-msvc="$(CFG_MSVC_LINK)" -nologo
CXX_x86_64-pc-windows-msvc="$(CFG_MSVC_CL)" -nologo
CPP_x86_64-pc-windows-msvc="$(CFG_MSVC_CL)" -nologo
AR_x86_64-pc-windows-msvc="$(CFG_MSVC_LIB)" -nologo
CFG_LIB_NAME_x86_64-pc-windows-msvc=$(1).dll
CFG_STATIC_LIB_NAME_x86_64-pc-windows-msvc=$(1).lib
CFG_LIB_GLOB_x86_64-pc-windows-msvc=$(1)-*.dll
CFG_LIB_DSYM_GLOB_x86_64-pc-windows-msvc=$(1)-*.dylib.dSYM
CFG_JEMALLOC_CFLAGS_x86_64-pc-windows-msvc :=
CFG_GCCISH_CFLAGS_x86_64-pc-windows-msvc :=
CFG_GCCISH_CXXFLAGS_x86_64-pc-windows-msvc :=
CFG_GCCISH_LINK_FLAGS_x86_64-pc-windows-msvc :=
CFG_GCCISH_DEF_FLAG_x86_64-pc-windows-msvc :=
CFG_LLC_FLAGS_x86_64-pc-windows-msvc :=
CFG_INSTALL_NAME_x86_64-pc-windows-msvc =
CFG_EXE_SUFFIX_x86_64-pc-windows-msvc := .exe
CFG_WINDOWSY_x86_64-pc-windows-msvc := 1
CFG_UNIXY_x86_64-pc-windows-msvc :=
CFG_LDPATH_x86_64-pc-windows-msvc :=
CFG_RUN_x86_64-pc-windows-msvc=$(2)
CFG_RUN_TARG_x86_64-pc-windows-msvc=$(call CFG_RUN_x86_64-pc-windows-msvc,,$(2))
CFG_GNU_TRIPLE_x86_64-pc-windows-msvc := x86_64-pc-win32

# These two environment variables are scraped by the `./configure` script and
# are necessary for `cl.exe` to find standard headers (the INCLUDE variable) and
# for `link.exe` to find standard libraries (the LIB variable).
ifdef CFG_MSVC_INCLUDE_PATH
export INCLUDE := $(CFG_MSVC_INCLUDE_PATH)
endif
ifdef CFG_MSVC_LIB_PATH
export LIB := $(CFG_MSVC_LIB_PATH)
endif

# Unfortunately `link.exe` is also a program in `/usr/bin` on MinGW installs,
# but it's not the one that we want. As a result we make sure that our detected
# `link.exe` shows up in PATH first.
ifdef CFG_MSVC_LINK
export PATH := $(CFG_MSVC_ROOT)/VC/bin/amd64:$(PATH)
endif

# There are more comments about this available in the target specification for
# Windows MSVC in the compiler, but the gist of it is that we use `llvm-ar.exe`
# instead of `lib.exe` for assembling archives, so we need to inject this custom
# dependency here.
NATIVE_TOOL_DEPS_core_T_x86_64-pc-windows-msvc += llvm-ar.exe

# When working with MSVC on windows, each DLL needs to explicitly declare its
# interface to the outside world through some means. The options for doing so
# include:
#
# 1. A custom attribute on each function itself
# 2. A linker argument saying what to export
# 3. A file which lists all symbols that need to be exported
#
# The Rust compiler takes care (1) for us for all Rust code by annotating all
# public-facing functions with dllexport, but we have a few native dependencies
# which need to cross the DLL boundary. The most important of these dependencies
# is LLVM which is linked into `rustc_llvm.dll` but primarily used from
# `rustc_trans.dll`. This means that many of LLVM's C API functions need to be
# exposed from `rustc_llvm.dll` to be forwarded over the boundary.
#
# Unfortunately, at this time, LLVM does not handle this sort of exportation on
# Windows for us, so we're forced to do it ourselves if we want it (which seems
# like the path of least resistance right now). To do this we generate a `.DEF`
# file [1] which we then custom-pass to the linker when building the rustc_llvm
# crate. This DEF file list all symbols that are exported from
# `src/librustc_llvm/lib.rs` and is generated by a small python script.
#
# Fun times!
#
# [1]: https://msdn.microsoft.com/en-us/library/28d6s79h.aspx
RUSTFLAGS_rustc_llvm_T_x86_64-pc-windows-msvc += \
	-C link-args="-DEF:x86_64-pc-windows-msvc/rt/rustc_llvm.def"
CUSTOM_DEPS_rustc_llvm_T_x86_64-pc-windows-msvc += \
	x86_64-pc-windows-msvc/rt/rustc_llvm.def

x86_64-pc-windows-msvc/rt/rustc_llvm.def: $(S)src/etc/mklldef.py \
			$(S)src/librustc_llvm/lib.rs
	$(CFG_PYTHON) $^ $@ rustc_llvm-$(CFG_FILENAME_EXTRA)
