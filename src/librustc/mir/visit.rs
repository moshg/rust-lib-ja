use crate::ty::subst::SubstsRef;
use crate::ty::{CanonicalUserTypeAnnotation, ClosureSubsts, GeneratorSubsts, Ty};
use crate::mir::*;
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
// MIR, e.g., types, spans, etc, but simply visit them and stop. This
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
//                           data: & $($mutability)? BasicBlockData<'tcx>) {
//     let BasicBlockData {
//         statements,
//         terminator,
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
    ($visitor_trait_name:ident, $($mutability:ident)?) => {
        pub trait $visitor_trait_name<'tcx> {
            // Override these, and call `self.super_xxx` to revert back to the
            // default behavior.

            fn visit_body(&mut self, mir: & $($mutability)? Body<'tcx>) {
                self.super_body(mir);
            }

            fn visit_basic_block_data(&mut self,
                                      block: BasicBlock,
                                      data: & $($mutability)? BasicBlockData<'tcx>) {
                self.super_basic_block_data(block, data);
            }

            fn visit_source_scope_data(&mut self,
                                           scope_data: & $($mutability)? SourceScopeData) {
                self.super_source_scope_data(scope_data);
            }

            fn visit_statement(&mut self,
                               statement: & $($mutability)? Statement<'tcx>,
                               location: Location) {
                self.super_statement(statement, location);
            }

            fn visit_assign(&mut self,
                            place: & $($mutability)? Place<'tcx>,
                            rvalue: & $($mutability)? Rvalue<'tcx>,
                            location: Location) {
                self.super_assign(place, rvalue, location);
            }

            fn visit_terminator(&mut self,
                                terminator: & $($mutability)? Terminator<'tcx>,
                                location: Location) {
                self.super_terminator(terminator, location);
            }

            fn visit_terminator_kind(&mut self,
                                     kind: & $($mutability)? TerminatorKind<'tcx>,
                                     location: Location) {
                self.super_terminator_kind(kind, location);
            }

            fn visit_assert_message(&mut self,
                                    msg: & $($mutability)? AssertMessage<'tcx>,
                                    location: Location) {
                self.super_assert_message(msg, location);
            }

            fn visit_rvalue(&mut self,
                            rvalue: & $($mutability)? Rvalue<'tcx>,
                            location: Location) {
                self.super_rvalue(rvalue, location);
            }

            fn visit_operand(&mut self,
                             operand: & $($mutability)? Operand<'tcx>,
                             location: Location) {
                self.super_operand(operand, location);
            }

            fn visit_ascribe_user_ty(&mut self,
                                     place: & $($mutability)? Place<'tcx>,
                                     variance: & $($mutability)? ty::Variance,
                                     user_ty: & $($mutability)? UserTypeProjection,
                                     location: Location) {
                self.super_ascribe_user_ty(place, variance, user_ty, location);
            }

            fn visit_retag(&mut self,
                           kind: & $($mutability)? RetagKind,
                           place: & $($mutability)? Place<'tcx>,
                           location: Location) {
                self.super_retag(kind, place, location);
            }

            fn visit_place(&mut self,
                            place: & $($mutability)? Place<'tcx>,
                            context: PlaceContext,
                            location: Location) {
                self.super_place(place, context, location);
            }

            fn visit_place_base(&mut self,
                                place_base: & $($mutability)? PlaceBase<'tcx>,
                                context: PlaceContext,
                                location: Location) {
                self.super_place_base(place_base, context, location);
            }

            fn visit_projection(&mut self,
                                place: & $($mutability)? Projection<'tcx>,
                                location: Location) {
                self.super_projection(place, location);
            }

            fn visit_constant(&mut self,
                              constant: & $($mutability)? Constant<'tcx>,
                              location: Location) {
                self.super_constant(constant, location);
            }

            fn visit_span(&mut self,
                          span: & $($mutability)? Span) {
                self.super_span(span);
            }

            fn visit_source_info(&mut self,
                                 source_info: & $($mutability)? SourceInfo) {
                self.super_source_info(source_info);
            }

            fn visit_ty(&mut self,
                        ty: $(& $mutability)? Ty<'tcx>,
                        _: TyContext) {
                self.super_ty(ty);
            }

            fn visit_user_type_projection(
                &mut self,
                ty: & $($mutability)? UserTypeProjection,
            ) {
                self.super_user_type_projection(ty);
            }

            fn visit_user_type_annotation(
                &mut self,
                index: UserTypeAnnotationIndex,
                ty: & $($mutability)? CanonicalUserTypeAnnotation<'tcx>,
            ) {
                self.super_user_type_annotation(index, ty);
            }

            fn visit_region(&mut self,
                            region: & $($mutability)? ty::Region<'tcx>,
                            _: Location) {
                self.super_region(region);
            }

            fn visit_const(&mut self,
                           constant: & $($mutability)? &'tcx ty::Const<'tcx>,
                           _: Location) {
                self.super_const(constant);
            }

            fn visit_substs(&mut self,
                            substs: & $($mutability)? SubstsRef<'tcx>,
                            _: Location) {
                self.super_substs(substs);
            }

            fn visit_closure_substs(&mut self,
                                    substs: & $($mutability)? ClosureSubsts<'tcx>,
                                    _: Location) {
                self.super_closure_substs(substs);
            }

            fn visit_generator_substs(&mut self,
                                      substs: & $($mutability)? GeneratorSubsts<'tcx>,
                                    _: Location) {
                self.super_generator_substs(substs);
            }

            fn visit_local_decl(&mut self,
                                local: Local,
                                local_decl: & $($mutability)? LocalDecl<'tcx>) {
                self.super_local_decl(local, local_decl);
            }

            fn visit_local(&mut self,
                            _local: & $($mutability)? Local,
                            _context: PlaceContext,
                            _location: Location) {
            }

            fn visit_source_scope(&mut self,
                                      scope: & $($mutability)? SourceScope) {
                self.super_source_scope(scope);
            }

            // The `super_xxx` methods comprise the default behavior and are
            // not meant to be overridden.

            fn super_body(&mut self,
                         mir: & $($mutability)? Body<'tcx>) {
                if let Some(yield_ty) = &$($mutability)? mir.yield_ty {
                    self.visit_ty(yield_ty, TyContext::YieldTy(SourceInfo {
                        span: mir.span,
                        scope: OUTERMOST_SOURCE_SCOPE,
                    }));
                }

                // for best performance, we want to use an iterator rather
                // than a for-loop, to avoid calling `mir::Body::invalidate` for
                // each basic block.
                macro_rules! basic_blocks {
                    (mut) => (mir.basic_blocks_mut().iter_enumerated_mut());
                    () => (mir.basic_blocks().iter_enumerated());
                };
                for (bb, data) in basic_blocks!($($mutability)?) {
                    self.visit_basic_block_data(bb, data);
                }

                for scope in &$($mutability)? mir.source_scopes {
                    self.visit_source_scope_data(scope);
                }

                self.visit_ty(&$($mutability)? mir.return_ty(), TyContext::ReturnTy(SourceInfo {
                    span: mir.span,
                    scope: OUTERMOST_SOURCE_SCOPE,
                }));

                for local in mir.local_decls.indices() {
                    self.visit_local_decl(local, & $($mutability)? mir.local_decls[local]);
                }

                macro_rules! type_annotations {
                    (mut) => (mir.user_type_annotations.iter_enumerated_mut());
                    () => (mir.user_type_annotations.iter_enumerated());
                };

                for (index, annotation) in type_annotations!($($mutability)?) {
                    self.visit_user_type_annotation(
                        index, annotation
                    );
                }

                self.visit_span(&$($mutability)? mir.span);
            }

            fn super_basic_block_data(&mut self,
                                      block: BasicBlock,
                                      data: & $($mutability)? BasicBlockData<'tcx>) {
                let BasicBlockData {
                    statements,
                    terminator,
                    is_cleanup: _
                } = data;

                let mut index = 0;
                for statement in statements {
                    let location = Location { block: block, statement_index: index };
                    self.visit_statement(statement, location);
                    index += 1;
                }

                if let Some(terminator) = terminator {
                    let location = Location { block: block, statement_index: index };
                    self.visit_terminator(terminator, location);
                }
            }

            fn super_source_scope_data(&mut self, scope_data: & $($mutability)? SourceScopeData) {
                let SourceScopeData {
                    span,
                    parent_scope,
                } = scope_data;

                self.visit_span(span);
                if let Some(parent_scope) = parent_scope {
                    self.visit_source_scope(parent_scope);
                }
            }

            fn super_statement(&mut self,
                               statement: & $($mutability)? Statement<'tcx>,
                               location: Location) {
                let Statement {
                    source_info,
                    kind,
                } = statement;

                self.visit_source_info(source_info);
                match kind {
                    StatementKind::Assign(place, rvalue) => {
                        self.visit_assign(place, rvalue, location);
                    }
                    StatementKind::FakeRead(_, place) => {
                        self.visit_place(
                            place,
                            PlaceContext::NonMutatingUse(NonMutatingUseContext::Inspect),
                            location
                        );
                    }
                    StatementKind::SetDiscriminant { place, .. } => {
                        self.visit_place(
                            place,
                            PlaceContext::MutatingUse(MutatingUseContext::Store),
                            location
                        );
                    }
                    StatementKind::StorageLive(local) => {
                        self.visit_local(
                            local,
                            PlaceContext::NonUse(NonUseContext::StorageLive),
                            location
                        );
                    }
                    StatementKind::StorageDead(local) => {
                        self.visit_local(
                            local,
                            PlaceContext::NonUse(NonUseContext::StorageDead),
                            location
                        );
                    }
                    StatementKind::InlineAsm(asm) => {
                        for output in & $($mutability)? asm.outputs[..] {
                            self.visit_place(
                                output,
                                PlaceContext::MutatingUse(MutatingUseContext::AsmOutput),
                                location
                            );
                        }
                        for (span, input) in & $($mutability)? asm.inputs[..] {
                            self.visit_span(span);
                            self.visit_operand(input, location);
                        }
                    }
                    StatementKind::Retag(kind, place) => {
                        self.visit_retag(kind, place, location);
                    }
                    StatementKind::AscribeUserType(place, variance, user_ty) => {
                        self.visit_ascribe_user_ty(place, variance, user_ty, location);
                    }
                    StatementKind::Nop => {}
                }
            }

            fn super_assign(&mut self,
                            place: &$($mutability)? Place<'tcx>,
                            rvalue: &$($mutability)? Rvalue<'tcx>,
                            location: Location) {
                self.visit_place(
                    place,
                    PlaceContext::MutatingUse(MutatingUseContext::Store),
                    location
                );
                self.visit_rvalue(rvalue, location);
            }

            fn super_terminator(&mut self,
                                terminator: &$($mutability)? Terminator<'tcx>,
                                location: Location) {
                let Terminator { source_info, kind } = terminator;

                self.visit_source_info(source_info);
                self.visit_terminator_kind(kind, location);
            }

            fn super_terminator_kind(&mut self,
                                     kind: & $($mutability)? TerminatorKind<'tcx>,
                                     source_location: Location) {
                match kind {
                    TerminatorKind::Goto { .. } |
                    TerminatorKind::Resume |
                    TerminatorKind::Abort |
                    TerminatorKind::Return |
                    TerminatorKind::GeneratorDrop |
                    TerminatorKind::Unreachable |
                    TerminatorKind::FalseEdges { .. } |
                    TerminatorKind::FalseUnwind { .. } => {
                    }

                    TerminatorKind::SwitchInt {
                        discr,
                        switch_ty,
                        values: _,
                        targets: _
                    } => {
                        self.visit_operand(discr, source_location);
                        self.visit_ty(switch_ty, TyContext::Location(source_location));
                    }

                    TerminatorKind::Drop {
                        location,
                        target: _,
                        unwind: _,
                    } => {
                        self.visit_place(
                            location,
                            PlaceContext::MutatingUse(MutatingUseContext::Drop),
                            source_location
                        );
                    }

                    TerminatorKind::DropAndReplace {
                        location,
                        value,
                        target: _,
                        unwind: _,
                    } => {
                        self.visit_place(
                            location,
                            PlaceContext::MutatingUse(MutatingUseContext::Drop),
                            source_location
                        );
                        self.visit_operand(value, source_location);
                    }

                    TerminatorKind::Call {
                        func,
                        args,
                        destination,
                        cleanup: _,
                        from_hir_call: _,
                    } => {
                        self.visit_operand(func, source_location);
                        for arg in args {
                            self.visit_operand(arg, source_location);
                        }
                        if let Some((destination, _)) = destination {
                            self.visit_place(
                                destination,
                                PlaceContext::MutatingUse(MutatingUseContext::Call),
                                source_location
                            );
                        }
                    }

                    TerminatorKind::Assert {
                        cond,
                        expected: _,
                        msg,
                        target: _,
                        cleanup: _,
                    } => {
                        self.visit_operand(cond, source_location);
                        self.visit_assert_message(msg, source_location);
                    }

                    TerminatorKind::Yield {
                        value,
                        resume: _,
                        drop: _,
                    } => {
                        self.visit_operand(value, source_location);
                    }

                }
            }

            fn super_assert_message(&mut self,
                                    msg: & $($mutability)? AssertMessage<'tcx>,
                                    location: Location) {
                use crate::mir::interpret::InterpError::*;
                if let BoundsCheck { len, index } = msg {
                    self.visit_operand(len, location);
                    self.visit_operand(index, location);
                }
            }

            fn super_rvalue(&mut self,
                            rvalue: & $($mutability)? Rvalue<'tcx>,
                            location: Location) {
                match rvalue {
                    Rvalue::Use(operand) => {
                        self.visit_operand(operand, location);
                    }

                    Rvalue::Repeat(value, _) => {
                        self.visit_operand(value, location);
                    }

                    Rvalue::Ref(r, bk, path) => {
                        self.visit_region(r, location);
                        let ctx = match bk {
                            BorrowKind::Shared => PlaceContext::NonMutatingUse(
                                NonMutatingUseContext::SharedBorrow
                            ),
                            BorrowKind::Shallow => PlaceContext::NonMutatingUse(
                                NonMutatingUseContext::ShallowBorrow
                            ),
                            BorrowKind::Unique => PlaceContext::NonMutatingUse(
                                NonMutatingUseContext::UniqueBorrow
                            ),
                            BorrowKind::Mut { .. } =>
                                PlaceContext::MutatingUse(MutatingUseContext::Borrow),
                        };
                        self.visit_place(path, ctx, location);
                    }

                    Rvalue::Len(path) => {
                        self.visit_place(
                            path,
                            PlaceContext::NonMutatingUse(NonMutatingUseContext::Inspect),
                            location
                        );
                    }

                    Rvalue::Cast(_cast_kind, operand, ty) => {
                        self.visit_operand(operand, location);
                        self.visit_ty(ty, TyContext::Location(location));
                    }

                    Rvalue::BinaryOp(_bin_op, lhs, rhs)
                    | Rvalue::CheckedBinaryOp(_bin_op, lhs, rhs) => {
                        self.visit_operand(lhs, location);
                        self.visit_operand(rhs, location);
                    }

                    Rvalue::UnaryOp(_un_op, op) => {
                        self.visit_operand(op, location);
                    }

                    Rvalue::Discriminant(place) => {
                        self.visit_place(
                            place,
                            PlaceContext::NonMutatingUse(NonMutatingUseContext::Inspect),
                            location
                        );
                    }

                    Rvalue::NullaryOp(_op, ty) => {
                        self.visit_ty(ty, TyContext::Location(location));
                    }

                    Rvalue::Aggregate(kind, operands) => {
                        let kind = &$($mutability)? **kind;
                        match kind {
                            AggregateKind::Array(ty) => {
                                self.visit_ty(ty, TyContext::Location(location));
                            }
                            AggregateKind::Tuple => {
                            }
                            AggregateKind::Adt(
                                _adt_def,
                                _variant_index,
                                substs,
                                _user_substs,
                                _active_field_index
                            ) => {
                                self.visit_substs(substs, location);
                            }
                            AggregateKind::Closure(
                                _,
                                closure_substs
                            ) => {
                                self.visit_closure_substs(closure_substs, location);
                            }
                            AggregateKind::Generator(
                                _,
                                generator_substs,
                                _movability,
                            ) => {
                                self.visit_generator_substs(generator_substs, location);
                            }
                        }

                        for operand in operands {
                            self.visit_operand(operand, location);
                        }
                    }
                }
            }

            fn super_operand(&mut self,
                             operand: & $($mutability)? Operand<'tcx>,
                             location: Location) {
                match operand {
                    Operand::Copy(place) => {
                        self.visit_place(
                            place,
                            PlaceContext::NonMutatingUse(NonMutatingUseContext::Copy),
                            location
                        );
                    }
                    Operand::Move(place) => {
                        self.visit_place(
                            place,
                            PlaceContext::NonMutatingUse(NonMutatingUseContext::Move),
                            location
                        );
                    }
                    Operand::Constant(constant) => {
                        self.visit_constant(constant, location);
                    }
                }
            }

            fn super_ascribe_user_ty(&mut self,
                                     place: & $($mutability)? Place<'tcx>,
                                     _variance: & $($mutability)? ty::Variance,
                                     user_ty: & $($mutability)? UserTypeProjection,
                                     location: Location) {
                self.visit_place(
                    place,
                    PlaceContext::NonUse(NonUseContext::AscribeUserTy),
                    location
                );
                self.visit_user_type_projection(user_ty);
            }

            fn super_retag(&mut self,
                           _kind: & $($mutability)? RetagKind,
                           place: & $($mutability)? Place<'tcx>,
                           location: Location) {
                self.visit_place(
                    place,
                    PlaceContext::MutatingUse(MutatingUseContext::Retag),
                    location,
                );
            }

            fn super_place(&mut self,
                            place: & $($mutability)? Place<'tcx>,
                            context: PlaceContext,
                            location: Location) {
                match place {
                    Place::Base(place_base) => {
                        self.visit_place_base(place_base, context, location);
                    }
                    Place::Projection(proj) => {
                        let context = if context.is_mutating_use() {
                            PlaceContext::MutatingUse(MutatingUseContext::Projection)
                        } else {
                            PlaceContext::NonMutatingUse(NonMutatingUseContext::Projection)
                        };

                        self.visit_place(& $($mutability)? proj.base, context, location);
                        self.visit_projection(proj, location);
                    }
                }
            }

            fn super_place_base(&mut self,
                                place_base: & $($mutability)? PlaceBase<'tcx>,
                                context: PlaceContext,
                                location: Location) {
                match place_base {
                    PlaceBase::Local(local) => {
                        self.visit_local(local, context, location);
                    }
                    PlaceBase::Static(box Static { kind: _, ty }) => {
                        self.visit_ty(& $($mutability)? *ty, TyContext::Location(location));
                    }
                }
            }

            fn super_projection(&mut self,
                                proj: & $($mutability)? Projection<'tcx>,
                                location: Location) {
                match & $($mutability)? proj.elem {
                    ProjectionElem::Deref => {
                    }
                    ProjectionElem::Subslice { from: _, to: _ } => {
                    }
                    ProjectionElem::Field(_field, ty) => {
                        self.visit_ty(ty, TyContext::Location(location));
                    }
                    ProjectionElem::Index(local) => {
                        self.visit_local(
                            local,
                            PlaceContext::NonMutatingUse(NonMutatingUseContext::Copy),
                            location
                        );
                    }
                    ProjectionElem::ConstantIndex { offset: _,
                                                    min_length: _,
                                                    from_end: _ } => {
                    }
                    ProjectionElem::Downcast(_name, _variant_index) => {
                    }
                }
            }

            fn super_local_decl(&mut self,
                                local: Local,
                                local_decl: & $($mutability)? LocalDecl<'tcx>) {
                let LocalDecl {
                    mutability: _,
                    ty,
                    user_ty,
                    name: _,
                    source_info,
                    visibility_scope,
                    internal: _,
                    is_user_variable: _,
                    is_block_tail: _,
                } = local_decl;

                self.visit_ty(ty, TyContext::LocalDecl {
                    local,
                    source_info: *source_info,
                });
                for (user_ty, _) in & $($mutability)? user_ty.contents {
                    self.visit_user_type_projection(user_ty);
                }
                self.visit_source_info(source_info);
                self.visit_source_scope(visibility_scope);
            }

            fn super_source_scope(&mut self,
                                      _scope: & $($mutability)? SourceScope) {
            }

            fn super_constant(&mut self,
                              constant: & $($mutability)? Constant<'tcx>,
                              location: Location) {
                let Constant {
                    span,
                    ty,
                    user_ty,
                    literal,
                } = constant;

                self.visit_span(span);
                self.visit_ty(ty, TyContext::Location(location));
                drop(user_ty); // no visit method for this
                self.visit_const(literal, location);
            }

            fn super_span(&mut self, _span: & $($mutability)? Span) {
            }

            fn super_source_info(&mut self, source_info: & $($mutability)? SourceInfo) {
                let SourceInfo {
                    span,
                    scope,
                } = source_info;

                self.visit_span(span);
                self.visit_source_scope(scope);
            }

            fn super_user_type_projection(
                &mut self,
                _ty: & $($mutability)? UserTypeProjection,
            ) {
            }

            fn super_user_type_annotation(
                &mut self,
                _index: UserTypeAnnotationIndex,
                ty: & $($mutability)? CanonicalUserTypeAnnotation<'tcx>,
            ) {
                self.visit_span(& $($mutability)? ty.span);
                self.visit_ty(& $($mutability)? ty.inferred_ty, TyContext::UserTy(ty.span));
            }

            fn super_ty(&mut self, _ty: $(& $mutability)? Ty<'tcx>) {
            }

            fn super_region(&mut self, _region: & $($mutability)? ty::Region<'tcx>) {
            }

            fn super_const(&mut self, _const: & $($mutability)? &'tcx ty::Const<'tcx>) {
            }

            fn super_substs(&mut self, _substs: & $($mutability)? SubstsRef<'tcx>) {
            }

            fn super_generator_substs(&mut self,
                                      _substs: & $($mutability)? GeneratorSubsts<'tcx>) {
            }

            fn super_closure_substs(&mut self,
                                    _substs: & $($mutability)? ClosureSubsts<'tcx>) {
            }

            // Convenience methods

            fn visit_location(&mut self, mir: & $($mutability)? Body<'tcx>, location: Location) {
                let basic_block = & $($mutability)? mir[location.block];
                if basic_block.statements.len() == location.statement_index {
                    if let Some(ref $($mutability)? terminator) = basic_block.terminator {
                        self.visit_terminator(terminator, location)
                    }
                } else {
                    let statement = & $($mutability)?
                        basic_block.statements[location.statement_index];
                    self.visit_statement(statement, location)
                }
            }
        }
    }
}

