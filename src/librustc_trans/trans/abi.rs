// Copyright 2012-2016 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use llvm::{self, ValueRef};
use trans::common::{return_type_is_void, type_is_fat_ptr};
use trans::context::CrateContext;
use trans::cabi_x86;
use trans::cabi_x86_64;
use trans::cabi_x86_win64;
use trans::cabi_arm;
use trans::cabi_aarch64;
use trans::cabi_powerpc;
use trans::cabi_powerpc64;
use trans::cabi_mips;
use trans::cabi_asmjs;
use trans::machine::{llsize_of_alloc, llsize_of_real};
use trans::type_::Type;
use trans::type_of;

use rustc_front::hir;
use middle::ty::{self, Ty};

pub use syntax::abi::Abi;

/// The first half of a fat pointer.
/// - For a closure, this is the code address.
/// - For an object or trait instance, this is the address of the box.
/// - For a slice, this is the base address.
pub const FAT_PTR_ADDR: usize = 0;

/// The second half of a fat pointer.
/// - For a closure, this is the address of the environment.
/// - For an object or trait instance, this is the address of the vtable.
/// - For a slice, this is the length.
pub const FAT_PTR_EXTRA: usize = 1;

#[derive(Clone, Copy, PartialEq, Debug)]
enum ArgKind {
    /// Pass the argument directly using the normal converted
    /// LLVM type or by coercing to another specified type
    Direct,
    /// Pass the argument indirectly via a hidden pointer
    Indirect,
    /// Ignore the argument (useful for empty struct)
    Ignore,
}

/// Information about how a specific C type
/// should be passed to or returned from a function
///
/// This is borrowed from clang's ABIInfo.h
#[derive(Clone, Copy, Debug)]
pub struct ArgType {
    kind: ArgKind,
    /// Original LLVM type
    pub original_ty: Type,
    /// Sizing LLVM type (pointers are opaque).
    /// Unlike original_ty, this is guaranteed to be complete.
    ///
    /// For example, while we're computing the function pointer type in
    /// `struct Foo(fn(Foo));`, `original_ty` is still LLVM's `%Foo = {}`.
    /// The field type will likely end up being `void(%Foo)*`, but we cannot
    /// use `%Foo` to compute properties (e.g. size and alignment) of `Foo`,
    /// until `%Foo` is completed by having all of its field types inserted,
    /// so `ty` holds the "sizing type" of `Foo`, which replaces all pointers
    /// with opaque ones, resulting in `{i8*}` for `Foo`.
    /// ABI-specific logic can then look at the size, alignment and fields of
    /// `{i8*}` in order to determine how the argument will be passed.
    /// Only later will `original_ty` aka `%Foo` be used in the LLVM function
    /// pointer type, without ever having introspected it.
    pub ty: Type,
    /// Coerced LLVM Type
    pub cast: Option<Type>,
    /// Dummy argument, which is emitted before the real argument
    pub pad: Option<Type>,
    /// LLVM attributes of argument
    pub attrs: llvm::Attributes
}

impl ArgType {
    fn new(original_ty: Type, ty: Type) -> ArgType {
        ArgType {
            kind: ArgKind::Direct,
            original_ty: original_ty,
            ty: ty,
            cast: None,
            pad: None,
            attrs: llvm::Attributes::default()
        }
    }

    pub fn make_indirect(&mut self, ccx: &CrateContext) {
        // Wipe old attributes, likely not valid through indirection.
        self.attrs = llvm::Attributes::default();

        let llarg_sz = llsize_of_real(ccx, self.ty);

        // For non-immediate arguments the callee gets its own copy of
        // the value on the stack, so there are no aliases. It's also
        // program-invisible so can't possibly capture
        self.attrs.set(llvm::Attribute::NoAlias)
                  .set(llvm::Attribute::NoCapture)
                  .set_dereferenceable(llarg_sz);

        self.kind = ArgKind::Indirect;
    }

    pub fn ignore(&mut self) {
        self.kind = ArgKind::Ignore;
    }

    pub fn is_indirect(&self) -> bool {
        self.kind == ArgKind::Indirect
    }

    pub fn is_ignore(&self) -> bool {
        self.kind == ArgKind::Ignore
    }
}

/// Metadata describing how the arguments to a native function
/// should be passed in order to respect the native ABI.
///
/// I will do my best to describe this structure, but these
/// comments are reverse-engineered and may be inaccurate. -NDM
pub struct FnType {
    /// The LLVM types of each argument.
    pub args: Vec<ArgType>,

    /// LLVM return type.
    pub ret: ArgType,

    pub variadic: bool,

    pub cconv: llvm::CallConv
}

