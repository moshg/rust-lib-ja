// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use middle::const_val::ConstVal;
use hir::def_id::DefId;
use ty::subst::Substs;
use ty::{ClosureSubsts, Region, Ty};
use mir::*;
use rustc_const_math::ConstUsize;
use rustc_data_structures::tuple_slice::TupleSlice;
use rustc_data_structures::indexed_vec::Idx;
use syntax_pos::Span;

// # The MIR Visitor
//
// ## Overview
//
// There are two visitors, one for immutable and one for mutable references,
// but both are generated by the following macro. The code is written according
// to the following conventions:
//
// - introduce a `visit_foo` and a `super_foo` method for every MIR type
// - `visit_foo`, by default, calls `super_foo`
// - `super_foo`, by default, destructures the `foo` and calls `visit_foo`
//
// This allows you as a user to override `visit_foo` for types are
// interested in, and invoke (within that method) call
// `self.super_foo` to get the default behavior. Just as in an OO
// language, you should never call `super` methods ordinarily except
// in that circumstance.
//
// For the most part, we do not destructure things external to the
// MIR, e.g. types, spans, etc, but simply visit them and stop. This
// avoids duplication with other visitors like `TypeFoldable`.
//
// ## Updating
//
// The code is written in a very deliberate style intended to minimize
// the chance of things being overlooked. You'll notice that we always
// use pattern matching to reference fields and we ensure that all
// matches are exhaustive.
//
// For example, the `super_basic_block_data` method begins like this:
//
// ```rust
// fn super_basic_block_data(&mut self,
//                           block: BasicBlock,
//                           data: & $($mutability)* BasicBlockData<'tcx>) {
//     let BasicBlockData {
//         ref $($mutability)* statements,
//         ref $($mutability)* terminator,
//         is_cleanup: _
//     } = *data;
//
//     for statement in statements {
//         self.visit_statement(block, statement);
//     }
//
//     ...
// }
// ```
//
// Here we used `let BasicBlockData { <fields> } = *data` deliberately,
// rather than writing `data.statements` in the body. This is because if one
// adds a new field to `BasicBlockData`, one will be forced to revise this code,
// and hence one will (hopefully) invoke the correct visit methods (if any).
//
// For this to work, ALL MATCHES MUST BE EXHAUSTIVE IN FIELDS AND VARIANTS.
// That means you never write `..` to skip over fields, nor do you write `_`
// to skip over variants in a `match`.
//
// The only place that `_` is acceptable is to match a field (or
// variant argument) that does not require visiting, as in
// `is_cleanup` above.

