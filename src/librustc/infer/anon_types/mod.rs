// Copyright 2012-2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use hir::def_id::DefId;
use infer::{self, InferCtxt, InferOk, TypeVariableOrigin};
use infer::outlives::free_region_map::FreeRegionRelations;
use syntax::ast;
use traits::{self, PredicateObligation};
use ty::{self, Ty};
use ty::fold::{BottomUpFolder, TypeFoldable};
use ty::outlives::Component;
use ty::subst::Substs;
use util::nodemap::DefIdMap;

pub type AnonTypeMap<'tcx> = DefIdMap<AnonTypeDecl<'tcx>>;

/// Information about the anonymous, abstract types whose values we
/// are inferring in this function (these are the `impl Trait` that
/// appear in the return type).
#[derive(Copy, Clone, Debug)]
pub struct AnonTypeDecl<'tcx> {
    /// The substitutions that we apply to the abstract that that this
    /// `impl Trait` desugars to. e.g., if:
    ///
    ///     fn foo<'a, 'b, T>() -> impl Trait<'a>
    ///
    /// winds up desugared to:
    ///
    ///     abstract type Foo<'x, T>: Trait<'x>
    ///     fn foo<'a, 'b, T>() -> Foo<'a, T>
    ///
    /// then `substs` would be `['a, T]`.
    pub substs: &'tcx Substs<'tcx>,

    /// The type variable that represents the value of the abstract type
    /// that we require. In other words, after we compile this function,
    /// we will be created a constraint like:
    ///
    ///     Foo<'a, T> = ?C
    ///
    /// where `?C` is the value of this type variable. =) It may
    /// naturally refer to the type and lifetime parameters in scope
    /// in this function, though ultimately it should only reference
    /// those that are arguments to `Foo` in the constraint above. (In
    /// other words, `?C` should not include `'b`, even though it's a
    /// lifetime parameter on `foo`.)
    pub concrete_ty: Ty<'tcx>,

    /// True if the `impl Trait` bounds include region bounds.
    /// For example, this would be true for:
    ///
    ///     fn foo<'a, 'b, 'c>() -> impl Trait<'c> + 'a + 'b
    ///
    /// but false for:
    ///
    ///     fn foo<'c>() -> impl Trait<'c>
    ///
    /// unless `Trait` was declared like:
    ///
    ///     trait Trait<'c>: 'c
    ///
    /// in which case it would be true.
    ///
    /// This is used during regionck to decide whether we need to
    /// impose any additional constraints to ensure that region
    /// variables in `concrete_ty` wind up being constrained to
    /// something from `substs` (or, at minimum, things that outlive
    /// the fn body). (Ultimately, writeback is responsible for this
    /// check.)
    pub has_required_region_bounds: bool,
}

