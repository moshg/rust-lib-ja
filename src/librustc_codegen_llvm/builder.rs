// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use llvm::{AtomicRmwBinOp, AtomicOrdering, SynchronizationScope, AsmDialect};
use llvm::{self, False, OperandBundleDef, BasicBlock};
use common::{self, *};
use context::CodegenCx;
use type_::Type;
use type_of::LayoutLlvmExt;
use value::Value;
use libc::{c_uint, c_char};
use rustc::ty::{self, Ty, TyCtxt};
use rustc::ty::layout::{self, Align, Size, TyLayout};
use rustc::session::config;
use rustc_data_structures::small_c_str::SmallCStr;
use interfaces::*;
use syntax;
use base;
use mir::operand::{OperandValue, OperandRef};
use mir::place::PlaceRef;
use std::borrow::Cow;
use std::ops::Range;
use std::ptr;

// All Builders must have an llfn associated with them
#[must_use]
pub struct Builder<'a, 'll: 'a, 'tcx: 'll, V: 'll = &'ll Value> {
    pub llbuilder: &'ll mut llvm::Builder<'ll>,
    pub cx: &'a CodegenCx<'ll, 'tcx, V>,
}

impl<V> Drop for Builder<'a, 'll, 'tcx, V> {
    fn drop(&mut self) {
        unsafe {
            llvm::LLVMDisposeBuilder(&mut *(self.llbuilder as *mut _));
        }
    }
}

// This is a really awful way to get a zero-length c-string, but better (and a
// lot more efficient) than doing str::as_c_str("", ...) every time.
fn noname() -> *const c_char {
    static CNULL: c_char = 0;
    &CNULL
}

bitflags! {
    pub struct MemFlags: u8 {
        const VOLATILE = 1 << 0;
        const NONTEMPORAL = 1 << 1;
        const UNALIGNED = 1 << 2;
    }
}

impl BackendTypes for Builder<'_, 'll, 'tcx> {
    type Value = <CodegenCx<'ll, 'tcx> as BackendTypes>::Value;
    type BasicBlock = <CodegenCx<'ll, 'tcx> as BackendTypes>::BasicBlock;
    type Type = <CodegenCx<'ll, 'tcx> as BackendTypes>::Type;
    type Context = <CodegenCx<'ll, 'tcx> as BackendTypes>::Context;

    type DIScope = <CodegenCx<'ll, 'tcx> as BackendTypes>::DIScope;
}

impl ty::layout::HasDataLayout for Builder<'_, '_, '_> {
    fn data_layout(&self) -> &ty::layout::TargetDataLayout {
        self.cx.data_layout()
    }
}

impl ty::layout::HasTyCtxt<'tcx> for Builder<'_, '_, 'tcx> {
    fn tcx<'a>(&'a self) -> TyCtxt<'a, 'tcx, 'tcx> {
        self.cx.tcx
    }
}

impl ty::layout::LayoutOf for Builder<'_, '_, 'tcx> {
    type Ty = Ty<'tcx>;
    type TyLayout = TyLayout<'tcx>;

    fn layout_of(&self, ty: Ty<'tcx>) -> Self::TyLayout {
        self.cx.layout_of(ty)
    }
}


impl HasCodegen<'tcx> for Builder<'_, 'll, 'tcx> {
    type CodegenCx = CodegenCx<'ll, 'tcx>;
}