macro_rules! make_mir_visitor {
    ($visitor_trait_name:ident, $($mutability:ident)*) => {
        pub trait $visitor_trait_name<'tcx> {
            // Override these, and call `self.super_xxx` to revert back to the
            // default behavior.

            fn visit_mir(&mut self, mir: & $($mutability)* Mir<'tcx>) {
                self.super_mir(mir);
            }

            fn visit_basic_block_data(&mut self,
                                      block: BasicBlock,
                                      data: & $($mutability)* BasicBlockData<'tcx>) {
                self.super_basic_block_data(block, data);
            }

            fn visit_visibility_scope_data(&mut self,
                                           scope_data: & $($mutability)* VisibilityScopeData) {
                self.super_visibility_scope_data(scope_data);
            }

            fn visit_statement(&mut self,
                               block: BasicBlock,
                               statement: & $($mutability)* Statement<'tcx>,
                               location: Location) {
                self.super_statement(block, statement, location);
            }

            fn visit_assign(&mut self,
                            block: BasicBlock,
                            lvalue: & $($mutability)* Lvalue<'tcx>,
                            rvalue: & $($mutability)* Rvalue<'tcx>,
                            location: Location) {
                self.super_assign(block, lvalue, rvalue, location);
            }

            fn visit_terminator(&mut self,
                                block: BasicBlock,
                                terminator: & $($mutability)* Terminator<'tcx>,
                                location: Location) {
                self.super_terminator(block, terminator, location);
            }

            fn visit_terminator_kind(&mut self,
                                     block: BasicBlock,
                                     kind: & $($mutability)* TerminatorKind<'tcx>,
                                     location: Location) {
                self.super_terminator_kind(block, kind, location);
            }

            fn visit_assert_message(&mut self,
                                    msg: & $($mutability)* AssertMessage<'tcx>,
                                    location: Location) {
                self.super_assert_message(msg, location);
            }

            fn visit_rvalue(&mut self,
                            rvalue: & $($mutability)* Rvalue<'tcx>,
                            location: Location) {
                self.super_rvalue(rvalue, location);
            }

            fn visit_operand(&mut self,
                             operand: & $($mutability)* Operand<'tcx>,
                             location: Location) {
                self.super_operand(operand, location);
            }

            fn visit_lvalue(&mut self,
                            lvalue: & $($mutability)* Lvalue<'tcx>,
                            context: LvalueContext<'tcx>,
                            location: Location) {
                self.super_lvalue(lvalue, context, location);
            }

            fn visit_projection(&mut self,
                                lvalue: & $($mutability)* LvalueProjection<'tcx>,
                                context: LvalueContext,
                                location: Location) {
                self.super_projection(lvalue, context, location);
            }

            fn visit_projection_elem(&mut self,
                                     lvalue: & $($mutability)* LvalueElem<'tcx>,
                                     context: LvalueContext,
                                     location: Location) {
                self.super_projection_elem(lvalue, context, location);
            }

            fn visit_branch(&mut self,
                            source: BasicBlock,
                            target: BasicBlock) {
                self.super_branch(source, target);
            }

            fn visit_constant(&mut self,
                              constant: & $($mutability)* Constant<'tcx>,
                              location: Location) {
                self.super_constant(constant, location);
            }

            fn visit_literal(&mut self,
                             literal: & $($mutability)* Literal<'tcx>,
                             location: Location) {
                self.super_literal(literal, location);
            }

            fn visit_def_id(&mut self,
                            def_id: & $($mutability)* DefId,
                            _: Location) {
                self.super_def_id(def_id);
            }

            fn visit_span(&mut self,
                          span: & $($mutability)* Span) {
                self.super_span(span);
            }

            fn visit_source_info(&mut self,
                                 source_info: & $($mutability)* SourceInfo) {
                self.super_source_info(source_info);
            }

            fn visit_ty(&mut self,
                        ty: & $($mutability)* Ty<'tcx>) {
                self.super_ty(ty);
            }

            fn visit_substs(&mut self,
                            substs: & $($mutability)* &'tcx Substs<'tcx>) {
                self.super_substs(substs);
            }

            fn visit_closure_substs(&mut self,
                                    substs: & $($mutability)* ClosureSubsts<'tcx>) {
                self.super_closure_substs(substs);
            }

            fn visit_const_val(&mut self,
                               const_val: & $($mutability)* ConstVal,
                               _: Location) {
                self.super_const_val(const_val);
            }

            fn visit_const_usize(&mut self,
                                 const_usize: & $($mutability)* ConstUsize,
                                 _: Location) {
                self.super_const_usize(const_usize);
            }

            fn visit_typed_const_val(&mut self,
                                     val: & $($mutability)* TypedConstVal<'tcx>,
                                     location: Location) {
                self.super_typed_const_val(val, location);
            }

            fn visit_local_decl(&mut self,
                                local_decl: & $($mutability)* LocalDecl<'tcx>) {
                self.super_local_decl(local_decl);
            }

            fn visit_visibility_scope(&mut self,
                                      scope: & $($mutability)* VisibilityScope) {
                self.super_visibility_scope(scope);
            }

            // The `super_xxx` methods comprise the default behavior and are
            // not meant to be overridden.

            fn super_mir(&mut self,
                         mir: & $($mutability)* Mir<'tcx>) {
                for index in 0..mir.basic_blocks().len() {
                    let block = BasicBlock::new(index);
                    self.visit_basic_block_data(block, &$($mutability)* mir[block]);
                }

                for scope in &$($mutability)* mir.visibility_scopes {
                    self.visit_visibility_scope_data(scope);
                }

                self.visit_ty(&$($mutability)* mir.return_ty);

                for local_decl in &$($mutability)* mir.local_decls {
                    self.visit_local_decl(local_decl);
                }

                self.visit_span(&$($mutability)* mir.span);
            }

            fn super_basic_block_data(&mut self,
                                      block: BasicBlock,
                                      data: & $($mutability)* BasicBlockData<'tcx>) {
                let BasicBlockData {
                    ref $($mutability)* statements,
                    ref $($mutability)* terminator,
                    is_cleanup: _
                } = *data;

                let mut index = 0;
                for statement in statements {
                    let location = Location { block: block, statement_index: index };
                    self.visit_statement(block, statement, location);
                    index += 1;
                }

                if let Some(ref $($mutability)* terminator) = *terminator {
                    let location = Location { block: block, statement_index: index };
                    self.visit_terminator(block, terminator, location);
                }
            }

            fn super_visibility_scope_data(&mut self,
                                           scope_data: & $($mutability)* VisibilityScopeData) {
                let VisibilityScopeData {
                    ref $($mutability)* span,
                    ref $($mutability)* parent_scope,
                } = *scope_data;

                self.visit_span(span);
                if let Some(ref $($mutability)* parent_scope) = *parent_scope {
                    self.visit_visibility_scope(parent_scope);
                }
            }

            fn super_statement(&mut self,
                               block: BasicBlock,
                               statement: & $($mutability)* Statement<'tcx>,
                               location: Location) {
                let Statement {
                    ref $($mutability)* source_info,
                    ref $($mutability)* kind,
                } = *statement;

                self.visit_source_info(source_info);
                match *kind {
                    StatementKind::Assign(ref $($mutability)* lvalue,
                                          ref $($mutability)* rvalue) => {
                        self.visit_assign(block, lvalue, rvalue, location);
                    }
                    StatementKind::SetDiscriminant{ ref $($mutability)* lvalue, .. } => {
                        self.visit_lvalue(lvalue, LvalueContext::Store, location);
                    }
                    StatementKind::StorageLive(ref $($mutability)* lvalue) => {
                        self.visit_lvalue(lvalue, LvalueContext::StorageLive, location);
                    }
                    StatementKind::StorageDead(ref $($mutability)* lvalue) => {
                        self.visit_lvalue(lvalue, LvalueContext::StorageDead, location);
                    }
                    StatementKind::Nop => {}
                }
            }

            fn super_assign(&mut self,
                            _block: BasicBlock,
                            lvalue: &$($mutability)* Lvalue<'tcx>,
                            rvalue: &$($mutability)* Rvalue<'tcx>,
                            location: Location) {
                self.visit_lvalue(lvalue, LvalueContext::Store, location);
                self.visit_rvalue(rvalue, location);
            }

            fn super_terminator(&mut self,
                                block: BasicBlock,
                                terminator: &$($mutability)* Terminator<'tcx>,
                                location: Location) {
                let Terminator {
                    ref $($mutability)* source_info,
                    ref $($mutability)* kind,
                } = *terminator;

                self.visit_source_info(source_info);
                self.visit_terminator_kind(block, kind, location);
            }

            fn super_terminator_kind(&mut self,
                                     block: BasicBlock,
                                     kind: & $($mutability)* TerminatorKind<'tcx>,
                                     source_location: Location) {
                match *kind {
                    TerminatorKind::Goto { target } => {
                        self.visit_branch(block, target);
                    }

                    TerminatorKind::If { ref $($mutability)* cond,
                                         ref $($mutability)* targets } => {
                        self.visit_operand(cond, source_location);
                        for &target in targets.as_slice() {
                            self.visit_branch(block, target);
                        }
                    }

                    TerminatorKind::Switch { ref $($mutability)* discr,
                                             adt_def: _,
                                             ref targets } => {
                        self.visit_lvalue(discr, LvalueContext::Inspect, source_location);
                        for &target in targets {
                            self.visit_branch(block, target);
                        }
                    }

                    TerminatorKind::SwitchInt { ref $($mutability)* discr,
                                                ref $($mutability)* switch_ty,
                                                ref $($mutability)* values,
                                                ref targets } => {
                        self.visit_lvalue(discr, LvalueContext::Inspect, source_location);
                        self.visit_ty(switch_ty);
                        for value in values {
                            self.visit_const_val(value, source_location);
                        }
                        for &target in targets {
                            self.visit_branch(block, target);
                        }
                    }

                    TerminatorKind::Resume |
                    TerminatorKind::Return |
                    TerminatorKind::Unreachable => {
                    }

                    TerminatorKind::Drop { ref $($mutability)* location,
                                           target,
                                           unwind } => {
                        self.visit_lvalue(location, LvalueContext::Drop, source_location);
                        self.visit_branch(block, target);
                        unwind.map(|t| self.visit_branch(block, t));
                    }

                    TerminatorKind::DropAndReplace { ref $($mutability)* location,
                                                     ref $($mutability)* value,
                                                     target,
                                                     unwind } => {
                        self.visit_lvalue(location, LvalueContext::Drop, source_location);
                        self.visit_operand(value, source_location);
                        self.visit_branch(block, target);
                        unwind.map(|t| self.visit_branch(block, t));
                    }

                    TerminatorKind::Call { ref $($mutability)* func,
                                           ref $($mutability)* args,
                                           ref $($mutability)* destination,
                                           cleanup } => {
                        self.visit_operand(func, source_location);
                        for arg in args {
                            self.visit_operand(arg, source_location);
                        }
                        if let Some((ref $($mutability)* destination, target)) = *destination {
                            self.visit_lvalue(destination, LvalueContext::Call, source_location);
                            self.visit_branch(block, target);
                        }
                        cleanup.map(|t| self.visit_branch(block, t));
                    }

                    TerminatorKind::Assert { ref $($mutability)* cond,
                                             expected: _,
                                             ref $($mutability)* msg,
                                             target,
                                             cleanup } => {
                        self.visit_operand(cond, source_location);
                        self.visit_assert_message(msg, source_location);
                        self.visit_branch(block, target);
                        cleanup.map(|t| self.visit_branch(block, t));
                    }
                }
            }

            fn super_assert_message(&mut self,
                                    msg: & $($mutability)* AssertMessage<'tcx>,
                                    location: Location) {
                match *msg {
                    AssertMessage::BoundsCheck {
                        ref $($mutability)* len,
                        ref $($mutability)* index
                    } => {
                        self.visit_operand(len, location);
                        self.visit_operand(index, location);
                    }
                    AssertMessage::Math(_) => {}
                }
            }

            fn super_rvalue(&mut self,
                            rvalue: & $($mutability)* Rvalue<'tcx>,
                            location: Location) {
                match *rvalue {
                    Rvalue::Use(ref $($mutability)* operand) => {
                        self.visit_operand(operand, location);
                    }

                    Rvalue::Repeat(ref $($mutability)* value,
                                   ref $($mutability)* typed_const_val) => {
                        self.visit_operand(value, location);
                        self.visit_typed_const_val(typed_const_val, location);
                    }

                    Rvalue::Ref(r, bk, ref $($mutability)* path) => {
                        self.visit_lvalue(path, LvalueContext::Borrow {
                            region: r,
                            kind: bk
                        }, location);
                    }

                    Rvalue::Len(ref $($mutability)* path) => {
                        self.visit_lvalue(path, LvalueContext::Inspect, location);
                    }

                    Rvalue::Cast(_cast_kind,
                                 ref $($mutability)* operand,
                                 ref $($mutability)* ty) => {
                        self.visit_operand(operand, location);
                        self.visit_ty(ty);
                    }

                    Rvalue::BinaryOp(_bin_op,
                                     ref $($mutability)* lhs,
                                     ref $($mutability)* rhs) |
                    Rvalue::CheckedBinaryOp(_bin_op,
                                     ref $($mutability)* lhs,
                                     ref $($mutability)* rhs) => {
                        self.visit_operand(lhs, location);
                        self.visit_operand(rhs, location);
                    }

                    Rvalue::UnaryOp(_un_op, ref $($mutability)* op) => {
                        self.visit_operand(op, location);
                    }

                    Rvalue::Box(ref $($mutability)* ty) => {
                        self.visit_ty(ty);
                    }

                    Rvalue::Aggregate(ref $($mutability)* kind,
                                      ref $($mutability)* operands) => {
                        match *kind {
                            AggregateKind::Array => {
                            }
                            AggregateKind::Tuple => {
                            }
                            AggregateKind::Adt(_adt_def,
                                               _variant_index,
                                               ref $($mutability)* substs,
                                               _active_field_index) => {
                                self.visit_substs(substs);
                            }
                            AggregateKind::Closure(ref $($mutability)* def_id,
                                                   ref $($mutability)* closure_substs) => {
                                self.visit_def_id(def_id, location);
                                self.visit_closure_substs(closure_substs);
                            }
                        }

                        for operand in operands {
                            self.visit_operand(operand, location);
                        }
                    }

                    Rvalue::InlineAsm { ref $($mutability)* outputs,
                                        ref $($mutability)* inputs,
                                        asm: _ } => {
                        for output in & $($mutability)* outputs[..] {
                            self.visit_lvalue(output, LvalueContext::Store, location);
                        }
                        for input in & $($mutability)* inputs[..] {
                            self.visit_operand(input, location);
                        }
                    }
                }
            }

            fn super_operand(&mut self,
                             operand: & $($mutability)* Operand<'tcx>,
                             location: Location) {
                match *operand {
                    Operand::Consume(ref $($mutability)* lvalue) => {
                        self.visit_lvalue(lvalue, LvalueContext::Consume, location);
                    }
                    Operand::Constant(ref $($mutability)* constant) => {
                        self.visit_constant(constant, location);
                    }
                }
            }

            fn super_lvalue(&mut self,
                            lvalue: & $($mutability)* Lvalue<'tcx>,
                            context: LvalueContext<'tcx>,
                            location: Location) {
                match *lvalue {
                    Lvalue::Local(_) => {
                    }
                    Lvalue::Static(ref $($mutability)* def_id) => {
                        self.visit_def_id(def_id, location);
                    }
                    Lvalue::Projection(ref $($mutability)* proj) => {
                        self.visit_projection(proj, context, location);
                    }
                }
            }

            fn super_projection(&mut self,
                                proj: & $($mutability)* LvalueProjection<'tcx>,
                                context: LvalueContext,
                                location: Location) {
                let Projection {
                    ref $($mutability)* base,
                    ref $($mutability)* elem,
                } = *proj;
                let context = if context.is_mutating_use() {
                    LvalueContext::Projection(Mutability::Mut)
                } else {
                    LvalueContext::Projection(Mutability::Not)
                };
                self.visit_lvalue(base, context, location);
                self.visit_projection_elem(elem, context, location);
            }

            fn super_projection_elem(&mut self,
                                     proj: & $($mutability)* LvalueElem<'tcx>,
                                     _context: LvalueContext,
                                     location: Location) {
                match *proj {
                    ProjectionElem::Deref => {
                    }
                    ProjectionElem::Subslice { from: _, to: _ } => {
                    }
                    ProjectionElem::Field(_field, ref $($mutability)* ty) => {
                        self.visit_ty(ty);
                    }
                    ProjectionElem::Index(ref $($mutability)* operand) => {
                        self.visit_operand(operand, location);
                    }
                    ProjectionElem::ConstantIndex { offset: _,
                                                    min_length: _,
                                                    from_end: _ } => {
                    }
                    ProjectionElem::Downcast(_adt_def, _variant_index) => {
                    }
                }
            }

            fn super_local_decl(&mut self,
                                local_decl: & $($mutability)* LocalDecl<'tcx>) {
                let LocalDecl {
                    mutability: _,
                    ref $($mutability)* ty,
                    name: _,
                    ref $($mutability)* source_info,
                } = *local_decl;

                self.visit_ty(ty);
                if let Some(ref $($mutability)* info) = *source_info {
                    self.visit_source_info(info);
                }
            }

            fn super_visibility_scope(&mut self,
                                      _scope: & $($mutability)* VisibilityScope) {
            }

            fn super_branch(&mut self,
                            _source: BasicBlock,
                            _target: BasicBlock) {
            }

            fn super_constant(&mut self,
                              constant: & $($mutability)* Constant<'tcx>,
                              location: Location) {
                let Constant {
                    ref $($mutability)* span,
                    ref $($mutability)* ty,
                    ref $($mutability)* literal,
                } = *constant;

                self.visit_span(span);
                self.visit_ty(ty);
                self.visit_literal(literal, location);
            }

            fn super_typed_const_val(&mut self,
                                     constant: & $($mutability)* TypedConstVal<'tcx>,
                                     location: Location) {
                let TypedConstVal {
                    ref $($mutability)* span,
                    ref $($mutability)* ty,
                    ref $($mutability)* value,
                } = *constant;

                self.visit_span(span);
                self.visit_ty(ty);
                self.visit_const_usize(value, location);
            }

            fn super_literal(&mut self,
                             literal: & $($mutability)* Literal<'tcx>,
                             location: Location) {
                match *literal {
                    Literal::Item { ref $($mutability)* def_id,
                                    ref $($mutability)* substs } => {
                        self.visit_def_id(def_id, location);
                        self.visit_substs(substs);
                    }
                    Literal::Value { ref $($mutability)* value } => {
                        self.visit_const_val(value, location);
                    }
                    Literal::Promoted { index: _ } => {}
                }
            }

            fn super_def_id(&mut self, _def_id: & $($mutability)* DefId) {
            }

            fn super_span(&mut self, _span: & $($mutability)* Span) {
            }

            fn super_source_info(&mut self, source_info: & $($mutability)* SourceInfo) {
                let SourceInfo {
                    ref $($mutability)* span,
                    ref $($mutability)* scope,
                } = *source_info;

                self.visit_span(span);
                self.visit_visibility_scope(scope);
            }

            fn super_ty(&mut self, _ty: & $($mutability)* Ty<'tcx>) {
            }

            fn super_substs(&mut self, _substs: & $($mutability)* &'tcx Substs<'tcx>) {
            }

            fn super_closure_substs(&mut self,
                                    _substs: & $($mutability)* ClosureSubsts<'tcx>) {
            }

            fn super_const_val(&mut self, _substs: & $($mutability)* ConstVal) {
            }

            fn super_const_usize(&mut self, _substs: & $($mutability)* ConstUsize) {
            }

            // Convenience methods

            fn visit_location(&mut self, mir: & $($mutability)* Mir<'tcx>, location: Location) {
                let basic_block = & $($mutability)* mir[location.block];
                if basic_block.statements.len() == location.statement_index {
                    if let Some(ref $($mutability)* terminator) = basic_block.terminator {
                        self.visit_terminator(location.block, terminator, location)
                    }
                } else {
                    let statement = & $($mutability)*
                        basic_block.statements[location.statement_index];
                    self.visit_statement(location.block, statement, location)
                }
            }
        }
    }
}

