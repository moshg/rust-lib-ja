// Copyright 2012-2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//#![allow(non_camel_case_types)]

use self::ConstVal::*;
use self::ErrKind::*;
use self::EvalHint::*;

use front::map as ast_map;
use front::map::blocks::FnLikeNode;
use middle::cstore::{self, CrateStore, InlinedItem};
use middle::{infer, subst, traits};
use middle::def::Def;
use middle::subst::Subst;
use middle::def_id::DefId;
use middle::pat_util::def_to_path;
use middle::ty::{self, Ty, TyCtxt};
use middle::ty::util::IntTypeExt;
use middle::astconv_util::ast_ty_to_prim_ty;
use util::nodemap::NodeMap;

use graphviz::IntoCow;
use syntax::ast;
use rustc_front::hir::{Expr, PatKind};
use rustc_front::hir;
use rustc_front::intravisit::FnKind;
use syntax::codemap::Span;
use syntax::parse::token::InternedString;
use syntax::ptr::P;
use syntax::codemap;
use syntax::attr::IntType;

use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::hash_map::Entry::Vacant;
use std::hash;
use std::mem::transmute;
use std::rc::Rc;

use rustc_const_eval::*;

macro_rules! math {
    ($e:expr, $op:expr) => {
        match $op {
            Ok(val) => val,
            Err(e) => signal!($e, Math(e)),
        }
    }
}

fn lookup_variant_by_id<'a>(tcx: &'a ty::TyCtxt,
                            enum_def: DefId,
                            variant_def: DefId)
                            -> Option<&'a Expr> {
    fn variant_expr<'a>(variants: &'a [hir::Variant], id: ast::NodeId)
                        -> Option<&'a Expr> {
        for variant in variants {
            if variant.node.data.id() == id {
                return variant.node.disr_expr.as_ref().map(|e| &**e);
            }
        }
        None
    }

    if let Some(enum_node_id) = tcx.map.as_local_node_id(enum_def) {
        let variant_node_id = tcx.map.as_local_node_id(variant_def).unwrap();
        match tcx.map.find(enum_node_id) {
            None => None,
            Some(ast_map::NodeItem(it)) => match it.node {
                hir::ItemEnum(hir::EnumDef { ref variants }, _) => {
                    variant_expr(variants, variant_node_id)
                }
                _ => None
            },
            Some(_) => None
        }
    } else {
        None
    }
}

/// * `def_id` is the id of the constant.
/// * `maybe_ref_id` is the id of the expr referencing the constant.
/// * `param_substs` is the monomorphization substitution for the expression.
///
/// `maybe_ref_id` and `param_substs` are optional and are used for
/// finding substitutions in associated constants. This generally
/// happens in late/trans const evaluation.
pub fn lookup_const_by_id<'a, 'tcx: 'a>(tcx: &'a TyCtxt<'tcx>,
                                        def_id: DefId,
                                        maybe_ref_id: Option<ast::NodeId>,
                                        param_substs: Option<&'tcx subst::Substs<'tcx>>)
                                        -> Option<&'tcx Expr> {
    if let Some(node_id) = tcx.map.as_local_node_id(def_id) {
        match tcx.map.find(node_id) {
            None => None,
            Some(ast_map::NodeItem(it)) => match it.node {
                hir::ItemConst(_, ref const_expr) => {
                    Some(&const_expr)
                }
                _ => None
            },
            Some(ast_map::NodeTraitItem(ti)) => match ti.node {
                hir::ConstTraitItem(_, _) => {
                    match maybe_ref_id {
                        // If we have a trait item, and we know the expression
                        // that's the source of the obligation to resolve it,
                        // `resolve_trait_associated_const` will select an impl
                        // or the default.
                        Some(ref_id) => {
                            let trait_id = tcx.trait_of_item(def_id)
                                              .unwrap();
                            let mut substs = tcx.node_id_item_substs(ref_id)
                                                .substs;
                            if let Some(param_substs) = param_substs {
                                substs = substs.subst(tcx, param_substs);
                            }
                            resolve_trait_associated_const(tcx, ti, trait_id,
                                                           substs)
                        }
                        // Technically, without knowing anything about the
                        // expression that generates the obligation, we could
                        // still return the default if there is one. However,
                        // it's safer to return `None` than to return some value
                        // that may differ from what you would get from
                        // correctly selecting an impl.
                        None => None
                    }
                }
                _ => None
            },
            Some(ast_map::NodeImplItem(ii)) => match ii.node {
                hir::ImplItemKind::Const(_, ref expr) => {
                    Some(&expr)
                }
                _ => None
            },
            Some(_) => None
        }
    } else {
        match tcx.extern_const_statics.borrow().get(&def_id) {
            Some(&ast::DUMMY_NODE_ID) => return None,
            Some(&expr_id) => {
                return Some(tcx.map.expect_expr(expr_id));
            }
            None => {}
        }
        let mut used_ref_id = false;
        let expr_id = match tcx.sess.cstore.maybe_get_item_ast(tcx, def_id) {
            cstore::FoundAst::Found(&InlinedItem::Item(ref item)) => match item.node {
                hir::ItemConst(_, ref const_expr) => Some(const_expr.id),
                _ => None
            },
            cstore::FoundAst::Found(&InlinedItem::TraitItem(trait_id, ref ti)) => match ti.node {
                hir::ConstTraitItem(_, _) => {
                    used_ref_id = true;
                    match maybe_ref_id {
                        // As mentioned in the comments above for in-crate
                        // constants, we only try to find the expression for
                        // a trait-associated const if the caller gives us
                        // the expression that refers to it.
                        Some(ref_id) => {
                            let mut substs = tcx.node_id_item_substs(ref_id)
                                                .substs;
                            if let Some(param_substs) = param_substs {
                                substs = substs.subst(tcx, param_substs);
                            }
                            resolve_trait_associated_const(tcx, ti, trait_id,
                                                           substs).map(|e| e.id)
                        }
                        None => None
                    }
                }
                _ => None
            },
            cstore::FoundAst::Found(&InlinedItem::ImplItem(_, ref ii)) => match ii.node {
                hir::ImplItemKind::Const(_, ref expr) => Some(expr.id),
                _ => None
            },
            _ => None
        };
        // If we used the reference expression, particularly to choose an impl
        // of a trait-associated const, don't cache that, because the next
        // lookup with the same def_id may yield a different result.
        if !used_ref_id {
            tcx.extern_const_statics
               .borrow_mut().insert(def_id,
                                    expr_id.unwrap_or(ast::DUMMY_NODE_ID));
        }
        expr_id.map(|id| tcx.map.expect_expr(id))
    }
}

fn inline_const_fn_from_external_crate(tcx: &TyCtxt, def_id: DefId)
                                       -> Option<ast::NodeId> {
    match tcx.extern_const_fns.borrow().get(&def_id) {
        Some(&ast::DUMMY_NODE_ID) => return None,
        Some(&fn_id) => return Some(fn_id),
        None => {}
    }

    if !tcx.sess.cstore.is_const_fn(def_id) {
        tcx.extern_const_fns.borrow_mut().insert(def_id, ast::DUMMY_NODE_ID);
        return None;
    }

    let fn_id = match tcx.sess.cstore.maybe_get_item_ast(tcx, def_id) {
        cstore::FoundAst::Found(&InlinedItem::Item(ref item)) => Some(item.id),
        cstore::FoundAst::Found(&InlinedItem::ImplItem(_, ref item)) => Some(item.id),
        _ => None
    };
    tcx.extern_const_fns.borrow_mut().insert(def_id,
                                             fn_id.unwrap_or(ast::DUMMY_NODE_ID));
    fn_id
}

