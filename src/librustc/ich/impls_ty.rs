// Copyright 2017 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! This module contains `HashStable` implementations for various data types
//! from rustc::ty in no particular order.

use ich::StableHashingContext;
use rustc_data_structures::stable_hasher::{HashStable, StableHasher,
                                           StableHasherResult};
use std::hash as std_hash;
use std::mem;
use ty;


impl<'a, 'tcx> HashStable<StableHashingContext<'a, 'tcx>> for ty::Ty<'tcx> {
    fn hash_stable<W: StableHasherResult>(&self,
                                          hcx: &mut StableHashingContext<'a, 'tcx>,
                                          hasher: &mut StableHasher<W>) {
        let type_hash = hcx.tcx().type_id_hash(*self);
        type_hash.hash_stable(hcx, hasher);
    }
}

impl_stable_hash_for!(struct ty::ItemSubsts<'tcx> { substs });

impl<'a, 'tcx, T> HashStable<StableHashingContext<'a, 'tcx>> for ty::Slice<T>
    where T: HashStable<StableHashingContext<'a, 'tcx>> {
    fn hash_stable<W: StableHasherResult>(&self,
                                          hcx: &mut StableHashingContext<'a, 'tcx>,
                                          hasher: &mut StableHasher<W>) {
        (&**self).hash_stable(hcx, hasher);
    }
}

impl<'a, 'tcx> HashStable<StableHashingContext<'a, 'tcx>> for ty::subst::Kind<'tcx> {
    fn hash_stable<W: StableHasherResult>(&self,
                                          hcx: &mut StableHashingContext<'a, 'tcx>,
                                          hasher: &mut StableHasher<W>) {
        self.as_type().hash_stable(hcx, hasher);
        self.as_region().hash_stable(hcx, hasher);
    }
}

impl<'a, 'tcx> HashStable<StableHashingContext<'a, 'tcx>> for ty::Region {
    fn hash_stable<W: StableHasherResult>(&self,
                                          hcx: &mut StableHashingContext<'a, 'tcx>,
                                          hasher: &mut StableHasher<W>) {
        mem::discriminant(self).hash_stable(hcx, hasher);
        match *self {
            ty::ReErased |
            ty::ReStatic |
            ty::ReEmpty => {
                // No variant fields to hash for these ...
            }
            ty::ReLateBound(db, ty::BrAnon(i)) => {
                db.depth.hash_stable(hcx, hasher);
                i.hash_stable(hcx, hasher);
            }
            ty::ReEarlyBound(ty::EarlyBoundRegion { index, name }) => {
                index.hash_stable(hcx, hasher);
                name.hash_stable(hcx, hasher);
            }
            ty::ReLateBound(..) |
            ty::ReFree(..) |
            ty::ReScope(..) |
            ty::ReVar(..) |
            ty::ReSkolemized(..) => {
                bug!("TypeIdHasher: unexpected region {:?}", *self)
            }
        }
    }
}

impl<'a, 'tcx> HashStable<StableHashingContext<'a, 'tcx>> for ty::adjustment::AutoBorrow<'tcx> {
    fn hash_stable<W: StableHasherResult>(&self,
                                          hcx: &mut StableHashingContext<'a, 'tcx>,
                                          hasher: &mut StableHasher<W>) {
        mem::discriminant(self).hash_stable(hcx, hasher);
        match *self {
            ty::adjustment::AutoBorrow::Ref(ref region, mutability) => {
                region.hash_stable(hcx, hasher);
                mutability.hash_stable(hcx, hasher);
            }
            ty::adjustment::AutoBorrow::RawPtr(mutability) => {
                mutability.hash_stable(hcx, hasher);
            }
        }
    }
}

