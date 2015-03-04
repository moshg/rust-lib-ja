# x86_64-pc-windows-msvc configuration
CROSS_PREFIX_x86_64-pc-windows-msvc=
CC_x86_64-pc-windows-msvc=cl
LINK_x86_64-pc-windows-msvc=link
CXX_x86_64-pc-windows-msvc=g++
CPP_x86_64-pc-windows-msvc=gcc -E
AR_x86_64-pc-windows-msvc=llvm-ar
CFG_LIB_NAME_x86_64-pc-windows-msvc=$(1).dll
CFG_STATIC_LIB_NAME_x86_64-pc-windows-msvc=$(1).lib
CFG_LIB_GLOB_x86_64-pc-windows-msvc=$(1)-*.dll
CFG_LIB_DSYM_GLOB_x86_64-pc-windows-msvc=$(1)-*.dylib.dSYM
CFG_JEMALLOC_CFLAGS_x86_64-pc-windows-msvc := $(CFLAGS)
CFG_GCCISH_CFLAGS_x86_64-pc-windows-msvc := $(CFLAGS)
CFG_GCCISH_CXXFLAGS_x86_64-pc-windows-msvc := -fno-rtti $(CXXFLAGS)
CFG_GCCISH_LINK_FLAGS_x86_64-pc-windows-msvc := -shared -g -m64
CFG_GCCISH_DEF_FLAG_x86_64-pc-windows-msvc :=
CFG_GCCISH_PRE_LIB_FLAGS_x86_64-pc-windows-msvc :=
CFG_GCCISH_POST_LIB_FLAGS_x86_64-pc-windows-msvc :=
CFG_DEF_SUFFIX_x86_64-pc-windows-msvc := .windows.def
CFG_LLC_FLAGS_x86_64-pc-windows-msvc :=
CFG_INSTALL_NAME_x86_64-pc-windows-msvc =
CFG_EXE_SUFFIX_x86_64-pc-windows-msvc := .exe
CFG_WINDOWSY_x86_64-pc-windows-msvc := 1
CFG_UNIXY_x86_64-pc-windows-msvc :=
CFG_PATH_MUNGE_x86_64-pc-windows-msvc :=
CFG_LDPATH_x86_64-pc-windows-msvc :=
CFG_RUN_x86_64-pc-windows-msvc=$(2)
CFG_RUN_TARG_x86_64-pc-windows-msvc=$(call CFG_RUN_x86_64-pc-windows-msvc,,$(2))
CFG_GNU_TRIPLE_x86_64-pc-windows-msvc := x86_64-w64-mingw32