pub fn lookup_const_fn_by_id<'tcx>(tcx: &TyCtxt<'tcx>, def_id: DefId)
                                   -> Option<FnLikeNode<'tcx>>
{
    let fn_id = if let Some(node_id) = tcx.map.as_local_node_id(def_id) {
        node_id
    } else {
        if let Some(fn_id) = inline_const_fn_from_external_crate(tcx, def_id) {
            fn_id
        } else {
            return None;
        }
    };

    let fn_like = match FnLikeNode::from_node(tcx.map.get(fn_id)) {
        Some(fn_like) => fn_like,
        None => return None
    };

    match fn_like.kind() {
        FnKind::ItemFn(_, _, _, hir::Constness::Const, _, _) => {
            Some(fn_like)
        }
        FnKind::Method(_, m, _) => {
            if m.constness == hir::Constness::Const {
                Some(fn_like)
            } else {
                None
            }
        }
        _ => None
    }
}

#[derive(Clone, Debug, RustcEncodable, RustcDecodable)]
pub enum ConstVal {
    Float(f64),
    Integral(ConstInt),
    Str(InternedString),
    ByteStr(Rc<Vec<u8>>),
    Bool(bool),
    Struct(ast::NodeId),
    Tuple(ast::NodeId),
    Function(DefId),
    Array(ast::NodeId, u64),
    Repeat(ast::NodeId, u64),
    Char(char),
    /// A value that only occurs in case `eval_const_expr` reported an error. You should never
    /// handle this case. Its sole purpose is to allow more errors to be reported instead of
    /// causing a fatal error.
    Dummy,
}

impl hash::Hash for ConstVal {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        match *self {
            Float(a) => unsafe { transmute::<_,u64>(a) }.hash(state),
            Integral(a) => a.hash(state),
            Str(ref a) => a.hash(state),
            ByteStr(ref a) => a.hash(state),
            Bool(a) => a.hash(state),
            Struct(a) => a.hash(state),
            Tuple(a) => a.hash(state),
            Function(a) => a.hash(state),
            Array(a, n) => { a.hash(state); n.hash(state) },
            Repeat(a, n) => { a.hash(state); n.hash(state) },
            Char(c) => c.hash(state),
            Dummy => ().hash(state),
        }
    }
}

/// Note that equality for `ConstVal` means that the it is the same
/// constant, not that the rust values are equal. In particular, `NaN
/// == NaN` (at least if it's the same NaN; distinct encodings for NaN
/// are considering unequal).
impl PartialEq for ConstVal {
    fn eq(&self, other: &ConstVal) -> bool {
        match (self, other) {
            (&Float(a), &Float(b)) => unsafe{transmute::<_,u64>(a) == transmute::<_,u64>(b)},
            (&Integral(a), &Integral(b)) => a == b,
            (&Str(ref a), &Str(ref b)) => a == b,
            (&ByteStr(ref a), &ByteStr(ref b)) => a == b,
            (&Bool(a), &Bool(b)) => a == b,
            (&Struct(a), &Struct(b)) => a == b,
            (&Tuple(a), &Tuple(b)) => a == b,
            (&Function(a), &Function(b)) => a == b,
            (&Array(a, an), &Array(b, bn)) => (a == b) && (an == bn),
            (&Repeat(a, an), &Repeat(b, bn)) => (a == b) && (an == bn),
            (&Char(a), &Char(b)) => a == b,
            (&Dummy, &Dummy) => true, // FIXME: should this be false?
            _ => false,
        }
    }
}

impl Eq for ConstVal { }

impl ConstVal {
    pub fn description(&self) -> &'static str {
        match *self {
            Float(_) => "float",
            Integral(i) => i.description(),
            Str(_) => "string literal",
            ByteStr(_) => "byte string literal",
            Bool(_) => "boolean",
            Struct(_) => "struct",
            Tuple(_) => "tuple",
            Function(_) => "function definition",
            Array(..) => "array",
            Repeat(..) => "repeat",
            Char(..) => "char",
            Dummy => "dummy value",
        }
    }
}

pub fn const_expr_to_pat(tcx: &TyCtxt, expr: &Expr, span: Span) -> P<hir::Pat> {
    let pat = match expr.node {
        hir::ExprTup(ref exprs) =>
            PatKind::Tup(exprs.iter().map(|expr| const_expr_to_pat(tcx, &expr, span)).collect()),

        hir::ExprCall(ref callee, ref args) => {
            let def = *tcx.def_map.borrow().get(&callee.id).unwrap();
            if let Vacant(entry) = tcx.def_map.borrow_mut().entry(expr.id) {
               entry.insert(def);
            }
            let path = match def.full_def() {
                Def::Struct(def_id) => def_to_path(tcx, def_id),
                Def::Variant(_, variant_did) => def_to_path(tcx, variant_did),
                Def::Fn(..) => return P(hir::Pat {
                    id: expr.id,
                    node: PatKind::Lit(P(expr.clone())),
                    span: span,
                }),
                _ => unreachable!()
            };
            let pats = args.iter().map(|expr| const_expr_to_pat(tcx, &expr, span)).collect();
            PatKind::TupleStruct(path, Some(pats))
        }

        hir::ExprStruct(ref path, ref fields, None) => {
            let field_pats = fields.iter().map(|field| codemap::Spanned {
                span: codemap::DUMMY_SP,
                node: hir::FieldPat {
                    name: field.name.node,
                    pat: const_expr_to_pat(tcx, &field.expr, span),
                    is_shorthand: false,
                },
            }).collect();
            PatKind::Struct(path.clone(), field_pats, false)
        }

        hir::ExprVec(ref exprs) => {
            let pats = exprs.iter().map(|expr| const_expr_to_pat(tcx, &expr, span)).collect();
            PatKind::Vec(pats, None, hir::HirVec::new())
        }

        hir::ExprPath(_, ref path) => {
            let opt_def = tcx.def_map.borrow().get(&expr.id).map(|d| d.full_def());
            match opt_def {
                Some(Def::Struct(..)) | Some(Def::Variant(..)) =>
                    PatKind::Path(path.clone()),
                Some(Def::Const(def_id)) |
                Some(Def::AssociatedConst(def_id)) => {
                    let expr = lookup_const_by_id(tcx, def_id, Some(expr.id), None).unwrap();
                    return const_expr_to_pat(tcx, expr, span);
                },
                _ => unreachable!(),
            }
        }

        _ => PatKind::Lit(P(expr.clone()))
    };
    P(hir::Pat { id: expr.id, node: pat, span: span })
}

pub fn eval_const_expr(tcx: &TyCtxt, e: &Expr) -> ConstVal {
    match eval_const_expr_partial(tcx, e, ExprTypeChecked, None) {
        Ok(r) => r,
        // non-const path still needs to be a fatal error, because enums are funky
        Err(ref s) if s.kind == NonConstPath => tcx.sess.span_fatal(s.span, &s.description()),
        Err(s) => {
            tcx.sess.span_err(s.span, &s.description());
            Dummy
        },
    }
}

