use hir::def_id::DefId;
use infer::error_reporting::nice_region_error::NiceRegionError;
use infer::lexical_region_resolve::RegionResolutionError;
use infer::ValuePairs;
use infer::{SubregionOrigin, TypeTrace};
use traits::{ObligationCause, ObligationCauseCode};
use ty;
use ty::error::ExpectedFound;
use ty::subst::Substs;
use util::common::ErrorReported;
use util::ppaux::RegionHighlightMode;

impl NiceRegionError<'me, 'gcx, 'tcx> {
    /// When given a `ConcreteFailure` for a function with arguments containing a named region and
    /// an anonymous region, emit an descriptive diagnostic error.
    pub(super) fn try_report_placeholder_conflict(&self) -> Option<ErrorReported> {
        // Check for the first case: relating two trait-refs, and we
        // find a conflict between two placeholders.
        match &self.error {
            Some(RegionResolutionError::SubSupConflict(
                vid,
                _,
                SubregionOrigin::Subtype(TypeTrace {
                    cause,
                    values: ValuePairs::TraitRefs(ExpectedFound { expected, found }),
                }),
                ty::RePlaceholder(sub_placeholder),
                _,
                ty::RePlaceholder(sup_placeholder),
            )) => if expected.def_id == found.def_id {
                return Some(self.try_report_two_placeholders_trait(
                    Some(*vid),
                    cause,
                    Some(*sub_placeholder),
                    Some(*sup_placeholder),
                    expected.def_id,
                    expected.substs,
                    found.substs,
                ));
            } else {
                // I actually can't see why this would be the case ever.
            },

            _ => {}
        }

        None
    }

    // error[E0308]: implementation of `Foo` does not apply to enough lifetimes
    //   --> /home/nmatsakis/tmp/foo.rs:12:5
    //    |
    // 12 |     all::<&'static u32>();
    //    |     ^^^^^^^^^^^^^^^^^^^ lifetime mismatch
    //    |
    //    = note: Due to a where-clause on the function `all`,
    //    = note: `T` must implement `...` for any two lifetimes `'1` and `'2`.
    //    = note: However, the type `T` only implements `...` for some specific lifetime `'2`.
    fn try_report_two_placeholders_trait(
        &self,
        vid: Option<ty::RegionVid>,
        cause: &ObligationCause<'tcx>,
        sub_placeholder: Option<ty::PlaceholderRegion>,
        sup_placeholder: Option<ty::PlaceholderRegion>,
        trait_def_id: DefId,
        expected_substs: &'tcx Substs<'tcx>,
        actual_substs: &'tcx Substs<'tcx>,
    ) -> ErrorReported {
        let mut err = self.tcx.sess.struct_span_err(
            cause.span(&self.tcx),
            &format!(
                "implementation of `{}` is not general enough",
                self.tcx.item_path_str(trait_def_id),
            ),
        );

        match cause.code {
            ObligationCauseCode::ItemObligation(def_id) => {
                err.note(&format!(
                    "Due to a where-clause on `{}`,",
                    self.tcx.item_path_str(def_id),
                ));
            }
            _ => (),
        }

        let expected_trait_ref = ty::TraitRef {
            def_id: trait_def_id,
            substs: expected_substs,
        };
        let actual_trait_ref = ty::TraitRef {
            def_id: trait_def_id,
            substs: actual_substs,
        };

        // Search the expected and actual trait references to see (a)
        // whether the sub/sup placeholders appear in them (sometimes
        // you have a trait ref like `T: Foo<fn(&u8)>`, where the
        // placeholder was created as part of an inner type) and (b)
        // whether the inference variable appears. In each case,
        // assign a counter value in each case if so.
        let mut counter = 0;
        let mut has_sub = None;
        let mut has_sup = None;
        let mut has_vid = None;

        self.tcx
            .for_each_free_region(&expected_trait_ref, |r| match r {
                ty::RePlaceholder(p) => {
                    if Some(*p) == sub_placeholder {
                        if has_sub.is_none() {
                            has_sub = Some(counter);
                            counter += 1;
                        }
                    } else if Some(*p) == sup_placeholder {
                        if has_sup.is_none() {
                            has_sup = Some(counter);
                            counter += 1;
                        }
                    }
                }
                _ => {}
            });

        self.tcx
            .for_each_free_region(&actual_trait_ref, |r| match r {
                ty::ReVar(v) if Some(*v) == vid => {
                    if has_vid.is_none() {
                        has_vid = Some(counter);
                        counter += 1;
                    }
                }
                _ => {}
            });

        maybe_highlight(
            sub_placeholder,
            has_sub,
            RegionHighlightMode::highlighting_placeholder,
            || {
                maybe_highlight(
                    sup_placeholder,
                    has_sup,
                    RegionHighlightMode::highlighting_placeholder,
                    || match (has_sub, has_sup) {
                        (Some(n1), Some(n2)) => {
                            err.note(&format!(
                                "`{}` must implement `{}` \
                                 for any two lifetimes `'{}` and `'{}`",
                                expected_trait_ref.self_ty(),
                                expected_trait_ref,
                                std::cmp::min(n1, n2),
                                std::cmp::max(n1, n2),
                            ));
                        }
                        (Some(n), _) | (_, Some(n)) => {
                            err.note(&format!(
                                "`{}` must implement `{}` \
                                 for any lifetime `'{}`",
                                expected_trait_ref.self_ty(),
                                expected_trait_ref,
                                n,
                            ));
                        }
                        (None, None) => {
                            err.note(&format!(
                                "`{}` must implement `{}`",
                                expected_trait_ref.self_ty(),
                                expected_trait_ref,
                            ));
                        }
                    },
                )
            },
        );

        maybe_highlight(
            vid,
            has_vid,
            RegionHighlightMode::highlighting_region_vid,
            || match has_vid {
                Some(n) => {
                    err.note(&format!(
                        "but `{}` only implements `{}` for some lifetime `'{}`",
                        actual_trait_ref.self_ty(),
                        actual_trait_ref,
                        n
                    ));
                }
                None => {
                    err.note(&format!(
                        "but `{}` only implements `{}`",
                        actual_trait_ref.self_ty(),
                        actual_trait_ref,
                    ));
                }
            },
        );

        err.emit();
        ErrorReported
    }
}

/// If both `thing` and `counter` are `Some`, invoke
/// `highlighting_func` with their contents (and the `op`). Else just
/// invoke `op`.
fn maybe_highlight<T, F, R>(
    thing: Option<T>,
    counter: Option<usize>,
    highlighting_func: impl FnOnce(T, usize, F) -> R,
    op: F,
) -> R
where
    F: FnOnce() -> R,
{
    if let Some(thing) = thing {
        if let Some(n) = counter {
            return highlighting_func(thing, n, op);
        }
    }
    op()
}