make_mir_visitor!(Visitor,);
make_mir_visitor!(MutVisitor,mut);

#[derive(Copy, Clone, Debug)]
pub enum LvalueContext<'tcx> {
    // Appears as LHS of an assignment
    Store,

    // Dest of a call
    Call,

    // Being dropped
    Drop,

    // Being inspected in some way, like loading a len
    Inspect,

    // Being borrowed
    Borrow { region: &'tcx Region, kind: BorrowKind },

    // Used as base for another lvalue, e.g. `x` in `x.y`.
    //
    // The `Mutability` argument specifies whether the projection is being performed in order to
    // (potentially) mutate the lvalue. For example, the projection `x.y` is marked as a mutation
    // in these cases:
    //
    //     x.y = ...;
    //     f(&mut x.y);
    //
    // But not in these cases:
    //
    //     z = x.y;
    //     f(&x.y);
    Projection(Mutability),

    // Consumed as part of an operand
    Consume,

    // Starting and ending a storage live range
    StorageLive,
    StorageDead,
}

impl<'tcx> LvalueContext<'tcx> {
    /// Returns true if this lvalue context represents a drop.
    pub fn is_drop(&self) -> bool {
        match *self {
            LvalueContext::Drop => true,
            _ => false,
        }
    }

    /// Returns true if this lvalue context represents a storage live or storage dead marker.
    pub fn is_storage_marker(&self) -> bool {
        match *self {
            LvalueContext::StorageLive | LvalueContext::StorageDead => true,
            _ => false,
        }
    }