pub type FnArgMap<'a> = Option<&'a NodeMap<ConstVal>>;

#[derive(Clone)]
pub struct ConstEvalErr {
    pub span: Span,
    pub kind: ErrKind,
}

#[derive(Clone, PartialEq)]
pub enum ErrKind {
    CannotCast,
    CannotCastTo(&'static str),
    InvalidOpForInts(hir::BinOp_),
    InvalidOpForBools(hir::BinOp_),
    InvalidOpForFloats(hir::BinOp_),
    InvalidOpForIntUint(hir::BinOp_),
    InvalidOpForUintInt(hir::BinOp_),
    NegateOn(ConstVal),
    NotOn(ConstVal),
    CallOn(ConstVal),

    NegateWithOverflow(i64),
    AddiWithOverflow(i64, i64),
    SubiWithOverflow(i64, i64),
    MuliWithOverflow(i64, i64),
    AdduWithOverflow(u64, u64),
    SubuWithOverflow(u64, u64),
    MuluWithOverflow(u64, u64),
    DivideByZero,
    DivideWithOverflow,
    ModuloByZero,
    ModuloWithOverflow,
    ShiftLeftWithOverflow,
    ShiftRightWithOverflow,
    MissingStructField,
    NonConstPath,
    UnimplementedConstVal(&'static str),
    UnresolvedPath,
    ExpectedConstTuple,
    ExpectedConstStruct,
    TupleIndexOutOfBounds,
    IndexedNonVec,
    IndexNegative,
    IndexNotInt,
    IndexOutOfBounds,
    RepeatCountNotNatural,
    RepeatCountNotInt,

    MiscBinaryOp,
    MiscCatchAll,

    IndexOpFeatureGated,
    Math(ConstMathErr),

    IntermediateUnsignedNegative,
    /// Expected, Got
    TypeMismatch(String, ConstInt),
    BadType(ConstVal),
}

impl From<ConstMathErr> for ErrKind {
    fn from(err: ConstMathErr) -> ErrKind {
        Math(err)
    }
}

impl ConstEvalErr {
    pub fn description(&self) -> Cow<str> {
        use self::ErrKind::*;

        match self.kind {
            CannotCast => "can't cast this type".into_cow(),
            CannotCastTo(s) => format!("can't cast this type to {}", s).into_cow(),
            InvalidOpForInts(_) =>  "can't do this op on integrals".into_cow(),
            InvalidOpForBools(_) =>  "can't do this op on bools".into_cow(),
            InvalidOpForFloats(_) => "can't do this op on floats".into_cow(),
            InvalidOpForIntUint(..) => "can't do this op on an isize and usize".into_cow(),
            InvalidOpForUintInt(..) => "can't do this op on a usize and isize".into_cow(),
            NegateOn(ref const_val) => format!("negate on {}", const_val.description()).into_cow(),
            NotOn(ref const_val) => format!("not on {}", const_val.description()).into_cow(),
            CallOn(ref const_val) => format!("call on {}", const_val.description()).into_cow(),

            NegateWithOverflow(..) => "attempted to negate with overflow".into_cow(),
            AddiWithOverflow(..) => "attempted to add with overflow".into_cow(),
            SubiWithOverflow(..) => "attempted to sub with overflow".into_cow(),
            MuliWithOverflow(..) => "attempted to mul with overflow".into_cow(),
            AdduWithOverflow(..) => "attempted to add with overflow".into_cow(),
            SubuWithOverflow(..) => "attempted to sub with overflow".into_cow(),
            MuluWithOverflow(..) => "attempted to mul with overflow".into_cow(),
            DivideByZero         => "attempted to divide by zero".into_cow(),
            DivideWithOverflow   => "attempted to divide with overflow".into_cow(),
            ModuloByZero         => "attempted remainder with a divisor of zero".into_cow(),
            ModuloWithOverflow   => "attempted remainder with overflow".into_cow(),
            ShiftLeftWithOverflow => "attempted left shift with overflow".into_cow(),
            ShiftRightWithOverflow => "attempted right shift with overflow".into_cow(),
            MissingStructField  => "nonexistent struct field".into_cow(),
            NonConstPath        => "non-constant path in constant expression".into_cow(),
            UnimplementedConstVal(what) =>
                format!("unimplemented constant expression: {}", what).into_cow(),
            UnresolvedPath => "unresolved path in constant expression".into_cow(),
            ExpectedConstTuple => "expected constant tuple".into_cow(),
            ExpectedConstStruct => "expected constant struct".into_cow(),
            TupleIndexOutOfBounds => "tuple index out of bounds".into_cow(),
            IndexedNonVec => "indexing is only supported for arrays".into_cow(),
            IndexNegative => "indices must be non-negative integers".into_cow(),
            IndexNotInt => "indices must be integers".into_cow(),
            IndexOutOfBounds => "array index out of bounds".into_cow(),
            RepeatCountNotNatural => "repeat count must be a natural number".into_cow(),
            RepeatCountNotInt => "repeat count must be integers".into_cow(),

            MiscBinaryOp => "bad operands for binary".into_cow(),
            MiscCatchAll => "unsupported constant expr".into_cow(),
            IndexOpFeatureGated => "the index operation on const values is unstable".into_cow(),
            Math(ref err) => err.description().into_cow(),

            IntermediateUnsignedNegative => "during the computation of an unsigned a negative \
                                             number was encountered. This is most likely a bug in\
                                             the constant evaluator".into_cow(),

            TypeMismatch(ref expected, ref got) => {
                format!("mismatched types: expected `{}`, found `{}`",
                        expected, got.description()).into_cow()
            },
            BadType(ref i) => format!("value of wrong type: {:?}", i).into_cow(),
        }
    }
}

pub type EvalResult = Result<ConstVal, ConstEvalErr>;
pub type CastResult = Result<ConstVal, ErrKind>;

// FIXME: Long-term, this enum should go away: trying to evaluate
// an expression which hasn't been type-checked is a recipe for
// disaster.  That said, it's not clear how to fix ast_ty_to_ty
// to avoid the ordering issue.

/// Hint to determine how to evaluate constant expressions which
/// might not be type-checked.
#[derive(Copy, Clone, Debug)]
pub enum EvalHint<'tcx> {
    /// We have a type-checked expression.
    ExprTypeChecked,
    /// We have an expression which hasn't been type-checked, but we have
    /// an idea of what the type will be because of the context. For example,
    /// the length of an array is always `usize`. (This is referred to as
    /// a hint because it isn't guaranteed to be consistent with what
    /// type-checking would compute.)
    UncheckedExprHint(Ty<'tcx>),
    /// We have an expression which has not yet been type-checked, and
    /// and we have no clue what the type will be.
    UncheckedExprNoHint,
}

impl<'tcx> EvalHint<'tcx> {
    fn erase_hint(&self) -> EvalHint<'tcx> {
        match *self {
            ExprTypeChecked => ExprTypeChecked,
            UncheckedExprHint(_) | UncheckedExprNoHint => UncheckedExprNoHint,
        }
    }
    fn checked_or(&self, ty: Ty<'tcx>) -> EvalHint<'tcx> {
        match *self {
            ExprTypeChecked => ExprTypeChecked,
            _ => UncheckedExprHint(ty),
        }
    }
}

macro_rules! signal {
    ($e:expr, $exn:expr) => {
        return Err(ConstEvalErr { span: $e.span, kind: $exn })
    }
}

/// Evaluate a constant expression in a context where the expression isn't
/// guaranteed to be evaluatable. `ty_hint` is usually ExprTypeChecked,
/// but a few places need to evaluate constants during type-checking, like
/// computing the length of an array. (See also the FIXME above EvalHint.)
pub fn eval_const_expr_partial<'tcx>(tcx: &TyCtxt<'tcx>,
                                     e: &Expr,
                                     ty_hint: EvalHint<'tcx>,
                                     fn_args: FnArgMap) -> EvalResult {
    // Try to compute the type of the expression based on the EvalHint.
    // (See also the definition of EvalHint, and the FIXME above EvalHint.)
    let ety = match ty_hint {
        ExprTypeChecked => {
            // After type-checking, expr_ty is guaranteed to succeed.
            Some(tcx.expr_ty(e))
        }
        UncheckedExprHint(ty) => {
            // Use the type hint; it's not guaranteed to be right, but it's
            // usually good enough.
            Some(ty)
        }
        UncheckedExprNoHint => {
            // This expression might not be type-checked, and we have no hint.
            // Try to query the context for a type anyway; we might get lucky
            // (for example, if the expression was imported from another crate).
            tcx.expr_ty_opt(e)
        }
    };
    let result = match e.node {
      hir::ExprUnary(hir::UnNeg, ref inner) => {
        // unary neg literals already got their sign during creation
        if let hir::ExprLit(ref lit) = inner.node {
            use syntax::ast::*;
            use syntax::ast::LitIntType::*;
            const I8_OVERFLOW: u64 = ::std::i8::MAX as u64 + 1;
            const I16_OVERFLOW: u64 = ::std::i16::MAX as u64 + 1;
            const I32_OVERFLOW: u64 = ::std::i32::MAX as u64 + 1;
            const I64_OVERFLOW: u64 = ::std::i64::MAX as u64 + 1;
            match (&lit.node, ety.map(|t| &t.sty)) {
                (&LitKind::Int(I8_OVERFLOW, Unsuffixed), Some(&ty::TyInt(IntTy::I8))) |
                (&LitKind::Int(I8_OVERFLOW, Signed(IntTy::I8)), _) => {
                    return Ok(Integral(I8(::std::i8::MIN)))
                },
                (&LitKind::Int(I16_OVERFLOW, Unsuffixed), Some(&ty::TyInt(IntTy::I16))) |
                (&LitKind::Int(I16_OVERFLOW, Signed(IntTy::I16)), _) => {
                    return Ok(Integral(I16(::std::i16::MIN)))
                },
                (&LitKind::Int(I32_OVERFLOW, Unsuffixed), Some(&ty::TyInt(IntTy::I32))) |
                (&LitKind::Int(I32_OVERFLOW, Signed(IntTy::I32)), _) => {
                    return Ok(Integral(I32(::std::i32::MIN)))
                },
                (&LitKind::Int(I64_OVERFLOW, Unsuffixed), Some(&ty::TyInt(IntTy::I64))) |
                (&LitKind::Int(I64_OVERFLOW, Signed(IntTy::I64)), _) => {
                    return Ok(Integral(I64(::std::i64::MIN)))
                },
                (&LitKind::Int(n, Unsuffixed), Some(&ty::TyInt(IntTy::Is))) |
                (&LitKind::Int(n, Signed(IntTy::Is)), _) => {
                    match tcx.sess.target.int_type {
                        IntTy::I32 => if n == I32_OVERFLOW {
                            return Ok(Integral(Isize(Is32(::std::i32::MIN))));
                        },
                        IntTy::I64 => if n == I64_OVERFLOW {
                            return Ok(Integral(Isize(Is64(::std::i64::MIN))));
                        },
                        _ => unreachable!(),
                    }
                },
                _ => {},
            }
        }
        match try!(eval_const_expr_partial(tcx, &inner, ty_hint, fn_args)) {
          Float(f) => Float(-f),
          Integral(i) => Integral(math!(e, -i)),
          const_val => signal!(e, NegateOn(const_val)),
        }
      }
      hir::ExprUnary(hir::UnNot, ref inner) => {
        match try!(eval_const_expr_partial(tcx, &inner, ty_hint, fn_args)) {
          Integral(i) => Integral(math!(e, !i)),
          Bool(b) => Bool(!b),
          const_val => signal!(e, NotOn(const_val)),
        }
      }
      hir::ExprBinary(op, ref a, ref b) => {
        let b_ty = match op.node {
            hir::BiShl | hir::BiShr => ty_hint.checked_or(tcx.types.usize),
            _ => ty_hint
        };
        // technically, if we don't have type hints, but integral eval
        // gives us a type through a type-suffix, cast or const def type
        // we need to re-eval the other value of the BinOp if it was
        // not inferred
        match (try!(eval_const_expr_partial(tcx, &a, ty_hint, fn_args)),
               try!(eval_const_expr_partial(tcx, &b, b_ty, fn_args))) {
          (Float(a), Float(b)) => {
            match op.node {
              hir::BiAdd => Float(a + b),
              hir::BiSub => Float(a - b),
              hir::BiMul => Float(a * b),
              hir::BiDiv => Float(a / b),
              hir::BiRem => Float(a % b),
              hir::BiEq => Bool(a == b),
              hir::BiLt => Bool(a < b),
              hir::BiLe => Bool(a <= b),
              hir::BiNe => Bool(a != b),
              hir::BiGe => Bool(a >= b),
              hir::BiGt => Bool(a > b),
              _ => signal!(e, InvalidOpForFloats(op.node)),
            }
          }
          (Integral(a), Integral(b)) => {
            use std::cmp::Ordering::*;
            match op.node {
              hir::BiAdd => Integral(math!(e, a + b)),
              hir::BiSub => Integral(math!(e, a - b)),
              hir::BiMul => Integral(math!(e, a * b)),
              hir::BiDiv => Integral(math!(e, a / b)),
              hir::BiRem => Integral(math!(e, a % b)),
              hir::BiBitAnd => Integral(math!(e, a & b)),
              hir::BiBitOr => Integral(math!(e, a | b)),
              hir::BiBitXor => Integral(math!(e, a ^ b)),
              hir::BiShl => Integral(math!(e, a << b)),
              hir::BiShr => Integral(math!(e, a >> b)),
              hir::BiEq => Bool(math!(e, a.try_cmp(b)) == Equal),
              hir::BiLt => Bool(math!(e, a.try_cmp(b)) == Less),
              hir::BiLe => Bool(math!(e, a.try_cmp(b)) != Greater),
              hir::BiNe => Bool(math!(e, a.try_cmp(b)) != Equal),
              hir::BiGe => Bool(math!(e, a.try_cmp(b)) != Less),
              hir::BiGt => Bool(math!(e, a.try_cmp(b)) == Greater),
              _ => signal!(e, InvalidOpForInts(op.node)),
            }
          }
          (Bool(a), Bool(b)) => {
            Bool(match op.node {
              hir::BiAnd => a && b,
              hir::BiOr => a || b,
              hir::BiBitXor => a ^ b,
              hir::BiBitAnd => a & b,
              hir::BiBitOr => a | b,
              hir::BiEq => a == b,
              hir::BiNe => a != b,
              _ => signal!(e, InvalidOpForBools(op.node)),
             })
          }

          _ => signal!(e, MiscBinaryOp),
        }
      }
      hir::ExprCast(ref base, ref target_ty) => {
        let ety = ast_ty_to_prim_ty(tcx, &target_ty).or_else(|| ety)
                .unwrap_or_else(|| {
                    tcx.sess.span_fatal(target_ty.span,
                                        "target type not found for const cast")
                });

        let base_hint = if let ExprTypeChecked = ty_hint {
            ExprTypeChecked
        } else {
            match tcx.expr_ty_opt(&base) {
                Some(t) => UncheckedExprHint(t),
                None => ty_hint
            }
        };

        let val = match eval_const_expr_partial(tcx, &base, base_hint, fn_args) {
            Ok(val) => val,
            Err(ConstEvalErr { kind: TypeMismatch(_, val), .. }) => {
                // Something like `5i8 as usize` doesn't need a type hint for the base
                // instead take the type hint from the inner value
                let hint = match val.int_type() {
                    Some(IntType::UnsignedInt(ty)) => ty_hint.checked_or(tcx.mk_mach_uint(ty)),
                    Some(IntType::SignedInt(ty)) => ty_hint.checked_or(tcx.mk_mach_int(ty)),
                    // we had a type hint, so we can't have an unknown type
                    None => unreachable!(),
                };
                try!(eval_const_expr_partial(tcx, &base, hint, fn_args))
            },
            Err(e) => return Err(e),
        };
        match cast_const(tcx, val, ety) {
            Ok(val) => val,
            Err(kind) => return Err(ConstEvalErr { span: e.span, kind: kind }),
        }
      }
      hir::ExprPath(..) => {
          let opt_def = if let Some(def) = tcx.def_map.borrow().get(&e.id) {
              // After type-checking, def_map contains definition of the
              // item referred to by the path. During type-checking, it
              // can contain the raw output of path resolution, which
              // might be a partially resolved path.
              // FIXME: There's probably a better way to make sure we don't
              // panic here.
              if def.depth != 0 {
                  signal!(e, UnresolvedPath);
              }
              Some(def.full_def())
          } else {
              None
          };
          let (const_expr, const_ty) = match opt_def {
              Some(Def::Const(def_id)) => {
                  if let Some(node_id) = tcx.map.as_local_node_id(def_id) {
                      match tcx.map.find(node_id) {
                          Some(ast_map::NodeItem(it)) => match it.node {
                              hir::ItemConst(ref ty, ref expr) => {
                                  (Some(&**expr), Some(&**ty))
                              }
                              _ => (None, None)
                          },
                          _ => (None, None)
                      }
                  } else {
                      (lookup_const_by_id(tcx, def_id, Some(e.id), None), None)
                  }
              }
              Some(Def::AssociatedConst(def_id)) => {
                  if let Some(node_id) = tcx.map.as_local_node_id(def_id) {
                      match impl_or_trait_container(tcx, def_id) {
                          ty::TraitContainer(trait_id) => match tcx.map.find(node_id) {
                              Some(ast_map::NodeTraitItem(ti)) => match ti.node {
                                  hir::ConstTraitItem(ref ty, _) => {
                                      if let ExprTypeChecked = ty_hint {
                                          let substs = tcx.node_id_item_substs(e.id).substs;
                                          (resolve_trait_associated_const(tcx,
                                                                          ti,
                                                                          trait_id,
                                                                          substs),
                                           Some(&**ty))
                                       } else {
                                           (None, None)
                                       }
                                  }
                                  _ => (None, None)
                              },
                              _ => (None, None)
                          },
                          ty::ImplContainer(_) => match tcx.map.find(node_id) {
                              Some(ast_map::NodeImplItem(ii)) => match ii.node {
                                  hir::ImplItemKind::Const(ref ty, ref expr) => {
                                      (Some(&**expr), Some(&**ty))
                                  }
                                  _ => (None, None)
                              },
                              _ => (None, None)
                          },
                      }
                  } else {
                      (lookup_const_by_id(tcx, def_id, Some(e.id), None), None)
                  }
              }
              Some(Def::Variant(enum_def, variant_def)) => {
                  (lookup_variant_by_id(tcx, enum_def, variant_def), None)
              }
              Some(Def::Struct(..)) => {
                  return Ok(ConstVal::Struct(e.id))
              }
              Some(Def::Local(_, id)) => {
                  debug!("Def::Local({:?}): {:?}", id, fn_args);
                  if let Some(val) = fn_args.and_then(|args| args.get(&id)) {
                      return Ok(val.clone());
                  } else {
                      (None, None)
                  }
              },
              Some(Def::Method(id)) | Some(Def::Fn(id)) => return Ok(Function(id)),
              _ => (None, None)
          };
          let const_expr = match const_expr {
              Some(actual_e) => actual_e,
              None => signal!(e, NonConstPath)
          };
          let item_hint = match const_ty {
              Some(ty) => match ast_ty_to_prim_ty(tcx, ty) {
                  Some(ty) => ty_hint.checked_or(ty),
                  None => ty_hint.erase_hint(),
              },
              None => ty_hint.erase_hint(),
          };
          try!(eval_const_expr_partial(tcx, const_expr, item_hint, fn_args))
      }
      hir::ExprCall(ref callee, ref args) => {
          let sub_ty_hint = ty_hint.erase_hint();
          let callee_val = try!(eval_const_expr_partial(tcx, callee, sub_ty_hint, fn_args));
          let did = match callee_val {
              Function(did) => did,
              callee => signal!(e, CallOn(callee)),
          };
          let (decl, result) = if let Some(fn_like) = lookup_const_fn_by_id(tcx, did) {
              (fn_like.decl(), &fn_like.body().expr)
          } else {
              signal!(e, NonConstPath)
          };
          let result = result.as_ref().expect("const fn has no result expression");
          assert_eq!(decl.inputs.len(), args.len());

          let mut call_args = NodeMap();
          for (arg, arg_expr) in decl.inputs.iter().zip(args.iter()) {
              let arg_hint = ty_hint.erase_hint();
              let arg_val = try!(eval_const_expr_partial(
                  tcx,
                  arg_expr,
                  arg_hint,
                  fn_args
              ));
              debug!("const call arg: {:?}", arg);
              let old = call_args.insert(arg.pat.id, arg_val);
              assert!(old.is_none());
          }
          debug!("const call({:?})", call_args);
          try!(eval_const_expr_partial(tcx, &result, ty_hint, Some(&call_args)))
      },
      hir::ExprLit(ref lit) => match lit_to_const(&lit.node, tcx, ety, lit.span) {
          Ok(val) => val,
          Err(err) => signal!(e, Math(err)),
      },
      hir::ExprBlock(ref block) => {
        match block.expr {
            Some(ref expr) => try!(eval_const_expr_partial(tcx, &expr, ty_hint, fn_args)),
            None => unreachable!(),
        }
      }
      hir::ExprType(ref e, _) => try!(eval_const_expr_partial(tcx, &e, ty_hint, fn_args)),
      hir::ExprTup(_) => Tuple(e.id),
      hir::ExprStruct(..) => Struct(e.id),
      hir::ExprIndex(ref arr, ref idx) => {
        if !tcx.sess.features.borrow().const_indexing {
            signal!(e, IndexOpFeatureGated);
        }
        let arr_hint = ty_hint.erase_hint();
        let arr = try!(eval_const_expr_partial(tcx, arr, arr_hint, fn_args));
        let idx_hint = ty_hint.checked_or(tcx.types.usize);
        let idx = match try!(eval_const_expr_partial(tcx, idx, idx_hint, fn_args)) {
            Integral(Usize(i)) => i.as_u64(tcx.sess.target.uint_type),
            Integral(_) => unreachable!(),
            _ => signal!(idx, IndexNotInt),
        };
        assert_eq!(idx as usize as u64, idx);
        match arr {
            Array(_, n) if idx >= n => signal!(e, IndexOutOfBounds),
            Array(v, n) => if let hir::ExprVec(ref v) = tcx.map.expect_expr(v).node {
                assert_eq!(n as usize as u64, n);
                try!(eval_const_expr_partial(tcx, &v[idx as usize], ty_hint, fn_args))
            } else {
                unreachable!()
            },

            Repeat(_, n) if idx >= n => signal!(e, IndexOutOfBounds),
            Repeat(elem, _) => try!(eval_const_expr_partial(
                tcx,
                &tcx.map.expect_expr(elem),
                ty_hint,
                fn_args,
            )),

            ByteStr(ref data) if idx >= data.len() as u64 => signal!(e, IndexOutOfBounds),
            ByteStr(data) => {
                Integral(U8(data[idx as usize]))
            },

            Str(ref s) if idx as usize >= s.len() => signal!(e, IndexOutOfBounds),
            Str(_) => unimplemented!(), // FIXME: return a const char
            _ => signal!(e, IndexedNonVec),
        }
      }
      hir::ExprVec(ref v) => Array(e.id, v.len() as u64),
      hir::ExprRepeat(_, ref n) => {
          let len_hint = ty_hint.checked_or(tcx.types.usize);
          Repeat(
              e.id,
              match try!(eval_const_expr_partial(tcx, &n, len_hint, fn_args)) {
                  Integral(Usize(i)) => i.as_u64(tcx.sess.target.uint_type),
                  Integral(_) => signal!(e, RepeatCountNotNatural),
                  _ => signal!(e, RepeatCountNotInt),
              },
          )
      },
      hir::ExprTupField(ref base, index) => {
        let base_hint = ty_hint.erase_hint();
        let c = try!(eval_const_expr_partial(tcx, base, base_hint, fn_args));
        if let Tuple(tup_id) = c {
            if let hir::ExprTup(ref fields) = tcx.map.expect_expr(tup_id).node {
                if index.node < fields.len() {
                    return eval_const_expr_partial(tcx, &fields[index.node], ty_hint, fn_args)
                } else {
                    signal!(e, TupleIndexOutOfBounds);
                }
            } else {
                unreachable!()
            }
        } else {
            signal!(base, ExpectedConstTuple);
        }
      }
      hir::ExprField(ref base, field_name) => {
        let base_hint = ty_hint.erase_hint();
        // Get the base expression if it is a struct and it is constant
        let c = try!(eval_const_expr_partial(tcx, base, base_hint, fn_args));
        if let Struct(struct_id) = c {
            if let hir::ExprStruct(_, ref fields, _) = tcx.map.expect_expr(struct_id).node {
                // Check that the given field exists and evaluate it
                // if the idents are compared run-pass/issue-19244 fails
                if let Some(f) = fields.iter().find(|f| f.name.node
                                                     == field_name.node) {
                    return eval_const_expr_partial(tcx, &f.expr, ty_hint, fn_args)
                } else {
                    signal!(e, MissingStructField);
                }
            } else {
                unreachable!()
            }
        } else {
            signal!(base, ExpectedConstStruct);
        }
      }
      _ => signal!(e, MiscCatchAll)
    };

    match (ety.map(|t| &t.sty), result) {
        (Some(ref ty_hint), Integral(i)) => Ok(Integral(try!(infer(i, tcx, ty_hint, e.span)))),
        (_, result) => Ok(result),
    }
}

fn infer<'tcx>(
    i: ConstInt,
    tcx: &TyCtxt<'tcx>,
    ty_hint: &ty::TypeVariants<'tcx>,
    span: Span
) -> Result<ConstInt, ConstEvalErr> {
    use syntax::ast::*;
    const I8MAX: u64 = ::std::i8::MAX as u64;
    const I16MAX: u64 = ::std::i16::MAX as u64;
    const I32MAX: u64 = ::std::i32::MAX as u64;
    const I64MAX: u64 = ::std::i64::MAX as u64;

    const U8MAX: u64 = ::std::u8::MAX as u64;
    const U16MAX: u64 = ::std::u16::MAX as u64;
    const U32MAX: u64 = ::std::u32::MAX as u64;

    const I8MAXI: i64 = ::std::i8::MAX as i64;
    const I16MAXI: i64 = ::std::i16::MAX as i64;
    const I32MAXI: i64 = ::std::i32::MAX as i64;

    const I8MINI: i64 = ::std::i8::MIN as i64;
    const I16MINI: i64 = ::std::i16::MIN as i64;
    const I32MINI: i64 = ::std::i32::MIN as i64;

    let err = |e| ConstEvalErr {
        span: span,
        kind: e,
    };

    match (ty_hint, i) {
        (&ty::TyInt(IntTy::I8), result @ I8(_)) => Ok(result),
        (&ty::TyInt(IntTy::I16), result @ I16(_)) => Ok(result),
        (&ty::TyInt(IntTy::I32), result @ I32(_)) => Ok(result),
        (&ty::TyInt(IntTy::I64), result @ I64(_)) => Ok(result),
        (&ty::TyInt(IntTy::Is), result @ Isize(_)) => Ok(result),

        (&ty::TyUint(UintTy::U8), result @ U8(_)) => Ok(result),
        (&ty::TyUint(UintTy::U16), result @ U16(_)) => Ok(result),
        (&ty::TyUint(UintTy::U32), result @ U32(_)) => Ok(result),
        (&ty::TyUint(UintTy::U64), result @ U64(_)) => Ok(result),
        (&ty::TyUint(UintTy::Us), result @ Usize(_)) => Ok(result),

        (&ty::TyInt(IntTy::I8), Infer(i @ 0...I8MAX)) => Ok(I8(i as i8)),
        (&ty::TyInt(IntTy::I16), Infer(i @ 0...I16MAX)) => Ok(I16(i as i16)),
        (&ty::TyInt(IntTy::I32), Infer(i @ 0...I32MAX)) => Ok(I32(i as i32)),
        (&ty::TyInt(IntTy::I64), Infer(i @ 0...I64MAX)) => Ok(I64(i as i64)),
        (&ty::TyInt(IntTy::Is), Infer(i @ 0...I64MAX)) => {
            match ConstIsize::new(i as i64, tcx.sess.target.int_type) {
                Ok(val) => Ok(Isize(val)),
                Err(e) => Err(err(e.into())),
            }
        },
        (&ty::TyInt(_), Infer(_)) => Err(err(Math(ConstMathErr::NotInRange))),

        (&ty::TyInt(IntTy::I8), InferSigned(i @ I8MINI...I8MAXI)) => Ok(I8(i as i8)),
        (&ty::TyInt(IntTy::I16), InferSigned(i @ I16MINI...I16MAXI)) => Ok(I16(i as i16)),
        (&ty::TyInt(IntTy::I32), InferSigned(i @ I32MINI...I32MAXI)) => Ok(I32(i as i32)),
        (&ty::TyInt(IntTy::I64), InferSigned(i)) => Ok(I64(i)),
        (&ty::TyInt(IntTy::Is), InferSigned(i)) => {
            match ConstIsize::new(i, tcx.sess.target.int_type) {
                Ok(val) => Ok(Isize(val)),
                Err(e) => Err(err(e.into())),
            }
        },
        (&ty::TyInt(_), InferSigned(_)) => Err(err(Math(ConstMathErr::NotInRange))),

        (&ty::TyUint(UintTy::U8), Infer(i @ 0...U8MAX)) => Ok(U8(i as u8)),
        (&ty::TyUint(UintTy::U16), Infer(i @ 0...U16MAX)) => Ok(U16(i as u16)),
        (&ty::TyUint(UintTy::U32), Infer(i @ 0...U32MAX)) => Ok(U32(i as u32)),
        (&ty::TyUint(UintTy::U64), Infer(i)) => Ok(U64(i)),
        (&ty::TyUint(UintTy::Us), Infer(i)) => {
            match ConstUsize::new(i, tcx.sess.target.uint_type) {
                Ok(val) => Ok(Usize(val)),
                Err(e) => Err(err(e.into())),
            }
        },
        (&ty::TyUint(_), Infer(_)) => Err(err(Math(ConstMathErr::NotInRange))),
        (&ty::TyUint(_), InferSigned(_)) => Err(err(IntermediateUnsignedNegative)),

        (&ty::TyInt(ity), i) => Err(err(TypeMismatch(ity.to_string(), i))),
        (&ty::TyUint(ity), i) => Err(err(TypeMismatch(ity.to_string(), i))),

        (&ty::TyEnum(ref adt, _), i) => {
            let hints = tcx.lookup_repr_hints(adt.did);
            let int_ty = tcx.enum_repr_type(hints.iter().next());
            infer(i, tcx, &int_ty.to_ty(tcx).sty, span)
        },
        (_, i) => Err(err(BadType(ConstVal::Integral(i)))),
    }
}