impl<'a, 'tcx> HashStable<StableHashingContext<'a, 'tcx>> for ty::adjustment::Adjust<'tcx> {
    fn hash_stable<W: StableHasherResult>(&self,
                                          hcx: &mut StableHashingContext<'a, 'tcx>,
                                          hasher: &mut StableHasher<W>) {
        mem::discriminant(self).hash_stable(hcx, hasher);
        match *self {
            ty::adjustment::Adjust::NeverToAny |
            ty::adjustment::Adjust::ReifyFnPointer |
            ty::adjustment::Adjust::UnsafeFnPointer |
            ty::adjustment::Adjust::ClosureFnPointer |
            ty::adjustment::Adjust::MutToConstPointer => {}
            ty::adjustment::Adjust::DerefRef { autoderefs, ref autoref, unsize } => {
                autoderefs.hash_stable(hcx, hasher);
                autoref.hash_stable(hcx, hasher);
                unsize.hash_stable(hcx, hasher);
            }
        }
    }
}

impl_stable_hash_for!(struct ty::adjustment::Adjustment<'tcx> { kind, target });
impl_stable_hash_for!(struct ty::MethodCall { expr_id, autoderef });
impl_stable_hash_for!(struct ty::MethodCallee<'tcx> { def_id, ty, substs });
impl_stable_hash_for!(struct ty::UpvarId { var_id, closure_expr_id });
impl_stable_hash_for!(struct ty::UpvarBorrow<'tcx> { kind, region });

impl_stable_hash_for!(enum ty::BorrowKind {
    ImmBorrow,
    UniqueImmBorrow,
    MutBorrow
});


impl<'a, 'tcx> HashStable<StableHashingContext<'a, 'tcx>> for ty::UpvarCapture<'tcx> {
    fn hash_stable<W: StableHasherResult>(&self,
                                          hcx: &mut StableHashingContext<'a, 'tcx>,
                                          hasher: &mut StableHasher<W>) {
        mem::discriminant(self).hash_stable(hcx, hasher);
        match *self {
            ty::UpvarCapture::ByValue => {}
            ty::UpvarCapture::ByRef(ref up_var_borrow) => {
                up_var_borrow.hash_stable(hcx, hasher);
            }
        }
    }
}

impl_stable_hash_for!(struct ty::FnSig<'tcx> {
    inputs_and_output,
    variadic,
    unsafety,
    abi
});

impl<'a, 'tcx, T> HashStable<StableHashingContext<'a, 'tcx>> for ty::Binder<T>
    where T: HashStable<StableHashingContext<'a, 'tcx>> + ty::fold::TypeFoldable<'tcx>
{
    fn hash_stable<W: StableHasherResult>(&self,
                                          hcx: &mut StableHashingContext<'a, 'tcx>,
                                          hasher: &mut StableHasher<W>) {
        hcx.tcx().anonymize_late_bound_regions(self).0.hash_stable(hcx, hasher);
    }
}

impl_stable_hash_for!(enum ty::ClosureKind { Fn, FnMut, FnOnce });

impl_stable_hash_for!(enum ty::Visibility {
    Public,
    Restricted(def_id),
    Invisible
});

impl_stable_hash_for!(struct ty::TraitRef<'tcx> { def_id, substs });
impl_stable_hash_for!(struct ty::TraitPredicate<'tcx> { trait_ref });
impl_stable_hash_for!(tuple_struct ty::EquatePredicate<'tcx> { t1, t2 });
impl_stable_hash_for!(struct ty::SubtypePredicate<'tcx> { a_is_expected, a, b });

impl<'a, 'tcx, A, B> HashStable<StableHashingContext<'a, 'tcx>> for ty::OutlivesPredicate<A, B>
    where A: HashStable<StableHashingContext<'a, 'tcx>>,
          B: HashStable<StableHashingContext<'a, 'tcx>>,
{
    fn hash_stable<W: StableHasherResult>(&self,
                                          hcx: &mut StableHashingContext<'a, 'tcx>,
                                          hasher: &mut StableHasher<W>) {
        let ty::OutlivesPredicate(ref a, ref b) = *self;
        a.hash_stable(hcx, hasher);
        b.hash_stable(hcx, hasher);
    }
}

impl_stable_hash_for!(struct ty::ProjectionPredicate<'tcx> { projection_ty, ty });
impl_stable_hash_for!(struct ty::ProjectionTy<'tcx> { trait_ref, item_name });


impl<'a, 'tcx> HashStable<StableHashingContext<'a, 'tcx>> for ty::Predicate<'tcx> {
    fn hash_stable<W: StableHasherResult>(&self,
                                          hcx: &mut StableHashingContext<'a, 'tcx>,
                                          hasher: &mut StableHasher<W>) {
        mem::discriminant(self).hash_stable(hcx, hasher);
        match *self {
            ty::Predicate::Trait(ref pred) => {
                pred.hash_stable(hcx, hasher);
            }
            ty::Predicate::Equate(ref pred) => {
                pred.hash_stable(hcx, hasher);
            }
            ty::Predicate::Subtype(ref pred) => {
                pred.hash_stable(hcx, hasher);
            }
            ty::Predicate::RegionOutlives(ref pred) => {
                pred.hash_stable(hcx, hasher);
            }
            ty::Predicate::TypeOutlives(ref pred) => {
                pred.hash_stable(hcx, hasher);
            }
            ty::Predicate::Projection(ref pred) => {
                pred.hash_stable(hcx, hasher);
            }
            ty::Predicate::WellFormed(ty) => {
                ty.hash_stable(hcx, hasher);
            }
            ty::Predicate::ObjectSafe(def_id) => {
                def_id.hash_stable(hcx, hasher);
            }
            ty::Predicate::ClosureKind(def_id, closure_kind) => {
                def_id.hash_stable(hcx, hasher);
                closure_kind.hash_stable(hcx, hasher);
            }
        }
    }
}


impl<'a, 'tcx> HashStable<StableHashingContext<'a, 'tcx>> for ty::AdtFlags {
    fn hash_stable<W: StableHasherResult>(&self,
                                          _: &mut StableHashingContext<'a, 'tcx>,
                                          hasher: &mut StableHasher<W>) {
        std_hash::Hash::hash(self, hasher);
    }
}

impl_stable_hash_for!(struct ty::VariantDef {
    did,
    name,
    discr,
    fields,
    ctor_kind
});

impl_stable_hash_for!(enum ty::VariantDiscr {
    Explicit(def_id),
    Relative(distance)
});

impl_stable_hash_for!(struct ty::FieldDef {
    did,
    name,
    vis
});

impl<'a, 'tcx> HashStable<StableHashingContext<'a, 'tcx>>
for ::middle::const_val::ConstVal<'tcx> {
    fn hash_stable<W: StableHasherResult>(&self,
                                          hcx: &mut StableHashingContext<'a, 'tcx>,
                                          hasher: &mut StableHasher<W>) {
        use middle::const_val::ConstVal;

        mem::discriminant(self).hash_stable(hcx, hasher);

        match *self {
            ConstVal::Float(ref value) => {
                value.hash_stable(hcx, hasher);
            }
            ConstVal::Integral(ref value) => {
                value.hash_stable(hcx, hasher);
            }
            ConstVal::Str(ref value) => {
                value.hash_stable(hcx, hasher);
            }
            ConstVal::ByteStr(ref value) => {
                value.hash_stable(hcx, hasher);
            }
            ConstVal::Bool(value) => {
                value.hash_stable(hcx, hasher);
            }
            ConstVal::Function(def_id, substs) => {
                def_id.hash_stable(hcx, hasher);
                substs.hash_stable(hcx, hasher);
            }
            ConstVal::Struct(ref _name_value_map) => {
                // BTreeMap<ast::Name, ConstVal<'tcx>>),
                panic!("Ordering still unstable")
            }
            ConstVal::Tuple(ref value) => {
                value.hash_stable(hcx, hasher);
            }
            ConstVal::Array(ref value) => {
                value.hash_stable(hcx, hasher);
            }
            ConstVal::Repeat(ref value, times) => {
                value.hash_stable(hcx, hasher);
                times.hash_stable(hcx, hasher);
            }
            ConstVal::Char(value) => {
                value.hash_stable(hcx, hasher);
            }
        }
    }
}

impl_stable_hash_for!(struct ty::ClosureSubsts<'tcx> { substs });


impl_stable_hash_for!(struct ty::GenericPredicates<'tcx> {
    parent,
    predicates
});

impl_stable_hash_for!(enum ty::Variance {
    Covariant,
    Invariant,
    Contravariant,
    Bivariant
});

impl_stable_hash_for!(enum ty::adjustment::CustomCoerceUnsized {
    Struct(index)
});

impl<'a, 'tcx> HashStable<StableHashingContext<'a, 'tcx>> for ty::Generics {
    fn hash_stable<W: StableHasherResult>(&self,
                                          hcx: &mut StableHashingContext<'a, 'tcx>,
                                          hasher: &mut StableHasher<W>) {
        let ty::Generics {
            parent,
            parent_regions,
            parent_types,
            ref regions,
            ref types,

            // Reverse map to each `TypeParameterDef`'s `index` field, from
            // `def_id.index` (`def_id.krate` is the same as the item's).
            type_param_to_index: _, // Don't hash this
            has_self,
        } = *self;

        parent.hash_stable(hcx, hasher);
        parent_regions.hash_stable(hcx, hasher);
        parent_types.hash_stable(hcx, hasher);
        regions.hash_stable(hcx, hasher);
        types.hash_stable(hcx, hasher);
        has_self.hash_stable(hcx, hasher);
    }
}

impl<'a, 'tcx> HashStable<StableHashingContext<'a, 'tcx>> for ty::RegionParameterDef {
    fn hash_stable<W: StableHasherResult>(&self,
                                          hcx: &mut StableHashingContext<'a, 'tcx>,
                                          hasher: &mut StableHasher<W>) {
        let ty::RegionParameterDef {
            name,
            def_id,
            index,
            issue_32330: _,
            pure_wrt_drop
        } = *self;

        name.hash_stable(hcx, hasher);
        def_id.hash_stable(hcx, hasher);
        index.hash_stable(hcx, hasher);
        pure_wrt_drop.hash_stable(hcx, hasher);
    }
}

impl_stable_hash_for!(struct ty::TypeParameterDef {
    name,
    def_id,
    index,
    has_default,
    object_lifetime_default,
    pure_wrt_drop
});


impl<'a, 'tcx, T> HashStable<StableHashingContext<'a, 'tcx>>
for ::middle::resolve_lifetime::Set1<T>
    where T: HashStable<StableHashingContext<'a, 'tcx>>
{
    fn hash_stable<W: StableHasherResult>(&self,
                                          hcx: &mut StableHashingContext<'a, 'tcx>,
                                          hasher: &mut StableHasher<W>) {
        use middle::resolve_lifetime::Set1;

        mem::discriminant(self).hash_stable(hcx, hasher);
        match *self {
            Set1::Empty |
            Set1::Many => {
                // Nothing to do.
            }
            Set1::One(ref value) => {
                value.hash_stable(hcx, hasher);
            }
        }
    }
}

impl_stable_hash_for!(enum ::middle::resolve_lifetime::Region {
    Static,
    EarlyBound(index, decl),
    LateBound(db_index, decl),
    LateBoundAnon(db_index, anon_index),
    Free(call_site_scope_data, decl)
});

impl_stable_hash_for!(struct ::middle::region::CallSiteScopeData {
    fn_id,
    body_id
});

impl_stable_hash_for!(struct ty::DebruijnIndex {
    depth
});
