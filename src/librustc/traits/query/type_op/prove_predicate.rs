// Copyright 2016 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use infer::canonical::{Canonical, CanonicalizedQueryResult, QueryResult};
use traits::query::Fallible;
use ty::{ParamEnv, Predicate, TyCtxt};

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
pub struct ProvePredicate<'tcx> {
    pub param_env: ParamEnv<'tcx>,
    pub predicate: Predicate<'tcx>,
}

impl<'tcx> ProvePredicate<'tcx> {
    pub fn new(param_env: ParamEnv<'tcx>, predicate: Predicate<'tcx>) -> Self {
        ProvePredicate {
            param_env,
            predicate,
        }
    }
}

impl<'gcx: 'tcx, 'tcx> super::QueryTypeOp<'gcx, 'tcx> for ProvePredicate<'tcx> {
    type QueryKey = Self;
    type QueryResult = ();

    fn prequery(self, _tcx: TyCtxt<'_, 'gcx, 'tcx>) -> Result<Self::QueryResult, Self::QueryKey> {
        Err(self)
    }

    fn param_env(key: &Self::QueryKey) -> ParamEnv<'tcx> {
        key.param_env
    }

    fn perform_query(
        tcx: TyCtxt<'_, 'gcx, 'tcx>,
        canonicalized: Canonical<'gcx, ProvePredicate<'gcx>>,
    ) -> Fallible<CanonicalizedQueryResult<'gcx, ()>> {
        tcx.type_op_prove_predicate(canonicalized)
    }

    fn shrink_to_tcx_lifetime(
        v: &'a CanonicalizedQueryResult<'gcx, ()>,
    ) -> &'a Canonical<'tcx, QueryResult<'tcx, ()>> {
        v
    }
}

BraceStructTypeFoldableImpl! {
    impl<'tcx> TypeFoldable<'tcx> for ProvePredicate<'tcx> {
        param_env,
        predicate,
    }
}

BraceStructLiftImpl! {
    impl<'a, 'tcx> Lift<'tcx> for ProvePredicate<'a> {
        type Lifted = ProvePredicate<'tcx>;
        param_env,
        predicate,
    }
}

impl_stable_hash_for! {
    struct ProvePredicate<'tcx> { param_env, predicate }
}