fn impl_or_trait_container(tcx: &TyCtxt, def_id: DefId) -> ty::ImplOrTraitItemContainer {
    // This is intended to be equivalent to tcx.impl_or_trait_item(def_id).container()
    // for local def_id, but it can be called before tcx.impl_or_trait_items is complete.
    if let Some(node_id) = tcx.map.as_local_node_id(def_id) {
        if let Some(ast_map::NodeItem(item)) = tcx.map.find(tcx.map.get_parent_node(node_id)) {
            let container_id = tcx.map.local_def_id(item.id);
            match item.node {
                hir::ItemImpl(..) => return ty::ImplContainer(container_id),
                hir::ItemTrait(..) => return ty::TraitContainer(container_id),
                _ => ()
            }
        }
        panic!("No impl or trait container for {:?}", def_id);
    }
    panic!("{:?} is not local", def_id);
}

fn resolve_trait_associated_const<'a, 'tcx: 'a>(tcx: &'a TyCtxt<'tcx>,
                                                ti: &'tcx hir::TraitItem,
                                                trait_id: DefId,
                                                rcvr_substs: subst::Substs<'tcx>)
                                                -> Option<&'tcx Expr>
{
    let trait_ref = ty::Binder(
        rcvr_substs.erase_regions().to_trait_ref(tcx, trait_id)
    );
    debug!("resolve_trait_associated_const: trait_ref={:?}",
           trait_ref);

    tcx.populate_implementations_for_trait_if_necessary(trait_ref.def_id());
    let infcx = infer::new_infer_ctxt(tcx, &tcx.tables, None);

    let mut selcx = traits::SelectionContext::new(&infcx);
    let obligation = traits::Obligation::new(traits::ObligationCause::dummy(),
                                             trait_ref.to_poly_trait_predicate());
    let selection = match selcx.select(&obligation) {
        Ok(Some(vtable)) => vtable,
        // Still ambiguous, so give up and let the caller decide whether this
        // expression is really needed yet. Some associated constant values
        // can't be evaluated until monomorphization is done in trans.
        Ok(None) => {
            return None
        }
        Err(_) => {
            return None
        }
    };

    match selection {
        traits::VtableImpl(ref impl_data) => {
            match tcx.associated_consts(impl_data.impl_def_id)
                     .iter().find(|ic| ic.name == ti.name) {
                Some(ic) => lookup_const_by_id(tcx, ic.def_id, None, None),
                None => match ti.node {
                    hir::ConstTraitItem(_, Some(ref expr)) => Some(&*expr),
                    _ => None,
                },
            }
        }
        _ => {
            tcx.sess.span_bug(
                ti.span,
                "resolve_trait_associated_const: unexpected vtable type")
        }
    }
}