make_mir_visitor!(Visitor,);
make_mir_visitor!(MutVisitor,mut);

pub trait MirVisitable<'tcx> {
    fn apply(&self, location: Location, visitor: &mut dyn Visitor<'tcx>);
}

impl<'tcx> MirVisitable<'tcx> for Statement<'tcx> {
    fn apply(&self, location: Location, visitor: &mut dyn Visitor<'tcx>)
    {
        visitor.visit_statement(self, location)
    }
}

impl<'tcx> MirVisitable<'tcx> for Terminator<'tcx> {
    fn apply(&self, location: Location, visitor: &mut dyn Visitor<'tcx>)
    {
        visitor.visit_terminator(self, location)
    }
}

impl<'tcx> MirVisitable<'tcx> for Option<Terminator<'tcx>> {
    fn apply(&self, location: Location, visitor: &mut dyn Visitor<'tcx>)
    {
        visitor.visit_terminator(self.as_ref().unwrap(), location)
    }
}

/// Extra information passed to `visit_ty` and friends to give context
/// about where the type etc appears.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum TyContext {
    LocalDecl {
        /// The index of the local variable we are visiting.
        local: Local,

        /// The source location where this local variable was declared.
        source_info: SourceInfo,
    },

    /// The inferred type of a user type annotation.
    UserTy(Span),

    /// The return type of the function.
    ReturnTy(SourceInfo),

    YieldTy(SourceInfo),

    /// A type found at some location.
    Location(Location),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum NonMutatingUseContext {
    /// Being inspected in some way, like loading a len.
    Inspect,
    /// Consumed as part of an operand.
    Copy,
    /// Consumed as part of an operand.
    Move,
    /// Shared borrow.
    SharedBorrow,
    /// Shallow borrow.
    ShallowBorrow,
    /// Unique borrow.
    UniqueBorrow,
    /// Used as base for another place, e.g., `x` in `x.y`. Will not mutate the place.
    /// For example, the projection `x.y` is not marked as a mutation in these cases:
    ///
    ///     z = x.y;
    ///     f(&x.y);
    ///
    Projection,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MutatingUseContext {
    /// Appears as LHS of an assignment.
    Store,
    /// Can often be treated as a `Store`, but needs to be separate because
    /// ASM is allowed to read outputs as well, so a `Store`-`AsmOutput` sequence
    /// cannot be simplified the way a `Store`-`Store` can be.
    AsmOutput,
    /// Destination of a call.
    Call,
    /// Being dropped.
    Drop,
    /// Mutable borrow.
    Borrow,
    /// Used as base for another place, e.g., `x` in `x.y`. Could potentially mutate the place.
    /// For example, the projection `x.y` is marked as a mutation in these cases:
    ///
    ///     x.y = ...;
    ///     f(&mut x.y);
    ///
    Projection,
    /// Retagging, a "Stacked Borrows" shadow state operation
    Retag,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum NonUseContext {
    /// Starting a storage live range.
    StorageLive,
    /// Ending a storage live range.
    StorageDead,
    /// User type annotation assertions for NLL.
    AscribeUserTy,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PlaceContext {
    NonMutatingUse(NonMutatingUseContext),
    MutatingUse(MutatingUseContext),
    NonUse(NonUseContext),
}

impl<'tcx> PlaceContext {
    /// Returns `true` if this place context represents a drop.
    pub fn is_drop(&self) -> bool {
        match *self {
            PlaceContext::MutatingUse(MutatingUseContext::Drop) => true,
            _ => false,
        }
    }

    /// Returns `true` if this place context represents a borrow.
    pub fn is_borrow(&self) -> bool {
        match *self {
            PlaceContext::NonMutatingUse(NonMutatingUseContext::SharedBorrow) |
            PlaceContext::NonMutatingUse(NonMutatingUseContext::ShallowBorrow) |
            PlaceContext::NonMutatingUse(NonMutatingUseContext::UniqueBorrow) |
            PlaceContext::MutatingUse(MutatingUseContext::Borrow) => true,
            _ => false,
        }
    }

    /// Returns `true` if this place context represents a storage live or storage dead marker.
    pub fn is_storage_marker(&self) -> bool {
        match *self {
            PlaceContext::NonUse(NonUseContext::StorageLive) |
            PlaceContext::NonUse(NonUseContext::StorageDead) => true,
            _ => false,
        }
    }

    /// Returns `true` if this place context represents a storage live marker.
    pub fn is_storage_live_marker(&self) -> bool {
        match *self {
            PlaceContext::NonUse(NonUseContext::StorageLive) => true,
            _ => false,
        }
    }

    /// Returns `true` if this place context represents a storage dead marker.
    pub fn is_storage_dead_marker(&self) -> bool {
        match *self {
            PlaceContext::NonUse(NonUseContext::StorageDead) => true,
            _ => false,
        }
    }

    /// Returns `true` if this place context represents a use that potentially changes the value.
    pub fn is_mutating_use(&self) -> bool {
        match *self {
            PlaceContext::MutatingUse(..) => true,
            _ => false,
        }
    }

    /// Returns `true` if this place context represents a use that does not change the value.
    pub fn is_nonmutating_use(&self) -> bool {
        match *self {
            PlaceContext::NonMutatingUse(..) => true,
            _ => false,
        }
    }

    /// Returns `true` if this place context represents a use.
    pub fn is_use(&self) -> bool {
        match *self {
            PlaceContext::NonUse(..) => false,
            _ => true,
        }
    }

    /// Returns `true` if this place context represents an assignment statement.
    pub fn is_place_assignment(&self) -> bool {
        match *self {
            PlaceContext::MutatingUse(MutatingUseContext::Store) |
            PlaceContext::MutatingUse(MutatingUseContext::Call) |
            PlaceContext::MutatingUse(MutatingUseContext::AsmOutput) => true,
            _ => false,
        }
    }
}