impl<'a, 'gcx, 'tcx> InferCtxt<'a, 'gcx, 'tcx> {
    /// Replace all anonymized types in `value` with fresh inference variables
    /// and creates appropriate obligations. For example, given the input:
    ///
    ///     impl Iterator<Item = impl Debug>
    ///
    /// this method would create two type variables, `?0` and `?1`. It would
    /// return the type `?0` but also the obligations:
    ///
    ///     ?0: Iterator<Item = ?1>
    ///     ?1: Debug
    ///
    /// Moreover, it returns a `AnonTypeMap` that would map `?0` to
    /// info about the `impl Iterator<..>` type and `?1` to info about
    /// the `impl Debug` type.
    pub fn instantiate_anon_types<T: TypeFoldable<'tcx>>(
        &self,
        body_id: ast::NodeId,
        param_env: ty::ParamEnv<'tcx>,
        value: &T,
    ) -> InferOk<'tcx, (T, AnonTypeMap<'tcx>)> {
        debug!(
            "instantiate_anon_types(value={:?}, body_id={:?}, param_env={:?})",
            value,
            body_id,
            param_env,
        );
        let mut instantiator = Instantiator {
            infcx: self,
            body_id,
            param_env,
            anon_types: DefIdMap(),
            obligations: vec![],
        };
        let value = instantiator.instantiate_anon_types_in_map(value);
        InferOk {
            value: (value, instantiator.anon_types),
            obligations: instantiator.obligations,
        }
    }

    /// Given the map `anon_types` containing the existential `impl
    /// Trait` types whose underlying, hidden types are being
    /// inferred, this method adds constraints to the regions
    /// appearing in those underlying hidden types to ensure that they
    /// at least do not refer to random scopes within the current
    /// function. These constraints are not (quite) sufficient to
    /// guarantee that the regions are actually legal values; that
    /// final condition is imposed after region inference is done.
    ///
    /// # The Problem
    ///
    /// Let's work through an example to explain how it works.  Assume
    /// the current function is as follows:
    ///
    ///     fn foo<'a, 'b>(..) -> (impl Bar<'a>, impl Bar<'b>)
    ///
    /// Here, we have two `impl Trait` types whose values are being
    /// inferred (the `impl Bar<'a>` and the `impl
    /// Bar<'b>`). Conceptually, this is sugar for a setup where we
    /// define underlying abstract types (`Foo1`, `Foo2`) and then, in
    /// the return type of `foo`, we *reference* those definitions:
    ///
    ///     abstract type Foo1<'x>: Bar<'x>;
    ///     abstract type Foo2<'x>: Bar<'x>;
    ///     fn foo<'a, 'b>(..) -> (Foo1<'a>, Foo2<'b>) { .. }
    ///                        //  ^^^^ ^^
    ///                        //  |    |
    ///                        //  |    substs
    ///                        //  def_id
    ///
    /// As indicating in the comments above, each of those references
    /// is (in the compiler) basically a substitution (`substs`)
    /// applied to the type of a suitable `def_id` (which identifies
    /// `Foo1` or `Foo2`).
    ///
    /// Now, at this point in compilation, what we have done is to
    /// replace each of the references (`Foo1<'a>`, `Foo2<'b>`) with
    /// fresh inference variables C1 and C2. We wish to use the values
    /// of these variables to infer the underlying types of `Foo1` and
    /// `Foo2`.  That is, this gives rise to higher-order (pattern) unification
    /// constraints like:
    ///
    ///     for<'a> (Foo1<'a> = C1)
    ///     for<'b> (Foo1<'b> = C2)
    ///
    /// For these equation to be satisfiable, the types `C1` and `C2`
    /// can only refer to a limited set of regions. For example, `C1`
    /// can only refer to `'static` and `'a`, and `C2` can only refer
    /// to `'static` and `'b`. The job of this function is to impose that
    /// constraint.
    ///
    /// Up to this point, C1 and C2 are basically just random type
    /// inference variables, and hence they may contain arbitrary
    /// regions. In fact, it is fairly likely that they do! Consider
    /// this possible definition of `foo`:
    ///
    ///     fn foo<'a, 'b>(x: &'a i32, y: &'b i32) -> (impl Bar<'a>, impl Bar<'b>) {
    ///         (&*x, &*y)
    ///     }
    ///
    /// Here, the values for the concrete types of the two impl
    /// traits will include inference variables:
    ///
    ///     &'0 i32
    ///     &'1 i32
    ///
    /// Ordinarily, the subtyping rules would ensure that these are
    /// sufficiently large. But since `impl Bar<'a>` isn't a specific
    /// type per se, we don't get such constraints by default.  This
    /// is where this function comes into play. It adds extra
    /// constraints to ensure that all the regions which appear in the
    /// inferred type are regions that could validly appear.
    ///
    /// This is actually a bit of a tricky constraint in general. We
    /// want to say that each variable (e.g., `'0``) can only take on
    /// values that were supplied as arguments to the abstract type
    /// (e.g., `'a` for `Foo1<'a>`) or `'static`, which is always in
    /// scope. We don't have a constraint quite of this kind in the current
    /// region checker.
    ///
    /// # The Solution
    ///
    /// We make use of the constraint that we *do* have in the `<=`
    /// relation. To do that, we find the "minimum" of all the
    /// arguments that appear in the substs: that is, some region
    /// which is less than all the others. In the case of `Foo1<'a>`,
    /// that would be `'a` (it's the only choice, after all). Then we
    /// apply that as a least bound to the variables (e.g., `'a <=
    /// '0`).
    ///
    /// In some cases, there is no minimum. Consider this example:
    ///
    ///    fn baz<'a, 'b>() -> impl Trait<'a, 'b> { ... }
    ///
    /// Here we would report an error, because `'a` and `'b` have no
    /// relation to one another.
    ///
    /// # The `free_region_relations` parameter
    ///
    /// The `free_region_relations` argument is used to find the
    /// "minimum" of the regions supplied to a given abstract type.
    /// It must be a relation that can answer whether `'a <= 'b`,
    /// where `'a` and `'b` are regions that appear in the "substs"
    /// for the abstract type references (the `<'a>` in `Foo1<'a>`).
    ///
    /// Note that we do not impose the constraints based on the
    /// generic regions from the `Foo1` definition (e.g., `'x`). This
    /// is because the constraints we are imposing here is basically
    /// the concern of the one generating the constraining type C1,
    /// which is the current function. It also means that we can
    /// take "implied bounds" into account in some cases:
    ///
    ///     trait SomeTrait<'a, 'b> { }
    ///     fn foo<'a, 'b>(_: &'a &'b u32) -> impl SomeTrait<'a, 'b> { .. }
    ///
    /// Here, the fact that `'b: 'a` is known only because of the
    /// implied bounds from the `&'a &'b u32` parameter, and is not
    /// "inherent" to the abstract type definition.
    ///
    /// # Parameters
    ///
    /// - `anon_types` -- the map produced by `instantiate_anon_types`
    /// - `free_region_relations` -- something that can be used to relate
    ///   the free regions (`'a`) that appear in the impl trait.
    pub fn constrain_anon_types<FRR: FreeRegionRelations<'tcx>>(
        &self,
        anon_types: &AnonTypeMap<'tcx>,
        free_region_relations: &FRR,
    ) {
        debug!("constrain_anon_types()");

        for (&def_id, anon_defn) in anon_types {
            self.constrain_anon_type(def_id, anon_defn, free_region_relations);
        }
    }

    fn constrain_anon_type<FRR: FreeRegionRelations<'tcx>>(
        &self,
        def_id: DefId,
        anon_defn: &AnonTypeDecl<'tcx>,
        free_region_relations: &FRR,
    ) {
        debug!("constrain_anon_type()");
        debug!("constrain_anon_type: def_id={:?}", def_id);
        debug!("constrain_anon_type: anon_defn={:#?}", anon_defn);

        let concrete_ty = self.resolve_type_vars_if_possible(&anon_defn.concrete_ty);

        debug!("constrain_anon_type: concrete_ty={:?}", concrete_ty);

        let abstract_type_generics = self.tcx.generics_of(def_id);

        let span = self.tcx.def_span(def_id);

        // If there are required region bounds, we can just skip
        // ahead.  There will already be a registered region
        // obligation related `concrete_ty` to those regions.
        if anon_defn.has_required_region_bounds {
            return;
        }

        // There were no `required_region_bounds`,
        // so we have to search for a `least_region`.
        // Go through all the regions used as arguments to the
        // abstract type. These are the parameters to the abstract
        // type; so in our example above, `substs` would contain
        // `['a]` for the first impl trait and `'b` for the
        // second.
        let mut least_region = None;
        for region_def in &abstract_type_generics.regions {
            // Find the index of this region in the list of substitutions.
            let index = region_def.index as usize;

            // Get the value supplied for this region from the substs.
            let subst_arg = anon_defn.substs[index].as_region().unwrap();

            // Compute the least upper bound of it with the other regions.
            debug!("constrain_anon_types: least_region={:?}", least_region);
            debug!("constrain_anon_types: subst_arg={:?}", subst_arg);
            match least_region {
                None => least_region = Some(subst_arg),
                Some(lr) => {
                    if free_region_relations.sub_free_regions(lr, subst_arg) {
                        // keep the current least region
                    } else if free_region_relations.sub_free_regions(subst_arg, lr) {
                        // switch to `subst_arg`
                        least_region = Some(subst_arg);
                    } else {
                        // There are two regions (`lr` and
                        // `subst_arg`) which are not relatable. We can't
                        // find a best choice.
                        self.tcx
                            .sess
                            .struct_span_err(span, "ambiguous lifetime bound in `impl Trait`")
                            .span_label(
                                span,
                                format!("neither `{}` nor `{}` outlives the other", lr, subst_arg),
                            )
                            .emit();

                        least_region = Some(self.tcx.mk_region(ty::ReEmpty));
                        break;
                    }
                }
            }
        }

        let least_region = least_region.unwrap_or(self.tcx.types.re_static);
        debug!("constrain_anon_types: least_region={:?}", least_region);

        // Require that the type `concrete_ty` outlives
        // `least_region`, modulo any type parameters that appear
        // in the type, which we ignore. This is because impl
        // trait values are assumed to capture all the in-scope
        // type parameters. This little loop here just invokes
        // `outlives` repeatedly, draining all the nested
        // obligations that result.
        let mut types = vec![concrete_ty];
        let bound_region = |r| self.sub_regions(infer::CallReturn(span), least_region, r);
        while let Some(ty) = types.pop() {
            let mut components = self.tcx.outlives_components(ty);
            while let Some(component) = components.pop() {
                match component {
                    Component::Region(r) => {
                        bound_region(r);
                    }

                    Component::Param(_) => {
                        // ignore type parameters like `T`, they are captured
                        // implicitly by the `impl Trait`
                    }

                    Component::UnresolvedInferenceVariable(_) => {
                        // we should get an error that more type
                        // annotations are needed in this case
                        self.tcx
                            .sess
                            .delay_span_bug(span, "unresolved inf var in anon");
                    }

                    Component::Projection(ty::ProjectionTy {
                        substs,
                        item_def_id: _,
                    }) => {
                        for r in substs.regions() {
                            bound_region(r);
                        }
                        types.extend(substs.types());
                    }

                    Component::EscapingProjection(more_components) => {
                        components.extend(more_components);
                    }
                }
            }
        }
    }
}