    /// Returns true if this lvalue context represents a storage live marker.
    pub fn is_storage_live_marker(&self) -> bool {
        match *self {
            LvalueContext::StorageLive => true,
            _ => false,
        }
    }

    /// Returns true if this lvalue context represents a storage dead marker.
    pub fn is_storage_dead_marker(&self) -> bool {
        match *self {
            LvalueContext::StorageDead => true,
            _ => false,
        }
    }

    /// Returns true if this lvalue context represents a use that potentially changes the value.
    pub fn is_mutating_use(&self) -> bool {
        match *self {
            LvalueContext::Store | LvalueContext::Call |
            LvalueContext::Borrow { kind: BorrowKind::Mut, .. } |
            LvalueContext::Projection(Mutability::Mut) |
            LvalueContext::Drop => true,
            LvalueContext::Inspect |
            LvalueContext::Borrow { kind: BorrowKind::Shared, .. } |
            LvalueContext::Borrow { kind: BorrowKind::Unique, .. } |
            LvalueContext::Projection(Mutability::Not) | LvalueContext::Consume |
            LvalueContext::StorageLive | LvalueContext::StorageDead => false,
        }
    }

    /// Returns true if this lvalue context represents a use that does not change the value.
    pub fn is_nonmutating_use(&self) -> bool {
        match *self {
            LvalueContext::Inspect | LvalueContext::Borrow { kind: BorrowKind::Shared, .. } |
            LvalueContext::Borrow { kind: BorrowKind::Unique, .. } |
            LvalueContext::Projection(Mutability::Not) | LvalueContext::Consume => true,
            LvalueContext::Borrow { kind: BorrowKind::Mut, .. } | LvalueContext::Store |
            LvalueContext::Call | LvalueContext::Projection(Mutability::Mut) |
            LvalueContext::Drop | LvalueContext::StorageLive | LvalueContext::StorageDead => false,
        }
    }

    pub fn is_use(&self) -> bool {
        self.is_mutating_use() || self.is_nonmutating_use()
    }
}