fn cast_const_int<'tcx>(tcx: &TyCtxt<'tcx>, val: ConstInt, ty: ty::Ty) -> CastResult {
    let v = val.to_u64_unchecked();
    match ty.sty {
        ty::TyBool if v == 0 => Ok(Bool(false)),
        ty::TyBool if v == 1 => Ok(Bool(true)),
        ty::TyInt(ast::IntTy::I8) => Ok(Integral(I8(v as i8))),
        ty::TyInt(ast::IntTy::I16) => Ok(Integral(I16(v as i16))),
        ty::TyInt(ast::IntTy::I32) => Ok(Integral(I32(v as i32))),
        ty::TyInt(ast::IntTy::I64) => Ok(Integral(I64(v as i64))),
        ty::TyInt(ast::IntTy::Is) => {
            Ok(Integral(Isize(try!(ConstIsize::new(v as i64, tcx.sess.target.int_type)))))
        },
        ty::TyUint(ast::UintTy::U8) => Ok(Integral(U8(v as u8))),
        ty::TyUint(ast::UintTy::U16) => Ok(Integral(U16(v as u16))),
        ty::TyUint(ast::UintTy::U32) => Ok(Integral(U32(v as u32))),
        ty::TyUint(ast::UintTy::U64) => Ok(Integral(U64(v as u64))),
        ty::TyUint(ast::UintTy::Us) => {
            Ok(Integral(Usize(try!(ConstUsize::new(v, tcx.sess.target.uint_type)))))
        },
        ty::TyFloat(ast::FloatTy::F64) if val.is_negative() => {
            // FIXME: this could probably be prettier
            // there's no easy way to turn an `Infer` into a f64
            let val = try!((-val).map_err(Math));
            let val = val.to_u64().unwrap() as f64;
            let val = -val;
            Ok(Float(val))
        },
        ty::TyFloat(ast::FloatTy::F64) => Ok(Float(val.to_u64().unwrap() as f64)),
        ty::TyFloat(ast::FloatTy::F32) if val.is_negative() => {
            let val = try!((-val).map_err(Math));
            let val = val.to_u64().unwrap() as f32;
            let val = -val;
            Ok(Float(val as f64))
        },
        ty::TyFloat(ast::FloatTy::F32) => Ok(Float(val.to_u64().unwrap() as f32 as f64)),
        _ => Err(CannotCast),
    }
}