impl BuilderMethods<'a, 'tcx> for Builder<'a, 'll, 'tcx> {
    fn new_block<'b>(
        cx: &'a CodegenCx<'ll, 'tcx>,
        llfn: &'ll Value,
        name: &'b str
    ) -> Self {
        let bx = Builder::with_cx(cx);
        let llbb = unsafe {
            let name = SmallCStr::new(name);
            llvm::LLVMAppendBasicBlockInContext(
                cx.llcx,
                llfn,
                name.as_ptr()
            )
        };
        bx.position_at_end(llbb);
        bx
    }

    fn with_cx(cx: &'a CodegenCx<'ll, 'tcx>) -> Self {
        // Create a fresh builder from the crate context.
        let llbuilder = unsafe {
            llvm::LLVMCreateBuilderInContext(cx.llcx)
        };
        Builder {
            llbuilder,
            cx,
        }
    }

    fn build_sibling_block<'b>(&self, name: &'b str) -> Self {
        Builder::new_block(self.cx, self.llfn(), name)
    }

    fn llfn(&self) -> &'ll Value {
        unsafe {
            llvm::LLVMGetBasicBlockParent(self.llbb())
        }
    }

    fn llbb(&self) -> &'ll BasicBlock {
        unsafe {
            llvm::LLVMGetInsertBlock(self.llbuilder)
        }
    }

    fn count_insn(&self, category: &str) {
        if self.cx().sess().codegen_stats() {
            self.cx().stats.borrow_mut().n_llvm_insns += 1;
        }
        if self.cx().sess().count_llvm_insns() {
            *self.cx().stats
                      .borrow_mut()
                      .llvm_insns
                      .entry(category.to_string())
                      .or_insert(0) += 1;
        }
    }

    fn set_value_name(&self, value: &'ll Value, name: &str) {
        let cname = SmallCStr::new(name);
        unsafe {
            llvm::LLVMSetValueName(value, cname.as_ptr());
        }
    }

    fn position_at_end(&self, llbb: &'ll BasicBlock) {
        unsafe {
            llvm::LLVMPositionBuilderAtEnd(self.llbuilder, llbb);
        }
    }

    fn position_at_start(&self, llbb: &'ll BasicBlock) {
        unsafe {
            llvm::LLVMRustPositionBuilderAtStart(self.llbuilder, llbb);
        }
    }

    fn ret_void(&self) {
        self.count_insn("retvoid");
        unsafe {
            llvm::LLVMBuildRetVoid(self.llbuilder);
        }
    }

    fn ret(&self, v: &'ll Value) {
        self.count_insn("ret");
        unsafe {
            llvm::LLVMBuildRet(self.llbuilder, v);
        }
    }

    fn br(&self, dest: &'ll BasicBlock) {
        self.count_insn("br");
        unsafe {
            llvm::LLVMBuildBr(self.llbuilder, dest);
        }
    }

    fn cond_br(
        &self,
        cond: &'ll Value,
        then_llbb: &'ll BasicBlock,
        else_llbb: &'ll BasicBlock,
    ) {
        self.count_insn("condbr");
        unsafe {
            llvm::LLVMBuildCondBr(self.llbuilder, cond, then_llbb, else_llbb);
        }
    }

    fn switch(
        &self,
        v: &'ll Value,
        else_llbb: &'ll BasicBlock,
        num_cases: usize,
    ) -> &'ll Value {
        unsafe {
            llvm::LLVMBuildSwitch(self.llbuilder, v, else_llbb, num_cases as c_uint)
        }
    }

    fn invoke(&self,
                  llfn: &'ll Value,
                  args: &[&'ll Value],
                  then: &'ll BasicBlock,
                  catch: &'ll BasicBlock,
                  funclet: Option<&common::Funclet<&'ll Value>>) -> &'ll Value {
        self.count_insn("invoke");

        debug!("Invoke {:?} with args ({:?})",
               llfn,
               args);

        let args = self.check_call("invoke", llfn, args);
        let bundle = funclet.map(|funclet| funclet.bundle());
        let bundle = bundle.map(OperandBundleDef::from_generic);
        let bundle = bundle.as_ref().map(|b| &*b.raw);

        unsafe {
            llvm::LLVMRustBuildInvoke(self.llbuilder,
                                      llfn,
                                      args.as_ptr(),
                                      args.len() as c_uint,
                                      then,
                                      catch,
                                      bundle,
                                      noname())
        }
    }

    fn unreachable(&self) {
        self.count_insn("unreachable");
        unsafe {
            llvm::LLVMBuildUnreachable(self.llbuilder);
        }
    }

    /* Arithmetic */
    fn add(&self, lhs: &'ll Value, rhs: &'ll Value) -> &'ll Value {
        self.count_insn("add");
        unsafe {
            llvm::LLVMBuildAdd(self.llbuilder, lhs, rhs, noname())
        }
    }

    fn fadd(&self, lhs: &'ll Value, rhs: &'ll Value) -> &'ll Value {
        self.count_insn("fadd");
        unsafe {
            llvm::LLVMBuildFAdd(self.llbuilder, lhs, rhs, noname())
        }
    }

    fn fadd_fast(&self, lhs: &'ll Value, rhs: &'ll Value) -> &'ll Value {
        self.count_insn("fadd");
        unsafe {
            let instr = llvm::LLVMBuildFAdd(self.llbuilder, lhs, rhs, noname());
            llvm::LLVMRustSetHasUnsafeAlgebra(instr);
            instr
        }
    }

    fn sub(&self, lhs: &'ll Value, rhs: &'ll Value) -> &'ll Value {
        self.count_insn("sub");
        unsafe {
            llvm::LLVMBuildSub(self.llbuilder, lhs, rhs, noname())
        }
    }

    fn fsub(&self, lhs: &'ll Value, rhs: &'ll Value) -> &'ll Value {
        self.count_insn("fsub");
        unsafe {
            llvm::LLVMBuildFSub(self.llbuilder, lhs, rhs, noname())
        }
    }

    fn fsub_fast(&self, lhs: &'ll Value, rhs: &'ll Value) -> &'ll Value {
        self.count_insn("fsub");
        unsafe {
            let instr = llvm::LLVMBuildFSub(self.llbuilder, lhs, rhs, noname());
            llvm::LLVMRustSetHasUnsafeAlgebra(instr);
            instr
        }
    }

    fn mul(&self, lhs: &'ll Value, rhs: &'ll Value) -> &'ll Value {
        self.count_insn("mul");
        unsafe {
            llvm::LLVMBuildMul(self.llbuilder, lhs, rhs, noname())
        }
    }

    fn fmul(&self, lhs: &'ll Value, rhs: &'ll Value) -> &'ll Value {
        self.count_insn("fmul");
        unsafe {
            llvm::LLVMBuildFMul(self.llbuilder, lhs, rhs, noname())
        }
    }

    fn fmul_fast(&self, lhs: &'ll Value, rhs: &'ll Value) -> &'ll Value {
        self.count_insn("fmul");
        unsafe {
            let instr = llvm::LLVMBuildFMul(self.llbuilder, lhs, rhs, noname());
            llvm::LLVMRustSetHasUnsafeAlgebra(instr);
            instr
        }
    }


    fn udiv(&self, lhs: &'ll Value, rhs: &'ll Value) -> &'ll Value {
        self.count_insn("udiv");
        unsafe {
            llvm::LLVMBuildUDiv(self.llbuilder, lhs, rhs, noname())
        }
    }

    fn exactudiv(&self, lhs: &'ll Value, rhs: &'ll Value) -> &'ll Value {
        self.count_insn("exactudiv");
        unsafe {
            llvm::LLVMBuildExactUDiv(self.llbuilder, lhs, rhs, noname())
        }
    }

    fn sdiv(&self, lhs: &'ll Value, rhs: &'ll Value) -> &'ll Value {
        self.count_insn("sdiv");
        unsafe {
            llvm::LLVMBuildSDiv(self.llbuilder, lhs, rhs, noname())
        }
    }

    fn exactsdiv(&self, lhs: &'ll Value, rhs: &'ll Value) -> &'ll Value {
        self.count_insn("exactsdiv");
        unsafe {
            llvm::LLVMBuildExactSDiv(self.llbuilder, lhs, rhs, noname())
        }
    }

    fn fdiv(&self, lhs: &'ll Value, rhs: &'ll Value) -> &'ll Value {
        self.count_insn("fdiv");
        unsafe {
            llvm::LLVMBuildFDiv(self.llbuilder, lhs, rhs, noname())
        }
    }

    fn fdiv_fast(&self, lhs: &'ll Value, rhs: &'ll Value) -> &'ll Value {
        self.count_insn("fdiv");
        unsafe {
            let instr = llvm::LLVMBuildFDiv(self.llbuilder, lhs, rhs, noname());
            llvm::LLVMRustSetHasUnsafeAlgebra(instr);
            instr
        }
    }

    fn urem(&self, lhs: &'ll Value, rhs: &'ll Value) -> &'ll Value {
        self.count_insn("urem");
        unsafe {
            llvm::LLVMBuildURem(self.llbuilder, lhs, rhs, noname())
        }
    }

    fn srem(&self, lhs: &'ll Value, rhs: &'ll Value) -> &'ll Value {
        self.count_insn("srem");
        unsafe {
            llvm::LLVMBuildSRem(self.llbuilder, lhs, rhs, noname())
        }
    }

    fn frem(&self, lhs: &'ll Value, rhs: &'ll Value) -> &'ll Value {
        self.count_insn("frem");
        unsafe {
            llvm::LLVMBuildFRem(self.llbuilder, lhs, rhs, noname())
        }
    }

    fn frem_fast(&self, lhs: &'ll Value, rhs: &'ll Value) -> &'ll Value {
        self.count_insn("frem");
        unsafe {
            let instr = llvm::LLVMBuildFRem(self.llbuilder, lhs, rhs, noname());
            llvm::LLVMRustSetHasUnsafeAlgebra(instr);
            instr
        }
    }

    fn shl(&self, lhs: &'ll Value, rhs: &'ll Value) -> &'ll Value {
        self.count_insn("shl");
        unsafe {
            llvm::LLVMBuildShl(self.llbuilder, lhs, rhs, noname())
        }
    }

    fn lshr(&self, lhs: &'ll Value, rhs: &'ll Value) -> &'ll Value {
        self.count_insn("lshr");
        unsafe {
            llvm::LLVMBuildLShr(self.llbuilder, lhs, rhs, noname())
        }
    }

    fn ashr(&self, lhs: &'ll Value, rhs: &'ll Value) -> &'ll Value {
        self.count_insn("ashr");
        unsafe {
            llvm::LLVMBuildAShr(self.llbuilder, lhs, rhs, noname())
        }
    }

    fn and(&self, lhs: &'ll Value, rhs: &'ll Value) -> &'ll Value {
        self.count_insn("and");
        unsafe {
            llvm::LLVMBuildAnd(self.llbuilder, lhs, rhs, noname())
        }
    }

    fn or(&self, lhs: &'ll Value, rhs: &'ll Value) -> &'ll Value {
        self.count_insn("or");
        unsafe {
            llvm::LLVMBuildOr(self.llbuilder, lhs, rhs, noname())
        }
    }

    fn xor(&self, lhs: &'ll Value, rhs: &'ll Value) -> &'ll Value {
        self.count_insn("xor");
        unsafe {
            llvm::LLVMBuildXor(self.llbuilder, lhs, rhs, noname())
        }
    }

    fn neg(&self, v: &'ll Value) -> &'ll Value {
        self.count_insn("neg");
        unsafe {
            llvm::LLVMBuildNeg(self.llbuilder, v, noname())
        }
    }

    fn fneg(&self, v: &'ll Value) -> &'ll Value {
        self.count_insn("fneg");
        unsafe {
            llvm::LLVMBuildFNeg(self.llbuilder, v, noname())
        }
    }

    fn not(&self, v: &'ll Value) -> &'ll Value {
        self.count_insn("not");
        unsafe {
            llvm::LLVMBuildNot(self.llbuilder, v, noname())
        }
    }

    fn alloca(&self, ty: &'ll Type, name: &str, align: Align) -> &'ll Value {
        let bx = Builder::with_cx(self.cx);
        bx.position_at_start(unsafe {
            llvm::LLVMGetFirstBasicBlock(self.llfn())
        });
        bx.dynamic_alloca(ty, name, align)
    }

    fn dynamic_alloca(&self, ty: &'ll Type, name: &str, align: Align) -> &'ll Value {
        self.count_insn("alloca");
        unsafe {
            let alloca = if name.is_empty() {
                llvm::LLVMBuildAlloca(self.llbuilder, ty, noname())
            } else {
                let name = SmallCStr::new(name);
                llvm::LLVMBuildAlloca(self.llbuilder, ty,
                                      name.as_ptr())
            };
            llvm::LLVMSetAlignment(alloca, align.abi() as c_uint);
            alloca
        }
    }

    fn array_alloca(&self,
                        ty: &'ll Type,
                        len: &'ll Value,
                        name: &str,
                        align: Align) -> &'ll Value {
        self.count_insn("alloca");
        unsafe {
            let alloca = if name.is_empty() {
                llvm::LLVMBuildArrayAlloca(self.llbuilder, ty, len, noname())
            } else {
                let name = SmallCStr::new(name);
                llvm::LLVMBuildArrayAlloca(self.llbuilder, ty, len,
                                           name.as_ptr())
            };
            llvm::LLVMSetAlignment(alloca, align.abi() as c_uint);
            alloca
        }
    }

    fn load(&self, ptr: &'ll Value, align: Align) -> &'ll Value {
        self.count_insn("load");
        unsafe {
            let load = llvm::LLVMBuildLoad(self.llbuilder, ptr, noname());
            llvm::LLVMSetAlignment(load, align.abi() as c_uint);
            load
        }
    }

    fn volatile_load(&self, ptr: &'ll Value) -> &'ll Value {
        self.count_insn("load.volatile");
        unsafe {
            let insn = llvm::LLVMBuildLoad(self.llbuilder, ptr, noname());
            llvm::LLVMSetVolatile(insn, llvm::True);
            insn
        }
    }

    fn atomic_load(
        &self,
        ptr: &'ll Value,
        order: common::AtomicOrdering,
        size: Size,
    ) -> &'ll Value {
        self.count_insn("load.atomic");
        unsafe {
            let load = llvm::LLVMRustBuildAtomicLoad(
                self.llbuilder,
                ptr,
                noname(),
                AtomicOrdering::from_generic(order),
            );
            // LLVM requires the alignment of atomic loads to be at least the size of the type.
            llvm::LLVMSetAlignment(load, size.bytes() as c_uint);
            load
        }
    }

    fn load_operand(
        &self,
        place: PlaceRef<'tcx, &'ll Value>
    ) -> OperandRef<'tcx, &'ll Value> {
        debug!("PlaceRef::load: {:?}", place);

        assert_eq!(place.llextra.is_some(), place.layout.is_unsized());

        if place.layout.is_zst() {
            return OperandRef::new_zst(self.cx(), place.layout);
        }

        let scalar_load_metadata = |load, scalar: &layout::Scalar| {
            let vr = scalar.valid_range.clone();
            match scalar.value {
                layout::Int(..) => {
                    let range = scalar.valid_range_exclusive(self.cx());
                    if range.start != range.end {
                        self.range_metadata(load, range);
                    }
                }
                layout::Pointer if vr.start() < vr.end() && !vr.contains(&0) => {
                    self.nonnull_metadata(load);
                }
                _ => {}
            }
        };

        let val = if let Some(llextra) = place.llextra {
            OperandValue::Ref(place.llval, Some(llextra), place.align)
        } else if place.layout.is_llvm_immediate() {
            let mut const_llval = None;
            unsafe {
                if let Some(global) = llvm::LLVMIsAGlobalVariable(place.llval) {
                    if llvm::LLVMIsGlobalConstant(global) == llvm::True {
                        const_llval = llvm::LLVMGetInitializer(global);
                    }
                }
            }
            let llval = const_llval.unwrap_or_else(|| {
                let load = self.load(place.llval, place.align);
                if let layout::Abi::Scalar(ref scalar) = place.layout.abi {
                    scalar_load_metadata(load, scalar);
                }
                load
            });
            OperandValue::Immediate(base::to_immediate(self, llval, place.layout))
        } else if let layout::Abi::ScalarPair(ref a, ref b) = place.layout.abi {
            let load = |i, scalar: &layout::Scalar| {
                let llptr = self.struct_gep(place.llval, i as u64);
                let load = self.load(llptr, place.align);
                scalar_load_metadata(load, scalar);
                if scalar.is_bool() {
                    self.trunc(load, self.cx().type_i1())
                } else {
                    load
                }
            };
            OperandValue::Pair(load(0, a), load(1, b))
        } else {
            OperandValue::Ref(place.llval, None, place.align)
        };

        OperandRef { val, layout: place.layout }
    }



    fn range_metadata(&self, load: &'ll Value, range: Range<u128>) {
        if self.cx().sess().target.target.arch == "amdgpu" {
            // amdgpu/LLVM does something weird and thinks a i64 value is
            // split into a v2i32, halving the bitwidth LLVM expects,
            // tripping an assertion. So, for now, just disable this
            // optimization.
            return;
        }

        unsafe {
            let llty = self.cx.val_ty(load);
            let v = [
                self.cx.const_uint_big(llty, range.start),
                self.cx.const_uint_big(llty, range.end)
            ];

            llvm::LLVMSetMetadata(load, llvm::MD_range as c_uint,
                                  llvm::LLVMMDNodeInContext(self.cx.llcx,
                                                            v.as_ptr(),
                                                            v.len() as c_uint));
        }
    }

    fn nonnull_metadata(&self, load: &'ll Value) {
        unsafe {
            llvm::LLVMSetMetadata(load, llvm::MD_nonnull as c_uint,
                                  llvm::LLVMMDNodeInContext(self.cx.llcx, ptr::null(), 0));
        }
    }

    fn store(&self, val: &'ll Value, ptr: &'ll Value, align: Align) -> &'ll Value {
        self.store_with_flags(val, ptr, align, MemFlags::empty())
    }

    fn store_with_flags(
        &self,
        val: &'ll Value,
        ptr: &'ll Value,
        align: Align,
        flags: MemFlags,
    ) -> &'ll Value {
        debug!("Store {:?} -> {:?} ({:?})", val, ptr, flags);
        self.count_insn("store");
        let ptr = self.check_store(val, ptr);
        unsafe {
            let store = llvm::LLVMBuildStore(self.llbuilder, val, ptr);
            let align = if flags.contains(MemFlags::UNALIGNED) {
                1
            } else {
                align.abi() as c_uint
            };
            llvm::LLVMSetAlignment(store, align);
            if flags.contains(MemFlags::VOLATILE) {
                llvm::LLVMSetVolatile(store, llvm::True);
            }
            if flags.contains(MemFlags::NONTEMPORAL) {
                // According to LLVM [1] building a nontemporal store must
                // *always* point to a metadata value of the integer 1.
                //
                // [1]: http://llvm.org/docs/LangRef.html#store-instruction
                let one = self.cx.const_i32(1);
                let node = llvm::LLVMMDNodeInContext(self.cx.llcx, &one, 1);
                llvm::LLVMSetMetadata(store, llvm::MD_nontemporal as c_uint, node);
            }
            store
        }
    }

   fn atomic_store(&self, val: &'ll Value, ptr: &'ll Value,
                   order: common::AtomicOrdering, size: Size) {
        debug!("Store {:?} -> {:?}", val, ptr);
        self.count_insn("store.atomic");
        let ptr = self.check_store(val, ptr);
        unsafe {
            let store = llvm::LLVMRustBuildAtomicStore(
                self.llbuilder,
                val,
                ptr,
                AtomicOrdering::from_generic(order),
            );
            // LLVM requires the alignment of atomic stores to be at least the size of the type.
            llvm::LLVMSetAlignment(store, size.bytes() as c_uint);
        }
    }

    fn gep(&self, ptr: &'ll Value, indices: &[&'ll Value]) -> &'ll Value {
        self.count_insn("gep");
        unsafe {
            llvm::LLVMBuildGEP(self.llbuilder, ptr, indices.as_ptr(),
                               indices.len() as c_uint, noname())
        }
    }

    fn inbounds_gep(&self, ptr: &'ll Value, indices: &[&'ll Value]) -> &'ll Value {
        self.count_insn("inboundsgep");
        unsafe {
            llvm::LLVMBuildInBoundsGEP(
                self.llbuilder, ptr, indices.as_ptr(), indices.len() as c_uint, noname())
        }
    }

    /* Casts */
    fn trunc(&self, val: &'ll Value, dest_ty: &'ll Type) -> &'ll Value {
        self.count_insn("trunc");
        unsafe {
            llvm::LLVMBuildTrunc(self.llbuilder, val, dest_ty, noname())
        }
    }

    fn sext(&self, val: &'ll Value, dest_ty: &'ll Type) -> &'ll Value {
        self.count_insn("sext");
        unsafe {
            llvm::LLVMBuildSExt(self.llbuilder, val, dest_ty, noname())
        }
    }

    fn fptoui(&self, val: &'ll Value, dest_ty: &'ll Type) -> &'ll Value {
        self.count_insn("fptoui");
        unsafe {
            llvm::LLVMBuildFPToUI(self.llbuilder, val, dest_ty, noname())
        }
    }

    fn fptosi(&self, val: &'ll Value, dest_ty: &'ll Type) -> &'ll Value {
        self.count_insn("fptosi");
        unsafe {
            llvm::LLVMBuildFPToSI(self.llbuilder, val, dest_ty,noname())
        }
    }

    fn uitofp(&self, val: &'ll Value, dest_ty: &'ll Type) -> &'ll Value {
        self.count_insn("uitofp");
        unsafe {
            llvm::LLVMBuildUIToFP(self.llbuilder, val, dest_ty, noname())
        }
    }

    fn sitofp(&self, val: &'ll Value, dest_ty: &'ll Type) -> &'ll Value {
        self.count_insn("sitofp");
        unsafe {
            llvm::LLVMBuildSIToFP(self.llbuilder, val, dest_ty, noname())
        }
    }

    fn fptrunc(&self, val: &'ll Value, dest_ty: &'ll Type) -> &'ll Value {
        self.count_insn("fptrunc");
        unsafe {
            llvm::LLVMBuildFPTrunc(self.llbuilder, val, dest_ty, noname())
        }
    }

    fn fpext(&self, val: &'ll Value, dest_ty: &'ll Type) -> &'ll Value {
        self.count_insn("fpext");
        unsafe {
            llvm::LLVMBuildFPExt(self.llbuilder, val, dest_ty, noname())
        }
    }

    fn ptrtoint(&self, val: &'ll Value, dest_ty: &'ll Type) -> &'ll Value {
        self.count_insn("ptrtoint");
        unsafe {
            llvm::LLVMBuildPtrToInt(self.llbuilder, val, dest_ty, noname())
        }
    }

    fn inttoptr(&self, val: &'ll Value, dest_ty: &'ll Type) -> &'ll Value {
        self.count_insn("inttoptr");
        unsafe {
            llvm::LLVMBuildIntToPtr(self.llbuilder, val, dest_ty, noname())
        }
    }

    fn bitcast(&self, val: &'ll Value, dest_ty: &'ll Type) -> &'ll Value {
        self.count_insn("bitcast");
        unsafe {
            llvm::LLVMBuildBitCast(self.llbuilder, val, dest_ty, noname())
        }
    }


    fn intcast(&self, val: &'ll Value, dest_ty: &'ll Type, is_signed: bool) -> &'ll Value {
        self.count_insn("intcast");
        unsafe {
            llvm::LLVMRustBuildIntCast(self.llbuilder, val, dest_ty, is_signed)
        }
    }

    fn pointercast(&self, val: &'ll Value, dest_ty: &'ll Type) -> &'ll Value {
        self.count_insn("pointercast");
        unsafe {
            llvm::LLVMBuildPointerCast(self.llbuilder, val, dest_ty, noname())
        }
    }

    /* Comparisons */
    fn icmp(&self, op: IntPredicate, lhs: &'ll Value, rhs: &'ll Value) -> &'ll Value {
        self.count_insn("icmp");
        let op = llvm::IntPredicate::from_generic(op);
        unsafe {
            llvm::LLVMBuildICmp(self.llbuilder, op as c_uint, lhs, rhs, noname())
        }
    }

    fn fcmp(&self, op: RealPredicate, lhs: &'ll Value, rhs: &'ll Value) -> &'ll Value {
        self.count_insn("fcmp");
        unsafe {
            llvm::LLVMBuildFCmp(self.llbuilder, op as c_uint, lhs, rhs, noname())
        }
    }

    /* Miscellaneous instructions */
    fn empty_phi(&self, ty: &'ll Type) -> &'ll Value {
        self.count_insn("emptyphi");
        unsafe {
            llvm::LLVMBuildPhi(self.llbuilder, ty, noname())
        }
    }

    fn phi(&self, ty: &'ll Type, vals: &[&'ll Value], bbs: &[&'ll BasicBlock]) -> &'ll Value {
        assert_eq!(vals.len(), bbs.len());
        let phi = self.empty_phi(ty);
        self.count_insn("addincoming");
        unsafe {
            llvm::LLVMAddIncoming(phi, vals.as_ptr(),
                                  bbs.as_ptr(),
                                  vals.len() as c_uint);
            phi
        }
    }

    fn inline_asm_call(&self, asm: *const c_char, cons: *const c_char,
                       inputs: &[&'ll Value], output: &'ll Type,
                       volatile: bool, alignstack: bool,
                       dia: syntax::ast::AsmDialect) -> Option<&'ll Value> {
        self.count_insn("inlineasm");

        let volatile = if volatile { llvm::True }
                       else        { llvm::False };
        let alignstack = if alignstack { llvm::True }
                         else          { llvm::False };

        let argtys = inputs.iter().map(|v| {
            debug!("Asm Input Type: {:?}", *v);
            self.cx.val_ty(*v)
        }).collect::<Vec<_>>();

        debug!("Asm Output Type: {:?}", output);
        let fty = self.cx().type_func(&argtys[..], output);
        unsafe {
            // Ask LLVM to verify that the constraints are well-formed.
            let constraints_ok = llvm::LLVMRustInlineAsmVerify(fty, cons);
            debug!("Constraint verification result: {:?}", constraints_ok);
            if constraints_ok {
                let v = llvm::LLVMRustInlineAsm(
                    fty, asm, cons, volatile, alignstack, AsmDialect::from_generic(dia));
                Some(self.call(v, inputs, None))
            } else {
                // LLVM has detected an issue with our constraints, bail out
                None
            }
        }
    }

    fn memcpy(&self, dst: &'ll Value, dst_align: Align,
                  src: &'ll Value, src_align: Align,
                  size: &'ll Value, flags: MemFlags) {
        if flags.contains(MemFlags::NONTEMPORAL) {
            // HACK(nox): This is inefficient but there is no nontemporal memcpy.
            let val = self.load(src, src_align);
            let ptr = self.pointercast(dst, self.cx().type_ptr_to(self.cx().val_ty(val)));
            self.store_with_flags(val, ptr, dst_align, flags);
            return;
        }
        let size = self.intcast(size, self.cx().type_isize(), false);
        let is_volatile = flags.contains(MemFlags::VOLATILE);
        let dst = self.pointercast(dst, self.cx().type_i8p());
        let src = self.pointercast(src, self.cx().type_i8p());
        unsafe {
            llvm::LLVMRustBuildMemCpy(self.llbuilder, dst, dst_align.abi() as c_uint,
                                      src, src_align.abi() as c_uint, size, is_volatile);
        }
    }

    fn memmove(&self, dst: &'ll Value, dst_align: Align,
                  src: &'ll Value, src_align: Align,
                  size: &'ll Value, flags: MemFlags) {
        if flags.contains(MemFlags::NONTEMPORAL) {
            // HACK(nox): This is inefficient but there is no nontemporal memmove.
            let val = self.load(src, src_align);
            let ptr = self.pointercast(dst, self.cx().type_ptr_to(self.cx().val_ty(val)));
            self.store_with_flags(val, ptr, dst_align, flags);
            return;
        }
        let size = self.intcast(size, self.cx().type_isize(), false);
        let is_volatile = flags.contains(MemFlags::VOLATILE);
        let dst = self.pointercast(dst, self.cx().type_i8p());
        let src = self.pointercast(src, self.cx().type_i8p());
        unsafe {
            llvm::LLVMRustBuildMemMove(self.llbuilder, dst, dst_align.abi() as c_uint,
                                      src, src_align.abi() as c_uint, size, is_volatile);
        }
    }

    fn memset(
        &self,
        ptr: &'ll Value,
        fill_byte: &'ll Value,
        size: &'ll Value,
        align: Align,
        flags: MemFlags,
    ) {
        let ptr_width = &self.cx().sess().target.target.target_pointer_width;
        let intrinsic_key = format!("llvm.memset.p0i8.i{}", ptr_width);
        let llintrinsicfn = self.cx().get_intrinsic(&intrinsic_key);
        let ptr = self.pointercast(ptr, self.cx().type_i8p());
        let align = self.cx().const_u32(align.abi() as u32);
        let volatile = self.cx().const_bool(flags.contains(MemFlags::VOLATILE));
        self.call(llintrinsicfn, &[ptr, fill_byte, size, align, volatile], None);
    }

    fn minnum(&self, lhs: &'ll Value, rhs: &'ll Value) -> &'ll Value {
        self.count_insn("minnum");
        unsafe {
            let instr = llvm::LLVMRustBuildMinNum(self.llbuilder, lhs, rhs);
            instr.expect("LLVMRustBuildMinNum is not available in LLVM version < 6.0")
        }
    }
    fn maxnum(&self, lhs: &'ll Value, rhs: &'ll Value) -> &'ll Value {
        self.count_insn("maxnum");
        unsafe {
            let instr = llvm::LLVMRustBuildMaxNum(self.llbuilder, lhs, rhs);
            instr.expect("LLVMRustBuildMaxNum is not available in LLVM version < 6.0")
        }
    }

    fn select(
        &self, cond: &'ll Value,
        then_val: &'ll Value,
        else_val: &'ll Value,
    ) -> &'ll Value {
        self.count_insn("select");
        unsafe {
            llvm::LLVMBuildSelect(self.llbuilder, cond, then_val, else_val, noname())
        }
    }

    #[allow(dead_code)]
    fn va_arg(&self, list: &'ll Value, ty: &'ll Type) -> &'ll Value {
        self.count_insn("vaarg");
        unsafe {
            llvm::LLVMBuildVAArg(self.llbuilder, list, ty, noname())
        }
    }

    fn extract_element(&self, vec: &'ll Value, idx: &'ll Value) -> &'ll Value {
        self.count_insn("extractelement");
        unsafe {
            llvm::LLVMBuildExtractElement(self.llbuilder, vec, idx, noname())
        }
    }

    fn insert_element(
        &self, vec: &'ll Value,
        elt: &'ll Value,
        idx: &'ll Value,
    ) -> &'ll Value {
        self.count_insn("insertelement");
        unsafe {
            llvm::LLVMBuildInsertElement(self.llbuilder, vec, elt, idx, noname())
        }
    }

    fn shuffle_vector(&self, v1: &'ll Value, v2: &'ll Value, mask: &'ll Value) -> &'ll Value {
        self.count_insn("shufflevector");
        unsafe {
            llvm::LLVMBuildShuffleVector(self.llbuilder, v1, v2, mask, noname())
        }
    }

    fn vector_splat(&self, num_elts: usize, elt: &'ll Value) -> &'ll Value {
        unsafe {
            let elt_ty = self.cx.val_ty(elt);
            let undef = llvm::LLVMGetUndef(self.cx().type_vector(elt_ty, num_elts as u64));
            let vec = self.insert_element(undef, elt, self.cx.const_i32(0));
            let vec_i32_ty = self.cx().type_vector(self.cx().type_i32(), num_elts as u64);
            self.shuffle_vector(vec, undef, self.cx().const_null(vec_i32_ty))
        }
    }

    fn vector_reduce_fadd_fast(&self, acc: &'ll Value, src: &'ll Value) -> &'ll Value {
        self.count_insn("vector.reduce.fadd_fast");
        unsafe {
            // FIXME: add a non-fast math version once
            // https://bugs.llvm.org/show_bug.cgi?id=36732
            // is fixed.
            let instr = llvm::LLVMRustBuildVectorReduceFAdd(self.llbuilder, acc, src);
            llvm::LLVMRustSetHasUnsafeAlgebra(instr);
            instr
        }
    }
    fn vector_reduce_fmul_fast(&self, acc: &'ll Value, src: &'ll Value) -> &'ll Value {
        self.count_insn("vector.reduce.fmul_fast");
        unsafe {
            // FIXME: add a non-fast math version once
            // https://bugs.llvm.org/show_bug.cgi?id=36732
            // is fixed.
            let instr = llvm::LLVMRustBuildVectorReduceFMul(self.llbuilder, acc, src);
            llvm::LLVMRustSetHasUnsafeAlgebra(instr);
            instr
        }
    }
    fn vector_reduce_add(&self, src: &'ll Value) -> &'ll Value {
        self.count_insn("vector.reduce.add");
        unsafe { llvm::LLVMRustBuildVectorReduceAdd(self.llbuilder, src) }
    }
    fn vector_reduce_mul(&self, src: &'ll Value) -> &'ll Value {
        self.count_insn("vector.reduce.mul");
        unsafe { llvm::LLVMRustBuildVectorReduceMul(self.llbuilder, src) }
    }
    fn vector_reduce_and(&self, src: &'ll Value) -> &'ll Value {
        self.count_insn("vector.reduce.and");
        unsafe { llvm::LLVMRustBuildVectorReduceAnd(self.llbuilder, src) }
    }
    fn vector_reduce_or(&self, src: &'ll Value) -> &'ll Value {
        self.count_insn("vector.reduce.or");
        unsafe { llvm::LLVMRustBuildVectorReduceOr(self.llbuilder, src) }
    }
    fn vector_reduce_xor(&self, src: &'ll Value) -> &'ll Value {
        self.count_insn("vector.reduce.xor");
        unsafe { llvm::LLVMRustBuildVectorReduceXor(self.llbuilder, src) }
    }
    fn vector_reduce_fmin(&self, src: &'ll Value) -> &'ll Value {
        self.count_insn("vector.reduce.fmin");
        unsafe { llvm::LLVMRustBuildVectorReduceFMin(self.llbuilder, src, /*NoNaNs:*/ false) }
    }
    fn vector_reduce_fmax(&self, src: &'ll Value) -> &'ll Value {
        self.count_insn("vector.reduce.fmax");
        unsafe { llvm::LLVMRustBuildVectorReduceFMax(self.llbuilder, src, /*NoNaNs:*/ false) }
    }
    fn vector_reduce_fmin_fast(&self, src: &'ll Value) -> &'ll Value {
        self.count_insn("vector.reduce.fmin_fast");
        unsafe {
            let instr = llvm::LLVMRustBuildVectorReduceFMin(self.llbuilder, src, /*NoNaNs:*/ true);
            llvm::LLVMRustSetHasUnsafeAlgebra(instr);
            instr
        }
    }
    fn vector_reduce_fmax_fast(&self, src: &'ll Value) -> &'ll Value {
        self.count_insn("vector.reduce.fmax_fast");
        unsafe {
            let instr = llvm::LLVMRustBuildVectorReduceFMax(self.llbuilder, src, /*NoNaNs:*/ true);
            llvm::LLVMRustSetHasUnsafeAlgebra(instr);
            instr
        }
    }
    fn vector_reduce_min(&self, src: &'ll Value, is_signed: bool) -> &'ll Value {
        self.count_insn("vector.reduce.min");
        unsafe { llvm::LLVMRustBuildVectorReduceMin(self.llbuilder, src, is_signed) }
    }
    fn vector_reduce_max(&self, src: &'ll Value, is_signed: bool) -> &'ll Value {
        self.count_insn("vector.reduce.max");
        unsafe { llvm::LLVMRustBuildVectorReduceMax(self.llbuilder, src, is_signed) }
    }

    fn extract_value(&self, agg_val: &'ll Value, idx: u64) -> &'ll Value {
        self.count_insn("extractvalue");
        assert_eq!(idx as c_uint as u64, idx);
        unsafe {
            llvm::LLVMBuildExtractValue(self.llbuilder, agg_val, idx as c_uint, noname())
        }
    }

    fn insert_value(&self, agg_val: &'ll Value, elt: &'ll Value,
                       idx: u64) -> &'ll Value {
        self.count_insn("insertvalue");
        assert_eq!(idx as c_uint as u64, idx);
        unsafe {
            llvm::LLVMBuildInsertValue(self.llbuilder, agg_val, elt, idx as c_uint,
                                       noname())
        }
    }

    fn landing_pad(&self, ty: &'ll Type, pers_fn: &'ll Value,
                       num_clauses: usize) -> &'ll Value {
        self.count_insn("landingpad");
        unsafe {
            llvm::LLVMBuildLandingPad(self.llbuilder, ty, pers_fn,
                                      num_clauses as c_uint, noname())
        }
    }

    fn add_clause(&self, landing_pad: &'ll Value, clause: &'ll Value) {
        unsafe {
            llvm::LLVMAddClause(landing_pad, clause);
        }
    }

    fn set_cleanup(&self, landing_pad: &'ll Value) {
        self.count_insn("setcleanup");
        unsafe {
            llvm::LLVMSetCleanup(landing_pad, llvm::True);
        }
    }

    fn resume(&self, exn: &'ll Value) -> &'ll Value {
        self.count_insn("resume");
        unsafe {
            llvm::LLVMBuildResume(self.llbuilder, exn)
        }
    }

    fn cleanup_pad(&self,
                       parent: Option<&'ll Value>,
                       args: &[&'ll Value]) -> &'ll Value {
        self.count_insn("cleanuppad");
        let name = const_cstr!("cleanuppad");
        let ret = unsafe {
            llvm::LLVMRustBuildCleanupPad(self.llbuilder,
                                          parent,
                                          args.len() as c_uint,
                                          args.as_ptr(),
                                          name.as_ptr())
        };
        ret.expect("LLVM does not have support for cleanuppad")
    }

    fn cleanup_ret(
        &self, cleanup: &'ll Value,
        unwind: Option<&'ll BasicBlock>,
    ) -> &'ll Value {
        self.count_insn("cleanupret");
        let ret = unsafe {
            llvm::LLVMRustBuildCleanupRet(self.llbuilder, cleanup, unwind)
        };
        ret.expect("LLVM does not have support for cleanupret")
    }

    fn catch_pad(&self,
                     parent: &'ll Value,
                     args: &[&'ll Value]) -> &'ll Value {
        self.count_insn("catchpad");
        let name = const_cstr!("catchpad");
        let ret = unsafe {
            llvm::LLVMRustBuildCatchPad(self.llbuilder, parent,
                                        args.len() as c_uint, args.as_ptr(),
                                        name.as_ptr())
        };
        ret.expect("LLVM does not have support for catchpad")
    }

    fn catch_ret(&self, pad: &'ll Value, unwind: &'ll BasicBlock) -> &'ll Value {
        self.count_insn("catchret");
        let ret = unsafe {
            llvm::LLVMRustBuildCatchRet(self.llbuilder, pad, unwind)
        };
        ret.expect("LLVM does not have support for catchret")
    }

    fn catch_switch(
        &self,
        parent: Option<&'ll Value>,
        unwind: Option<&'ll BasicBlock>,
        num_handlers: usize,
    ) -> &'ll Value {
        self.count_insn("catchswitch");
        let name = const_cstr!("catchswitch");
        let ret = unsafe {
            llvm::LLVMRustBuildCatchSwitch(self.llbuilder, parent, unwind,
                                           num_handlers as c_uint,
                                           name.as_ptr())
        };
        ret.expect("LLVM does not have support for catchswitch")
    }

    fn add_handler(&self, catch_switch: &'ll Value, handler: &'ll BasicBlock) {
        unsafe {
            llvm::LLVMRustAddHandler(catch_switch, handler);
        }
    }

    fn set_personality_fn(&self, personality: &'ll Value) {
        unsafe {
            llvm::LLVMSetPersonalityFn(self.llfn(), personality);
        }
    }

    // Atomic Operations
    fn atomic_cmpxchg(
        &self,
        dst: &'ll Value,
        cmp: &'ll Value,
        src: &'ll Value,
        order: common::AtomicOrdering,
        failure_order: common::AtomicOrdering,
        weak: bool,
    ) -> &'ll Value {
        let weak = if weak { llvm::True } else { llvm::False };
        unsafe {
            llvm::LLVMRustBuildAtomicCmpXchg(
                self.llbuilder,
                dst,
                cmp,
                src,
                AtomicOrdering::from_generic(order),
                AtomicOrdering::from_generic(failure_order),
                weak
            )
        }
    }
    fn atomic_rmw(
        &self,
        op: common::AtomicRmwBinOp,
        dst: &'ll Value,
        src: &'ll Value,
        order: common::AtomicOrdering,
    ) -> &'ll Value {
        unsafe {
            llvm::LLVMBuildAtomicRMW(
                self.llbuilder,
                AtomicRmwBinOp::from_generic(op),
                dst,
                src,
                AtomicOrdering::from_generic(order),
                False)
        }
    }

    fn atomic_fence(&self, order: common::AtomicOrdering, scope: common::SynchronizationScope) {
        unsafe {
            llvm::LLVMRustBuildAtomicFence(
                self.llbuilder,
                AtomicOrdering::from_generic(order),
                SynchronizationScope::from_generic(scope)
            );
        }
    }

    fn add_case(&self, s: &'ll Value, on_val: &'ll Value, dest: &'ll BasicBlock) {
        unsafe {
            llvm::LLVMAddCase(s, on_val, dest)
        }
    }

    fn add_incoming_to_phi(&self, phi: &'ll Value, val: &'ll Value, bb: &'ll BasicBlock) {
        self.count_insn("addincoming");
        unsafe {
            llvm::LLVMAddIncoming(phi, &val, &bb, 1 as c_uint);
        }
    }

    fn set_invariant_load(&self, load: &'ll Value) {
        unsafe {
            llvm::LLVMSetMetadata(load, llvm::MD_invariant_load as c_uint,
                                  llvm::LLVMMDNodeInContext(self.cx.llcx, ptr::null(), 0));
        }
    }

    fn check_store<'b>(&self,
                       val: &'ll Value,
                       ptr: &'ll Value) -> &'ll Value {
        let dest_ptr_ty = self.cx.val_ty(ptr);
        let stored_ty = self.cx.val_ty(val);
        let stored_ptr_ty = self.cx.type_ptr_to(stored_ty);

        assert_eq!(self.cx.type_kind(dest_ptr_ty), TypeKind::Pointer);

        if dest_ptr_ty == stored_ptr_ty {
            ptr
        } else {
            debug!("Type mismatch in store. \
                    Expected {:?}, got {:?}; inserting bitcast",
                   dest_ptr_ty, stored_ptr_ty);
            self.bitcast(ptr, stored_ptr_ty)
        }
    }

    fn check_call<'b>(&self,
                      typ: &str,
                      llfn: &'ll Value,
                      args: &'b [&'ll Value]) -> Cow<'b, [&'ll Value]> {
        let mut fn_ty = self.cx.val_ty(llfn);
        // Strip off pointers
        while self.cx.type_kind(fn_ty) == TypeKind::Pointer {
            fn_ty = self.cx.element_type(fn_ty);
        }

        assert!(self.cx.type_kind(fn_ty) == TypeKind::Function,
                "builder::{} not passed a function, but {:?}", typ, fn_ty);

        let param_tys = self.cx.func_params_types(fn_ty);

        let all_args_match = param_tys.iter()
            .zip(args.iter().map(|&v| self.cx().val_ty(v)))
            .all(|(expected_ty, actual_ty)| *expected_ty == actual_ty);

        if all_args_match {
            return Cow::Borrowed(args);
        }

        let casted_args: Vec<_> = param_tys.into_iter()
            .zip(args.iter())
            .enumerate()
            .map(|(i, (expected_ty, &actual_val))| {
                let actual_ty = self.cx().val_ty(actual_val);
                if expected_ty != actual_ty {
                    debug!("Type mismatch in function call of {:?}. \
                            Expected {:?} for param {}, got {:?}; injecting bitcast",
                           llfn, expected_ty, i, actual_ty);
                    self.bitcast(actual_val, expected_ty)
                } else {
                    actual_val
                }
            })
            .collect();

        Cow::Owned(casted_args)
    }

    fn lifetime_start(&self, ptr: &'ll Value, size: Size) {
        self.call_lifetime_intrinsic("llvm.lifetime.start", ptr, size);
    }

    fn lifetime_end(&self, ptr: &'ll Value, size: Size) {
        self.call_lifetime_intrinsic("llvm.lifetime.end", ptr, size);
    }

    fn call_lifetime_intrinsic(&self, intrinsic: &str, ptr: &'ll Value, size: Size) {
        if self.cx.sess().opts.optimize == config::OptLevel::No {
            return;
        }

        let size = size.bytes();
        if size == 0 {
            return;
        }

        let lifetime_intrinsic = self.cx.get_intrinsic(intrinsic);

        let ptr = self.pointercast(ptr, self.cx.type_i8p());
        self.call(lifetime_intrinsic, &[self.cx.const_u64(size), ptr], None);
    }

    fn call(&self, llfn: &'ll Value, args: &[&'ll Value],
                funclet: Option<&common::Funclet<&'ll Value>>) -> &'ll Value {
        self.count_insn("call");

        debug!("Call {:?} with args ({:?})",
               llfn,
               args);

        let args = self.check_call("call", llfn, args);
        let bundle = funclet.map(|funclet| funclet.bundle());
        let bundle = bundle.map(OperandBundleDef::from_generic);
        let bundle = bundle.as_ref().map(|b| &*b.raw);

        unsafe {
            llvm::LLVMRustBuildCall(
                self.llbuilder,
                llfn,
                args.as_ptr() as *const &llvm::Value,
                args.len() as c_uint,
                bundle, noname()
            )
        }
    }

    fn zext(&self, val: &'ll Value, dest_ty: &'ll Type) -> &'ll Value {
        self.count_insn("zext");
        unsafe {
            llvm::LLVMBuildZExt(self.llbuilder, val, dest_ty, noname())
        }
    }

    fn struct_gep(&self, ptr: &'ll Value, idx: u64) -> &'ll Value {
        self.count_insn("structgep");
        assert_eq!(idx as c_uint as u64, idx);
        unsafe {
            llvm::LLVMBuildStructGEP(self.llbuilder, ptr, idx as c_uint, noname())
        }
    }

    fn cx(&self) -> &CodegenCx<'ll, 'tcx> {
        self.cx
    }

    fn delete_basic_block(&self, bb: &'ll BasicBlock) {
        unsafe {
            llvm::LLVMDeleteBasicBlock(bb);
        }
    }

    fn do_not_inline(&self, llret: &'ll Value) {
        llvm::Attribute::NoInline.apply_callsite(llvm::AttributePlace::Function, llret);
    }
}