impl FnType {
    pub fn new<'a, 'tcx>(ccx: &CrateContext<'a, 'tcx>,
                         abi: Abi,
                         sig: &ty::FnSig<'tcx>,
                         extra_args: &[Ty<'tcx>]) -> FnType {
        use self::Abi::*;
        let cconv = match ccx.sess().target.target.adjust_abi(abi) {
            RustIntrinsic => {
                // Intrinsics are emitted at the call site
                ccx.sess().bug("asked to compute FnType of intrinsic");
            }
            PlatformIntrinsic => {
                // Intrinsics are emitted at the call site
                ccx.sess().bug("asked to compute FnType of platform intrinsic");
            }

            Rust | RustCall => llvm::CCallConv,

            // It's the ABI's job to select this, not us.
            System => ccx.sess().bug("system abi should be selected elsewhere"),

            Stdcall => llvm::X86StdcallCallConv,
            Fastcall => llvm::X86FastcallCallConv,
            Vectorcall => llvm::X86_VectorCall,
            C => llvm::CCallConv,
            Win64 => llvm::X86_64_Win64,

            // These API constants ought to be more specific...
            Cdecl => llvm::CCallConv,
            Aapcs => llvm::CCallConv,
        };

        let mut inputs = &sig.inputs[..];
        let extra_args = if abi == RustCall {
            assert!(!sig.variadic && extra_args.is_empty());

            match inputs[inputs.len() - 1].sty {
                ty::TyTuple(ref tupled_arguments) => {
                    inputs = &inputs[..inputs.len() - 1];
                    &tupled_arguments[..]
                }
                _ => {
                    unreachable!("argument to function with \"rust-call\" ABI \
                                  is not a tuple");
                }
            }
        } else {
            assert!(sig.variadic || extra_args.is_empty());
            extra_args
        };

        let arg_of = |ty: Ty<'tcx>| {
            if ty.is_bool() {
                let llty = Type::i1(ccx);
                let mut arg = ArgType::new(llty, llty);
                arg.attrs.set(llvm::Attribute::ZExt);
                arg
            } else {
                ArgType::new(type_of::type_of(ccx, ty),
                             type_of::sizing_type_of(ccx, ty))
            }
        };

        let mut ret = match sig.output {
            ty::FnConverging(ret_ty) if !return_type_is_void(ccx, ret_ty) => {
                arg_of(ret_ty)
            }
            _ => ArgType::new(Type::void(ccx), Type::void(ccx))
        };

        if let ty::FnConverging(ret_ty) = sig.output {
            if !type_is_fat_ptr(ccx.tcx(), ret_ty) {
                // The `noalias` attribute on the return value is useful to a
                // function ptr caller.
                if let ty::TyBox(_) = ret_ty.sty {
                    // `Box` pointer return values never alias because ownership
                    // is transferred
                    ret.attrs.set(llvm::Attribute::NoAlias);
                }

                // We can also mark the return value as `dereferenceable` in certain cases
                match ret_ty.sty {
                    // These are not really pointers but pairs, (pointer, len)
                    ty::TyRef(_, ty::TypeAndMut { ty, .. }) |
                    ty::TyBox(ty) => {
                        let llty = type_of::sizing_type_of(ccx, ty);
                        let llsz = llsize_of_real(ccx, llty);
                        ret.attrs.set_dereferenceable(llsz);
                    }
                    _ => {}
                }
            }
        }

        let mut args = Vec::with_capacity(inputs.len() + extra_args.len());

        // Handle safe Rust thin and fat pointers.
        let rust_ptr_attrs = |ty: Ty<'tcx>, arg: &mut ArgType| match ty.sty {
            // `Box` pointer parameters never alias because ownership is transferred
            ty::TyBox(inner) => {
                arg.attrs.set(llvm::Attribute::NoAlias);
                Some(inner)
            }

            ty::TyRef(b, mt) => {
                use middle::ty::{BrAnon, ReLateBound};

                // `&mut` pointer parameters never alias other parameters, or mutable global data
                //
                // `&T` where `T` contains no `UnsafeCell<U>` is immutable, and can be marked as
                // both `readonly` and `noalias`, as LLVM's definition of `noalias` is based solely
                // on memory dependencies rather than pointer equality
                let interior_unsafe = mt.ty.type_contents(ccx.tcx()).interior_unsafe();

                if mt.mutbl != hir::MutMutable && !interior_unsafe {
                    arg.attrs.set(llvm::Attribute::NoAlias);
                }

                if mt.mutbl == hir::MutImmutable && !interior_unsafe {
                    arg.attrs.set(llvm::Attribute::ReadOnly);
                }

                // When a reference in an argument has no named lifetime, it's
                // impossible for that reference to escape this function
                // (returned or stored beyond the call by a closure).
                if let ReLateBound(_, BrAnon(_)) = *b {
                    arg.attrs.set(llvm::Attribute::NoCapture);
                }

                Some(mt.ty)
            }
            _ => None
        };