fn cast_const_float<'tcx>(tcx: &TyCtxt<'tcx>, f: f64, ty: ty::Ty) -> CastResult {
    match ty.sty {
        ty::TyInt(_) if f >= 0.0 => cast_const_int(tcx, Infer(f as u64), ty),
        ty::TyInt(_) => cast_const_int(tcx, InferSigned(f as i64), ty),
        ty::TyUint(_) if f >= 0.0 => cast_const_int(tcx, Infer(f as u64), ty),
        ty::TyFloat(ast::FloatTy::F64) => Ok(Float(f)),
        ty::TyFloat(ast::FloatTy::F32) => Ok(Float(f as f32 as f64)),
        _ => Err(CannotCast),
    }
}

fn cast_const<'tcx>(tcx: &TyCtxt<'tcx>, val: ConstVal, ty: ty::Ty) -> CastResult {
    match val {
        Integral(i) => cast_const_int(tcx, i, ty),
        Bool(b) => cast_const_int(tcx, Infer(b as u64), ty),
        Float(f) => cast_const_float(tcx, f, ty),
        Char(c) => cast_const_int(tcx, Infer(c as u64), ty),
        _ => Err(CannotCast),
    }
}

fn lit_to_const<'tcx>(lit: &ast::LitKind,
                      tcx: &TyCtxt<'tcx>,
                      ty_hint: Option<Ty<'tcx>>,
                      span: Span,
                      ) -> Result<ConstVal, ConstMathErr> {
    use syntax::ast::*;
    use syntax::ast::LitIntType::*;
    const I8MAX: u64 = ::std::i8::MAX as u64;
    const I16MAX: u64 = ::std::i16::MAX as u64;
    const I32MAX: u64 = ::std::i32::MAX as u64;
    const I64MAX: u64 = ::std::i64::MAX as u64;
    const U8MAX: u64 = ::std::u8::MAX as u64;
    const U16MAX: u64 = ::std::u16::MAX as u64;
    const U32MAX: u64 = ::std::u32::MAX as u64;
    const U64MAX: u64 = ::std::u64::MAX as u64;
    match *lit {
        LitKind::Str(ref s, _) => Ok(Str((*s).clone())),
        LitKind::ByteStr(ref data) => Ok(ByteStr(data.clone())),
        LitKind::Byte(n) => Ok(Integral(U8(n))),
        LitKind::Int(n @ 0...I8MAX, Signed(IntTy::I8)) => Ok(Integral(I8(n as i8))),
        LitKind::Int(n @ 0...I16MAX, Signed(IntTy::I16)) => Ok(Integral(I16(n as i16))),
        LitKind::Int(n @ 0...I32MAX, Signed(IntTy::I32)) => Ok(Integral(I32(n as i32))),
        LitKind::Int(n @ 0...I64MAX, Signed(IntTy::I64)) => Ok(Integral(I64(n as i64))),
        LitKind::Int(n, Signed(IntTy::Is)) => {
            Ok(Integral(Isize(try!(ConstIsize::new(n as i64, tcx.sess.target.int_type)))))
        },

        LitKind::Int(_, Signed(ty)) => Err(ConstMathErr::LitOutOfRange(ty)),

        LitKind::Int(n, Unsuffixed) => {
            match ty_hint.map(|t| &t.sty) {
                Some(&ty::TyInt(ity)) => {
                    lit_to_const(&LitKind::Int(n, Signed(ity)), tcx, ty_hint, span)
                },
                Some(&ty::TyUint(uty)) => {
                    lit_to_const(&LitKind::Int(n, Unsigned(uty)), tcx, ty_hint, span)
                },
                None => Ok(Integral(Infer(n))),
                Some(&ty::TyEnum(ref adt, _)) => {
                    let hints = tcx.lookup_repr_hints(adt.did);
                    let int_ty = tcx.enum_repr_type(hints.iter().next());
                    lit_to_const(lit, tcx, Some(int_ty.to_ty(tcx)), span)
                },
                Some(ty_hint) => panic!("bad ty_hint: {:?}, {:?}", ty_hint, lit),
            }
        },
        LitKind::Int(n @ 0...U8MAX, Unsigned(UintTy::U8)) => Ok(Integral(U8(n as u8))),
        LitKind::Int(n @ 0...U16MAX, Unsigned(UintTy::U16)) => Ok(Integral(U16(n as u16))),
        LitKind::Int(n @ 0...U32MAX, Unsigned(UintTy::U32)) => Ok(Integral(U32(n as u32))),
        LitKind::Int(n @ 0...U64MAX, Unsigned(UintTy::U64)) => Ok(Integral(U64(n as u64))),

        LitKind::Int(n, Unsigned(UintTy::Us)) => {
            Ok(Integral(Usize(try!(ConstUsize::new(n as u64, tcx.sess.target.uint_type)))))
        },
        LitKind::Int(_, Unsigned(ty)) => Err(ConstMathErr::ULitOutOfRange(ty)),

        LitKind::Float(ref n, _) |
        LitKind::FloatUnsuffixed(ref n) => {
            if let Ok(x) = n.parse::<f64>() {
                Ok(Float(x))
            } else {
                // FIXME(#31407) this is only necessary because float parsing is buggy
                tcx.sess.span_bug(span, "could not evaluate float literal (see issue #31407)");
            }
        }
        LitKind::Bool(b) => Ok(Bool(b)),
        LitKind::Char(c) => Ok(Char(c)),
    }
}