struct Instantiator<'a, 'gcx: 'tcx, 'tcx: 'a> {
    infcx: &'a InferCtxt<'a, 'gcx, 'tcx>,
    body_id: ast::NodeId,
    param_env: ty::ParamEnv<'tcx>,
    anon_types: AnonTypeMap<'tcx>,
    obligations: Vec<PredicateObligation<'tcx>>,
}

impl<'a, 'gcx, 'tcx> Instantiator<'a, 'gcx, 'tcx> {
    fn instantiate_anon_types_in_map<T: TypeFoldable<'tcx>>(&mut self, value: &T) -> T {
        debug!("instantiate_anon_types_in_map(value={:?})", value);
        value.fold_with(&mut BottomUpFolder {
            tcx: self.infcx.tcx,
            fldop: |ty| if let ty::TyAnon(def_id, substs) = ty.sty {
                self.fold_anon_ty(ty, def_id, substs)
            } else {
                ty
            },
        })
    }

    fn fold_anon_ty(
        &mut self,
        ty: Ty<'tcx>,
        def_id: DefId,
        substs: &'tcx Substs<'tcx>,
    ) -> Ty<'tcx> {
        let infcx = self.infcx;
        let tcx = infcx.tcx;

        debug!(
            "instantiate_anon_types: TyAnon(def_id={:?}, substs={:?})",
            def_id,
            substs
        );

        // Use the same type variable if the exact same TyAnon appears more
        // than once in the return type (e.g. if it's passed to a type alias).
        if let Some(anon_defn) = self.anon_types.get(&def_id) {
            return anon_defn.concrete_ty;
        }
        let span = tcx.def_span(def_id);
        let ty_var = infcx.next_ty_var(TypeVariableOrigin::TypeInference(span));

        let predicates_of = tcx.predicates_of(def_id);
        let bounds = predicates_of.instantiate(tcx, substs);
        debug!("instantiate_anon_types: bounds={:?}", bounds);

        let required_region_bounds = tcx.required_region_bounds(ty, bounds.predicates.clone());
        debug!(
            "instantiate_anon_types: required_region_bounds={:?}",
            required_region_bounds
        );

        self.anon_types.insert(
            def_id,
            AnonTypeDecl {
                substs,
                concrete_ty: ty_var,
                has_required_region_bounds: !required_region_bounds.is_empty(),
            },
        );
        debug!("instantiate_anon_types: ty_var={:?}", ty_var);

        for predicate in bounds.predicates {
            // Change the predicate to refer to the type variable,
            // which will be the concrete type, instead of the TyAnon.
            // This also instantiates nested `impl Trait`.
            let predicate = self.instantiate_anon_types_in_map(&predicate);

            let cause = traits::ObligationCause::new(span, self.body_id, traits::SizedReturnType);

            // Require that the predicate holds for the concrete type.
            debug!("instantiate_anon_types: predicate={:?}", predicate);
            self.obligations
                .push(traits::Obligation::new(cause, self.param_env, predicate));
        }

        ty_var
    }
}