        for ty in inputs.iter().chain(extra_args.iter()) {
            let mut arg = arg_of(ty);

            if type_is_fat_ptr(ccx.tcx(), ty) {
                let original_tys = arg.original_ty.field_types();
                let sizing_tys = arg.ty.field_types();
                assert_eq!((original_tys.len(), sizing_tys.len()), (2, 2));

                let mut data = ArgType::new(original_tys[0], sizing_tys[0]);
                let mut info = ArgType::new(original_tys[1], sizing_tys[1]);

                if let Some(inner) = rust_ptr_attrs(ty, &mut data) {
                    data.attrs.set(llvm::Attribute::NonNull);
                    if ccx.tcx().struct_tail(inner).is_trait() {
                        info.attrs.set(llvm::Attribute::NonNull);
                    }
                }
                args.push(data);
                args.push(info);
            } else {
                if let Some(inner) = rust_ptr_attrs(ty, &mut arg) {
                    let llty = type_of::sizing_type_of(ccx, inner);
                    let llsz = llsize_of_real(ccx, llty);
                    arg.attrs.set_dereferenceable(llsz);
                }
                args.push(arg);
            }
        }

        let mut fty = FnType {
            args: args,
            ret: ret,
            variadic: sig.variadic,
            cconv: cconv
        };

        if abi == Rust || abi == RustCall {
            let fixup = |arg: &mut ArgType| {
                if !arg.ty.is_aggregate() {
                    // Scalars and vectors, always immediate.
                    return;
                }
                let size = llsize_of_alloc(ccx, arg.ty);
                if size > llsize_of_alloc(ccx, ccx.int_type()) {
                    arg.make_indirect(ccx);
                } else if size > 0 {
                    // We want to pass small aggregates as immediates, but using
                    // a LLVM aggregate type for this leads to bad optimizations,
                    // so we pick an appropriately sized integer type instead.
                    arg.cast = Some(Type::ix(ccx, size * 8));
                }
            };
            if fty.ret.ty != Type::void(ccx) {
                // Fat pointers are returned by-value.
                if !type_is_fat_ptr(ccx.tcx(), sig.output.unwrap()) {
                    fixup(&mut fty.ret);
                }
            }
            for arg in &mut fty.args {
                fixup(arg);
            }
            if fty.ret.is_indirect() {
                fty.ret.attrs.set(llvm::Attribute::StructRet);
            }
            return fty;
        }

        match &ccx.sess().target.target.arch[..] {
            "x86" => cabi_x86::compute_abi_info(ccx, &mut fty),
            "x86_64" => if ccx.sess().target.target.options.is_like_windows {
                cabi_x86_win64::compute_abi_info(ccx, &mut fty);
            } else {
                cabi_x86_64::compute_abi_info(ccx, &mut fty);
            },
            "aarch64" => cabi_aarch64::compute_abi_info(ccx, &mut fty),
            "arm" => {
                let flavor = if ccx.sess().target.target.target_os == "ios" {
                    cabi_arm::Flavor::Ios
                } else {
                    cabi_arm::Flavor::General
                };
                cabi_arm::compute_abi_info(ccx, &mut fty, flavor);
            },
            "mips" => cabi_mips::compute_abi_info(ccx, &mut fty),
            "powerpc" => cabi_powerpc::compute_abi_info(ccx, &mut fty),
            "powerpc64" => cabi_powerpc64::compute_abi_info(ccx, &mut fty),
            "asmjs" => cabi_asmjs::compute_abi_info(ccx, &mut fty),
            a => ccx.sess().fatal(&format!("unrecognized arch \"{}\" in target specification", a))
        }

        if fty.ret.is_indirect() {
            fty.ret.attrs.set(llvm::Attribute::StructRet);
        }

        fty
    }

    pub fn llvm_type(&self, ccx: &CrateContext) -> Type {
        let mut llargument_tys = Vec::new();

        let llreturn_ty = if self.ret.is_indirect() {
            llargument_tys.push(self.ret.original_ty.ptr_to());
            Type::void(ccx)
        } else {
            self.ret.cast.unwrap_or(self.ret.original_ty)
        };

        for arg in &self.args {
            if arg.is_ignore() {
                continue;
            }
            // add padding
            if let Some(ty) = arg.pad {
                llargument_tys.push(ty);
            }

            let llarg_ty = if arg.is_indirect() {
                arg.original_ty.ptr_to()
            } else {
                arg.cast.unwrap_or(arg.original_ty)
            };

            llargument_tys.push(llarg_ty);
        }

        if self.variadic {
            Type::variadic_func(&llargument_tys, &llreturn_ty)
        } else {
            Type::func(&llargument_tys, &llreturn_ty)
        }
    }

    pub fn apply_attrs_llfn(&self, llfn: ValueRef) {
        let mut i = if self.ret.is_indirect() { 1 } else { 0 };
        self.ret.attrs.apply_llfn(i, llfn);
        i += 1;
        for arg in &self.args {
            if !arg.is_ignore() {
                if arg.pad.is_some() { i += 1; }
                arg.attrs.apply_llfn(i, llfn);
                i += 1;
            }
        }
    }

    pub fn apply_attrs_callsite(&self, callsite: ValueRef) {
        let mut i = if self.ret.is_indirect() { 1 } else { 0 };
        self.ret.attrs.apply_callsite(i, callsite);
        i += 1;
        for arg in &self.args {
            if !arg.is_ignore() {
                if arg.pad.is_some() { i += 1; }
                arg.attrs.apply_callsite(i, callsite);
                i += 1;
            }
        }
    }
}