pub fn compare_const_vals(a: &ConstVal, b: &ConstVal) -> Option<Ordering> {
    match (a, b) {
        (&Integral(a), &Integral(b)) => a.try_cmp(b).ok(),
        (&Float(a), &Float(b)) => {
            // This is pretty bad but it is the existing behavior.
            Some(if a == b {
                Ordering::Equal
            } else if a < b {
                Ordering::Less
            } else {
                Ordering::Greater
            })
        }
        (&Str(ref a), &Str(ref b)) => Some(a.cmp(b)),
        (&Bool(a), &Bool(b)) => Some(a.cmp(&b)),
        (&ByteStr(ref a), &ByteStr(ref b)) => Some(a.cmp(b)),
        (&Char(a), &Char(ref b)) => Some(a.cmp(b)),
        _ => None,
    }
}

pub fn compare_lit_exprs<'tcx>(tcx: &TyCtxt<'tcx>,
                               a: &Expr,
                               b: &Expr) -> Option<Ordering> {
    let a = match eval_const_expr_partial(tcx, a, ExprTypeChecked, None) {
        Ok(a) => a,
        Err(e) => {
            tcx.sess.span_err(a.span, &e.description());
            return None;
        }
    };
    let b = match eval_const_expr_partial(tcx, b, ExprTypeChecked, None) {
        Ok(b) => b,
        Err(e) => {
            tcx.sess.span_err(b.span, &e.description());
            return None;
        }
    };
    compare_const_vals(&a, &b)
}
