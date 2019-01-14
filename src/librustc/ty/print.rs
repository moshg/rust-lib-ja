use crate::hir::def::Namespace;
use crate::hir::map::DefPathData;
use crate::hir::def_id::{CrateNum, DefId, CRATE_DEF_INDEX, LOCAL_CRATE};
use crate::middle::region;
use crate::ty::{self, DefIdTree, Ty, TyCtxt, TypeFoldable};
use crate::ty::subst::{Kind, Subst, SubstsRef, UnpackedKind};
use crate::middle::cstore::{ExternCrate, ExternCrateSource};
use syntax::symbol::{keywords, Symbol};

use rustc_data_structures::fx::FxHashSet;
use syntax::symbol::InternedString;

use std::cell::Cell;
use std::fmt::{self, Write as _};
use std::iter;
use std::ops::Deref;

thread_local! {
    static FORCE_IMPL_FILENAME_LINE: Cell<bool> = Cell::new(false);
    static SHOULD_PREFIX_WITH_CRATE: Cell<bool> = Cell::new(false);
}

/// Force us to name impls with just the filename/line number. We
/// normally try to use types. But at some points, notably while printing
/// cycle errors, this can result in extra or suboptimal error output,
/// so this variable disables that check.
pub fn with_forced_impl_filename_line<F: FnOnce() -> R, R>(f: F) -> R {
    FORCE_IMPL_FILENAME_LINE.with(|force| {
        let old = force.get();
        force.set(true);
        let result = f();
        force.set(old);
        result
    })
}

/// Adds the `crate::` prefix to paths where appropriate.
pub fn with_crate_prefix<F: FnOnce() -> R, R>(f: F) -> R {
    SHOULD_PREFIX_WITH_CRATE.with(|flag| {
        let old = flag.get();
        flag.set(true);
        let result = f();
        flag.set(old);
        result
    })
}

// FIXME(eddyb) this module uses `pub(crate)` for things used only
// from `ppaux` - when that is removed, they can be re-privatized.

/// The "region highlights" are used to control region printing during
/// specific error messages. When a "region highlight" is enabled, it
/// gives an alternate way to print specific regions. For now, we
/// always print those regions using a number, so something like "`'0`".
///
/// Regions not selected by the region highlight mode are presently
/// unaffected.
#[derive(Copy, Clone, Default)]
pub struct RegionHighlightMode {
    /// If enabled, when we see the selected region, use "`'N`"
    /// instead of the ordinary behavior.
    highlight_regions: [Option<(ty::RegionKind, usize)>; 3],

    /// If enabled, when printing a "free region" that originated from
    /// the given `ty::BoundRegion`, print it as "`'1`". Free regions that would ordinarily
    /// have names print as normal.
    ///
    /// This is used when you have a signature like `fn foo(x: &u32,
    /// y: &'a u32)` and we want to give a name to the region of the
    /// reference `x`.
    highlight_bound_region: Option<(ty::BoundRegion, usize)>,
}

impl RegionHighlightMode {
    /// If `region` and `number` are both `Some`, invokes
    /// `highlighting_region`.
    pub fn maybe_highlighting_region(
        &mut self,
        region: Option<ty::Region<'_>>,
        number: Option<usize>,
    ) {
        if let Some(k) = region {
            if let Some(n) = number {
                self.highlighting_region(k, n);
            }
        }
    }

    /// Highlights the region inference variable `vid` as `'N`.
    pub fn highlighting_region(
        &mut self,
        region: ty::Region<'_>,
        number: usize,
    ) {
        let num_slots = self.highlight_regions.len();
        let first_avail_slot = self.highlight_regions.iter_mut()
            .filter(|s| s.is_none())
            .next()
            .unwrap_or_else(|| {
                bug!(
                    "can only highlight {} placeholders at a time",
                    num_slots,
                )
            });
        *first_avail_slot = Some((*region, number));
    }

    /// Convenience wrapper for `highlighting_region`.
    pub fn highlighting_region_vid(
        &mut self,
        vid: ty::RegionVid,
        number: usize,
    ) {
        self.highlighting_region(&ty::ReVar(vid), number)
    }

    /// Returns `Some(n)` with the number to use for the given region, if any.
    fn region_highlighted(&self, region: ty::Region<'_>) -> Option<usize> {
        self
            .highlight_regions
            .iter()
            .filter_map(|h| match h {
                Some((r, n)) if r == region => Some(*n),
                _ => None,
            })
            .next()
    }

    /// Highlight the given bound region.
    /// We can only highlight one bound region at a time. See
    /// the field `highlight_bound_region` for more detailed notes.
    pub fn highlighting_bound_region(
        &mut self,
        br: ty::BoundRegion,
        number: usize,
    ) {
        assert!(self.highlight_bound_region.is_none());
        self.highlight_bound_region = Some((br, number));
    }
}

struct LateBoundRegionNameCollector(FxHashSet<InternedString>);
impl<'tcx> ty::fold::TypeVisitor<'tcx> for LateBoundRegionNameCollector {
    fn visit_region(&mut self, r: ty::Region<'tcx>) -> bool {
        match *r {
            ty::ReLateBound(_, ty::BrNamed(_, name)) => {
                self.0.insert(name);
            },
            _ => {},
        }
        r.super_visit_with(self)
    }
}

pub(crate) struct PrintConfig {
    pub(crate) is_debug: bool,
    pub(crate) is_verbose: bool,
    pub(crate) identify_regions: bool,
    pub(crate) used_region_names: Option<FxHashSet<InternedString>>,
    pub(crate) region_index: usize,
    pub(crate) binder_depth: usize,
}

impl PrintConfig {
    pub(crate) fn new(tcx: TyCtxt<'_, '_, '_>) -> Self {
        PrintConfig {
            is_debug: false,
            is_verbose: tcx.sess.verbose(),
            identify_regions: tcx.sess.opts.debugging_opts.identify_regions,
            used_region_names: None,
            region_index: 0,
            binder_depth: 0,
        }
    }
}

pub struct PrintCx<'a, 'gcx, 'tcx, P> {
    pub tcx: TyCtxt<'a, 'gcx, 'tcx>,
    pub printer: P,
    pub(crate) config: &'a mut PrintConfig,
}

// HACK(eddyb) this is solely for `self: PrintCx<Self>`, e.g. to
// implement traits on the printer and call the methods on the context.
impl<P> Deref for PrintCx<'_, '_, '_, P> {
    type Target = P;
    fn deref(&self) -> &P {
        &self.printer
    }
}

impl<'a, 'gcx, 'tcx, P> PrintCx<'a, 'gcx, 'tcx, P> {
    pub fn with<R>(
        tcx: TyCtxt<'a, 'gcx, 'tcx>,
        printer: P,
        f: impl FnOnce(PrintCx<'_, 'gcx, 'tcx, P>) -> R,
    ) -> R {
        f(PrintCx {
            tcx,
            printer,
            config: &mut PrintConfig::new(tcx),
        })
    }

    pub(crate) fn with_tls_tcx<R>(printer: P, f: impl FnOnce(PrintCx<'_, '_, '_, P>) -> R) -> R {
        ty::tls::with(|tcx| PrintCx::with(tcx, printer, f))
    }
    pub(crate) fn prepare_late_bound_region_info<T>(&mut self, value: &ty::Binder<T>)
    where T: TypeFoldable<'tcx>
    {
        let mut collector = LateBoundRegionNameCollector(Default::default());
        value.visit_with(&mut collector);
        self.config.used_region_names = Some(collector.0);
        self.config.region_index = 0;
    }
}

pub trait Print<'tcx, P> {
    type Output;
    type Error;

    fn print(&self, cx: PrintCx<'_, '_, 'tcx, P>) -> Result<Self::Output, Self::Error>;
    fn print_display(
        &self,
        cx: PrintCx<'_, '_, 'tcx, P>,
    ) -> Result<Self::Output, Self::Error> {
        let old_debug = cx.config.is_debug;
        cx.config.is_debug = false;
        let result = self.print(PrintCx {
            tcx: cx.tcx,
            printer: cx.printer,
            config: cx.config,
        });
        cx.config.is_debug = old_debug;
        result
    }
    fn print_debug(&self, cx: PrintCx<'_, '_, 'tcx, P>) -> Result<Self::Output, Self::Error> {
        let old_debug = cx.config.is_debug;
        cx.config.is_debug = true;
        let result = self.print(PrintCx {
            tcx: cx.tcx,
            printer: cx.printer,
            config: cx.config,
        });
        cx.config.is_debug = old_debug;
        result
    }
}

pub trait Printer: Sized {
    type Error;

    type Path;
    type Region;
    type Type;

    fn print_def_path(
        self: PrintCx<'_, '_, 'tcx, Self>,
        def_id: DefId,
        substs: Option<SubstsRef<'tcx>>,
        projections: impl Iterator<Item = ty::ExistentialProjection<'tcx>>,
    ) -> Result<Self::Path, Self::Error> {
        self.default_print_def_path(def_id, substs, projections)
    }
    fn print_impl_path(
        self: PrintCx<'_, '_, 'tcx, Self>,
        impl_def_id: DefId,
        substs: Option<SubstsRef<'tcx>>,
        self_ty: Ty<'tcx>,
        trait_ref: Option<ty::TraitRef<'tcx>>,
    ) -> Result<Self::Path, Self::Error> {
        self.default_print_impl_path(impl_def_id, substs, self_ty, trait_ref)
    }

    fn print_region(
        self: PrintCx<'_, '_, '_, Self>,
        region: ty::Region<'_>,
    ) -> Result<Self::Region, Self::Error>;

    fn print_type(
        self: PrintCx<'_, '_, 'tcx, Self>,
        ty: Ty<'tcx>,
    ) -> Result<Self::Type, Self::Error>;

    fn path_crate(
        self: PrintCx<'_, '_, '_, Self>,
        cnum: CrateNum,
    ) -> Result<Self::Path, Self::Error>;
    fn path_qualified(
        self: PrintCx<'_, '_, 'tcx, Self>,
        self_ty: Ty<'tcx>,
        trait_ref: Option<ty::TraitRef<'tcx>>,
    ) -> Result<Self::Path, Self::Error>;

    fn path_append_impl<'gcx, 'tcx>(
        self: PrintCx<'_, 'gcx, 'tcx, Self>,
        print_prefix: impl FnOnce(
            PrintCx<'_, 'gcx, 'tcx, Self>,
        ) -> Result<Self::Path, Self::Error>,
        self_ty: Ty<'tcx>,
        trait_ref: Option<ty::TraitRef<'tcx>>,
    ) -> Result<Self::Path, Self::Error>;
    fn path_append<'gcx, 'tcx>(
        self: PrintCx<'_, 'gcx, 'tcx, Self>,
        print_prefix: impl FnOnce(
            PrintCx<'_, 'gcx, 'tcx, Self>,
        ) -> Result<Self::Path, Self::Error>,
        text: &str,
    ) -> Result<Self::Path, Self::Error>;
    fn path_generic_args<'gcx, 'tcx>(
        self: PrintCx<'_, 'gcx, 'tcx, Self>,
        print_prefix: impl FnOnce(
            PrintCx<'_, 'gcx, 'tcx, Self>,
        ) -> Result<Self::Path, Self::Error>,
        params: &[ty::GenericParamDef],
        substs: SubstsRef<'tcx>,
        projections: impl Iterator<Item = ty::ExistentialProjection<'tcx>>,
    ) -> Result<Self::Path, Self::Error>;
}

/// Trait for printers that pretty-print using `fmt::Write` to the printer.
pub trait PrettyPrinter:
    Printer<
        Error = fmt::Error,
        Path = Self,
        Region = Self,
        Type = Self,
    > +
    fmt::Write
{
    /// Enter a nested print context, for pretty-printing
    /// nested components in some larger context.
    fn nest<'a, 'gcx, 'tcx, E>(
        self: PrintCx<'a, 'gcx, 'tcx, Self>,
        f: impl FnOnce(PrintCx<'_, 'gcx, 'tcx, Self>) -> Result<Self, E>,
    ) -> Result<PrintCx<'a, 'gcx, 'tcx, Self>, E> {
        let printer = f(PrintCx {
            tcx: self.tcx,
            printer: self.printer,
            config: self.config,
        })?;
        Ok(PrintCx {
            tcx: self.tcx,
            printer,
            config: self.config,
        })
    }

    /// Like `print_def_path` but for value paths.
    fn print_value_path(
        self: PrintCx<'_, '_, 'tcx, Self>,
        def_id: DefId,
        substs: Option<SubstsRef<'tcx>>,
    ) -> Result<Self::Path, Self::Error> {
        self.print_def_path(def_id, substs, iter::empty())
    }

    /// Print `<...>` around what `f` prints.
    fn generic_delimiters<'gcx, 'tcx>(
        self: PrintCx<'_, 'gcx, 'tcx, Self>,
        f: impl FnOnce(PrintCx<'_, 'gcx, 'tcx, Self>) -> Result<Self, Self::Error>,
    ) -> Result<Self, Self::Error>;

    /// Return `true` if the region should be printed in path generic args
    /// even when it's `'_`, such as in e.g. `Foo<'_, '_, '_>`.
    fn always_print_region_in_paths(
        self: &PrintCx<'_, '_, '_, Self>,
        _region: ty::Region<'_>,
    ) -> bool {
        false
    }

    // HACK(eddyb) Trying to print a lifetime might not print anything, which
    // may need special handling in the caller (of `ty::RegionKind::print`).
    // To avoid printing to a temporary string (which isn't even supported),
    // the `print_region_outputs_anything` method can instead be used to
    // determine this, ahead of time.
    //
    // NB: this must be kept in sync with the implementation of `print_region`.
    fn print_region_outputs_anything(
        self: &PrintCx<'_, '_, '_, Self>,
        region: ty::Region<'_>,
    ) -> bool;
}

macro_rules! nest {
    ($cx:ident, $closure:expr) => {
        $cx = $cx.nest($closure)?
    }
}

impl<'a, 'gcx, 'tcx> TyCtxt<'a, 'gcx, 'tcx> {
    // HACK(eddyb) get rid of `def_path_str` and/or pass `Namespace` explicitly always
    // (but also some things just print a `DefId` generally so maybe we need this?)
    fn guess_def_namespace(self, def_id: DefId) -> Namespace {
        match self.def_key(def_id).disambiguated_data.data {
            DefPathData::ValueNs(..) |
            DefPathData::EnumVariant(..) |
            DefPathData::Field(..) |
            DefPathData::AnonConst |
            DefPathData::ConstParam(..) |
            DefPathData::ClosureExpr |
            DefPathData::StructCtor => Namespace::ValueNS,

            DefPathData::MacroDef(..) => Namespace::MacroNS,

            _ => Namespace::TypeNS,
        }
    }

    /// Returns a string identifying this `DefId`. This string is
    /// suitable for user output.
    pub fn def_path_str(self, def_id: DefId) -> String {
        let ns = self.guess_def_namespace(def_id);
        debug!("def_path_str: def_id={:?}, ns={:?}", def_id, ns);
        let mut s = String::new();
        let _ = PrintCx::with(self, FmtPrinter::new(&mut s, ns), |cx| {
            cx.print_def_path(def_id, None, iter::empty())
        });
        s
    }
}

impl<P: Printer> PrintCx<'a, 'gcx, 'tcx, P> {
    pub fn default_print_def_path(
        self,
        def_id: DefId,
        substs: Option<SubstsRef<'tcx>>,
        projections: impl Iterator<Item = ty::ExistentialProjection<'tcx>>,
    ) -> Result<P::Path, P::Error> {
        debug!("default_print_def_path: def_id={:?}, substs={:?}", def_id, substs);
        let key = self.tcx.def_key(def_id);
        debug!("default_print_def_path: key={:?}", key);

        match key.disambiguated_data.data {
            DefPathData::CrateRoot => {
                assert!(key.parent.is_none());
                self.path_crate(def_id.krate)
            }

            DefPathData::Impl => {
                let mut self_ty = self.tcx.type_of(def_id);
                if let Some(substs) = substs {
                    self_ty = self_ty.subst(self.tcx, substs);
                }

                let mut impl_trait_ref = self.tcx.impl_trait_ref(def_id);
                if let Some(substs) = substs {
                    impl_trait_ref = impl_trait_ref.subst(self.tcx, substs);
                }
                self.print_impl_path(def_id, substs, self_ty, impl_trait_ref)
            }

            _ => {
                let generics = substs.map(|_| self.tcx.generics_of(def_id));
                let generics_parent = generics.as_ref().and_then(|g| g.parent);
                let parent_def_id = DefId { index: key.parent.unwrap(), ..def_id };
                let print_parent_path = |cx: PrintCx<'_, 'gcx, 'tcx, P>| {
                    if let Some(generics_parent_def_id) = generics_parent {
                        assert_eq!(parent_def_id, generics_parent_def_id);

                        // FIXME(eddyb) try to move this into the parent's printing
                        // logic, instead of doing it when printing the child.
                        let parent_generics = cx.tcx.generics_of(parent_def_id);
                        let parent_has_own_self =
                            parent_generics.has_self && parent_generics.parent_count == 0;
                        if let (Some(substs), true) = (substs, parent_has_own_self) {
                            let trait_ref = ty::TraitRef::new(parent_def_id, substs);
                            cx.path_qualified(trait_ref.self_ty(), Some(trait_ref))
                        } else {
                            cx.print_def_path(parent_def_id, substs, iter::empty())
                        }
                    } else {
                        cx.print_def_path(parent_def_id, None, iter::empty())
                    }
                };
                let print_path = |cx: PrintCx<'_, 'gcx, 'tcx, P>| {
                    match key.disambiguated_data.data {
                        // Skip `::{{constructor}}` on tuple/unit structs.
                        DefPathData::StructCtor => print_parent_path(cx),

                        _ => {
                            cx.path_append(
                                print_parent_path,
                                &key.disambiguated_data.data.as_interned_str().as_str(),
                            )
                        }
                    }
                };

                if let (Some(generics), Some(substs)) = (generics, substs) {
                    let has_own_self = generics.has_self && generics.parent_count == 0;
                    let params = &generics.params[has_own_self as usize..];
                    self.path_generic_args(print_path, params, substs, projections)
                } else {
                    print_path(self)
                }
            }
        }
    }

    fn default_print_impl_path(
        self,
        impl_def_id: DefId,
        _substs: Option<SubstsRef<'tcx>>,
        self_ty: Ty<'tcx>,
        impl_trait_ref: Option<ty::TraitRef<'tcx>>,
    ) -> Result<P::Path, P::Error> {
        debug!("default_print_impl_path: impl_def_id={:?}, self_ty={}, impl_trait_ref={:?}",
               impl_def_id, self_ty, impl_trait_ref);

        // Decide whether to print the parent path for the impl.
        // Logically, since impls are global, it's never needed, but
        // users may find it useful. Currently, we omit the parent if
        // the impl is either in the same module as the self-type or
        // as the trait.
        let parent_def_id = self.tcx.parent(impl_def_id).unwrap();
        let in_self_mod = match characteristic_def_id_of_type(self_ty) {
            None => false,
            Some(ty_def_id) => self.tcx.parent(ty_def_id) == Some(parent_def_id),
        };
        let in_trait_mod = match impl_trait_ref {
            None => false,
            Some(trait_ref) => self.tcx.parent(trait_ref.def_id) == Some(parent_def_id),
        };

        if !in_self_mod && !in_trait_mod {
            // If the impl is not co-located with either self-type or
            // trait-type, then fallback to a format that identifies
            // the module more clearly.
            self.path_append_impl(
                |cx| cx.print_def_path(parent_def_id, None, iter::empty()),
                self_ty,
                impl_trait_ref,
            )
        } else {
            // Otherwise, try to give a good form that would be valid language
            // syntax. Preferably using associated item notation.
            self.path_qualified(self_ty, impl_trait_ref)
        }
    }
}

/// As a heuristic, when we see an impl, if we see that the
/// 'self type' is a type defined in the same module as the impl,
/// we can omit including the path to the impl itself. This
/// function tries to find a "characteristic `DefId`" for a
/// type. It's just a heuristic so it makes some questionable
/// decisions and we may want to adjust it later.
pub fn characteristic_def_id_of_type(ty: Ty<'_>) -> Option<DefId> {
    match ty.sty {
        ty::Adt(adt_def, _) => Some(adt_def.did),

        ty::Dynamic(data, ..) => data.principal_def_id(),

        ty::Array(subty, _) |
        ty::Slice(subty) => characteristic_def_id_of_type(subty),

        ty::RawPtr(mt) => characteristic_def_id_of_type(mt.ty),

        ty::Ref(_, ty, _) => characteristic_def_id_of_type(ty),

        ty::Tuple(ref tys) => tys.iter()
                                   .filter_map(|ty| characteristic_def_id_of_type(ty))
                                   .next(),

        ty::FnDef(def_id, _) |
        ty::Closure(def_id, _) |
        ty::Generator(def_id, _, _) |
        ty::Foreign(def_id) => Some(def_id),

        ty::Bool |
        ty::Char |
        ty::Int(_) |
        ty::Uint(_) |
        ty::Str |
        ty::FnPtr(_) |
        ty::Projection(_) |
        ty::Placeholder(..) |
        ty::UnnormalizedProjection(..) |
        ty::Param(_) |
        ty::Opaque(..) |
        ty::Infer(_) |
        ty::Bound(..) |
        ty::Error |
        ty::GeneratorWitness(..) |
        ty::Never |
        ty::Float(_) => None,
    }
}

pub struct FmtPrinter<F: fmt::Write> {
    pub(crate) fmt: F,
    empty: bool,
    in_value: bool,
    pub region_highlight_mode: RegionHighlightMode,
}

impl<F: fmt::Write> FmtPrinter<F> {
    pub fn new(fmt: F, ns: Namespace) -> Self {
        FmtPrinter {
            fmt,
            empty: true,
            in_value: ns == Namespace::ValueNS,
            region_highlight_mode: RegionHighlightMode::default(),
        }
    }
}

impl<'gcx, 'tcx, P: PrettyPrinter> PrintCx<'_, 'gcx, 'tcx, P> {
    /// If possible, this returns a global path resolving to `def_id` that is visible
    /// from at least one local module and returns true. If the crate defining `def_id` is
    /// declared with an `extern crate`, the path is guaranteed to use the `extern crate`.
    fn try_print_visible_def_path(
        mut self,
        def_id: DefId,
    ) -> Result<(P, bool), P::Error> {
        debug!("try_print_visible_def_path: def_id={:?}", def_id);

        // If `def_id` is a direct or injected extern crate, return the
        // path to the crate followed by the path to the item within the crate.
        if def_id.index == CRATE_DEF_INDEX {
            let cnum = def_id.krate;

            if cnum == LOCAL_CRATE {
                return Ok((self.path_crate(cnum)?, true));
            }

            // In local mode, when we encounter a crate other than
            // LOCAL_CRATE, execution proceeds in one of two ways:
            //
            // 1. for a direct dependency, where user added an
            //    `extern crate` manually, we put the `extern
            //    crate` as the parent. So you wind up with
            //    something relative to the current crate.
            // 2. for an extern inferred from a path or an indirect crate,
            //    where there is no explicit `extern crate`, we just prepend
            //    the crate name.
            match *self.tcx.extern_crate(def_id) {
                Some(ExternCrate {
                    src: ExternCrateSource::Extern(def_id),
                    direct: true,
                    span,
                    ..
                }) => {
                    debug!("try_print_visible_def_path: def_id={:?}", def_id);
                    return Ok((if !span.is_dummy() {
                        self.print_def_path(def_id, None, iter::empty())?
                    } else {
                        self.path_crate(cnum)?
                    }, true));
                }
                None => {
                    return Ok((self.path_crate(cnum)?, true));
                }
                _ => {},
            }
        }

        if def_id.is_local() {
            return Ok((self.printer, false));
        }

        let visible_parent_map = self.tcx.visible_parent_map(LOCAL_CRATE);

        let mut cur_def_key = self.tcx.def_key(def_id);
        debug!("try_print_visible_def_path: cur_def_key={:?}", cur_def_key);

        // For a UnitStruct or TupleStruct we want the name of its parent rather than <unnamed>.
        if let DefPathData::StructCtor = cur_def_key.disambiguated_data.data {
            let parent = DefId {
                krate: def_id.krate,
                index: cur_def_key.parent.expect("DefPathData::StructCtor missing a parent"),
            };

            cur_def_key = self.tcx.def_key(parent);
        }

        let visible_parent = match visible_parent_map.get(&def_id).cloned() {
            Some(parent) => parent,
            None => return Ok((self.printer, false)),
        };
        // HACK(eddyb) this uses `nest` to avoid knowing ahead of time whether
        // the entire path will succeed or not. To support printers that do not
        // implement `PrettyPrinter`, a `Vec` or linked list on the stack would
        // need to be built, before starting to print anything.
        let mut prefix_success = false;
        nest!(self, |cx| {
            let (printer, success) = cx.try_print_visible_def_path(visible_parent)?;
            prefix_success = success;
            Ok(printer)
        });
        if !prefix_success {
            return Ok((self.printer, false));
        };
        let actual_parent = self.tcx.parent(def_id);

        let data = cur_def_key.disambiguated_data.data;
        debug!(
            "try_print_visible_def_path: data={:?} visible_parent={:?} actual_parent={:?}",
            data, visible_parent, actual_parent,
        );

        let symbol = match data {
            // In order to output a path that could actually be imported (valid and visible),
            // we need to handle re-exports correctly.
            //
            // For example, take `std::os::unix::process::CommandExt`, this trait is actually
            // defined at `std::sys::unix::ext::process::CommandExt` (at time of writing).
            //
            // `std::os::unix` rexports the contents of `std::sys::unix::ext`. `std::sys` is
            // private so the "true" path to `CommandExt` isn't accessible.
            //
            // In this case, the `visible_parent_map` will look something like this:
            //
            // (child) -> (parent)
            // `std::sys::unix::ext::process::CommandExt` -> `std::sys::unix::ext::process`
            // `std::sys::unix::ext::process` -> `std::sys::unix::ext`
            // `std::sys::unix::ext` -> `std::os`
            //
            // This is correct, as the visible parent of `std::sys::unix::ext` is in fact
            // `std::os`.
            //
            // When printing the path to `CommandExt` and looking at the `cur_def_key` that
            // corresponds to `std::sys::unix::ext`, we would normally print `ext` and then go
            // to the parent - resulting in a mangled path like
            // `std::os::ext::process::CommandExt`.
            //
            // Instead, we must detect that there was a re-export and instead print `unix`
            // (which is the name `std::sys::unix::ext` was re-exported as in `std::os`). To
            // do this, we compare the parent of `std::sys::unix::ext` (`std::sys::unix`) with
            // the visible parent (`std::os`). If these do not match, then we iterate over
            // the children of the visible parent (as was done when computing
            // `visible_parent_map`), looking for the specific child we currently have and then
            // have access to the re-exported name.
            DefPathData::Module(actual_name) |
            DefPathData::TypeNs(actual_name) if Some(visible_parent) != actual_parent => {
                self.tcx.item_children(visible_parent)
                    .iter()
                    .find(|child| child.def.def_id() == def_id)
                    .map(|child| child.ident.as_str())
                    .unwrap_or_else(|| actual_name.as_str())
            }
            _ => {
                data.get_opt_name().map(|n| n.as_str()).unwrap_or_else(|| {
                    // Re-exported `extern crate` (#43189).
                    if let DefPathData::CrateRoot = data {
                        self.tcx.original_crate_name(def_id.krate).as_str()
                    } else {
                        Symbol::intern("<unnamed>").as_str()
                    }
                })
            },
        };
        debug!("try_print_visible_def_path: symbol={:?}", symbol);
        Ok((self.path_append(|cx| Ok(cx.printer), &symbol)?, true))
    }

    pub fn pretty_path_qualified(
        self,
        self_ty: Ty<'tcx>,
        trait_ref: Option<ty::TraitRef<'tcx>>,
    ) -> Result<P::Path, P::Error> {
        if trait_ref.is_none() {
            // Inherent impls. Try to print `Foo::bar` for an inherent
            // impl on `Foo`, but fallback to `<Foo>::bar` if self-type is
            // anything other than a simple path.
            match self_ty.sty {
                ty::Adt(..) | ty::Foreign(_) |
                ty::Bool | ty::Char | ty::Str |
                ty::Int(_) | ty::Uint(_) | ty::Float(_) => {
                    return self_ty.print_display(self);
                }

                _ => {}
            }
        }

        self.generic_delimiters(|mut cx| {
            nest!(cx, |cx| self_ty.print_display(cx));
            if let Some(trait_ref) = trait_ref {
                write!(cx.printer, " as ")?;
                nest!(cx, |cx| trait_ref.print_display(cx));
            }
            Ok(cx.printer)
        })
    }

    pub fn pretty_path_append_impl(
        mut self,
        print_prefix: impl FnOnce(
            PrintCx<'_, 'gcx, 'tcx, P>,
        ) -> Result<P::Path, P::Error>,
        self_ty: Ty<'tcx>,
        trait_ref: Option<ty::TraitRef<'tcx>>,
    ) -> Result<P::Path, P::Error> {
        nest!(self, print_prefix);

        self.generic_delimiters(|mut cx| {
            write!(cx.printer, "impl ")?;
            if let Some(trait_ref) = trait_ref {
                nest!(cx, |cx| trait_ref.print_display(cx));
                write!(cx.printer, " for ")?;
            }
            nest!(cx, |cx| self_ty.print_display(cx));

            Ok(cx.printer)
        })
    }

    pub fn pretty_path_generic_args(
        mut self,
        print_prefix: impl FnOnce(
            PrintCx<'_, 'gcx, 'tcx, P>,
        ) -> Result<P::Path, P::Error>,
        params: &[ty::GenericParamDef],
        substs: SubstsRef<'tcx>,
        projections: impl Iterator<Item = ty::ExistentialProjection<'tcx>>,
    ) -> Result<P::Path, P::Error> {
        nest!(self, |cx| print_prefix(cx));

        // Don't print `'_` if there's no printed region.
        let print_regions = params.iter().any(|param| {
            match substs[param.index as usize].unpack() {
                UnpackedKind::Lifetime(r) => {
                    self.always_print_region_in_paths(r) ||
                    self.print_region_outputs_anything(r)
                }
                _ => false,
            }
        });

        // Don't print args that are the defaults of their respective parameters.
        let num_supplied_defaults = if self.config.is_verbose {
            0
        } else {
            params.iter().rev().take_while(|param| {
                match param.kind {
                    ty::GenericParamDefKind::Lifetime => false,
                    ty::GenericParamDefKind::Type { has_default, .. } => {
                        has_default && substs[param.index as usize] == Kind::from(
                            self.tcx.type_of(param.def_id).subst(self.tcx, substs)
                        )
                    }
                    ty::GenericParamDefKind::Const => false, // FIXME(const_generics:defaults)
                }
            }).count()
        };

        let params = &params[..params.len() - num_supplied_defaults];
        let mut args = params.iter().map(|param| {
            substs[param.index as usize].unpack()
        }).filter(|arg| {
            match arg {
                UnpackedKind::Lifetime(_) => print_regions,
                _ => true,
            }
        });
        let arg0 = args.next();

        let mut projections = projections;
        let projection0 = projections.next();

        if arg0.is_none() && projection0.is_none() {
            return Ok(self.printer);
        }

        self.generic_delimiters(|mut cx| {
            let mut empty = true;
            let mut maybe_comma = |cx: &mut Self| {
                if empty {
                    empty = false;
                    Ok(())
                } else {
                    write!(cx.printer, ", ")
                }
            };

            for arg in arg0.into_iter().chain(args) {
                maybe_comma(&mut cx)?;

                match arg {
                    UnpackedKind::Lifetime(region) => {
                        if !cx.print_region_outputs_anything(region) {
                            // This happens when the value of the region
                            // parameter is not easily serialized. This may be
                            // because the user omitted it in the first place,
                            // or because it refers to some block in the code,
                            // etc. I'm not sure how best to serialize this.
                            write!(cx.printer, "'_")?;
                        } else {
                            nest!(cx, |cx| region.print_display(cx));
                        }
                    }
                    UnpackedKind::Type(ty) => {
                        nest!(cx, |cx| ty.print_display(cx));
                    }
                    UnpackedKind::Const(ct) => {
                        nest!(cx, |cx| ct.print_display(cx));
                    }
                }
            }

            for projection in projection0.into_iter().chain(projections) {
                maybe_comma(&mut cx)?;

                write!(cx.printer, "{}=",
                    cx.tcx.associated_item(projection.item_def_id).ident)?;
                nest!(cx, |cx| projection.ty.print_display(cx));
            }

            Ok(cx.printer)
        })
    }
}

impl<F: fmt::Write> fmt::Write for FmtPrinter<F> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.empty &= s.is_empty();
        self.fmt.write_str(s)
    }
}

impl<F: fmt::Write> Printer for FmtPrinter<F> {
    type Error = fmt::Error;

    type Path = Self;
    type Region = Self;
    type Type = Self;

    fn print_def_path(
        mut self: PrintCx<'_, '_, 'tcx, Self>,
        def_id: DefId,
        substs: Option<SubstsRef<'tcx>>,
        projections: impl Iterator<Item = ty::ExistentialProjection<'tcx>>,
    ) -> Result<Self::Path, Self::Error> {
        // FIXME(eddyb) avoid querying `tcx.generics_of` and `tcx.def_key`
        // both here and in `default_print_def_path`.
        let generics = substs.map(|_| self.tcx.generics_of(def_id));
        if generics.as_ref().and_then(|g| g.parent).is_none() {
            let mut visible_path_success = false;
            nest!(self, |cx| {
                let (printer, success) = cx.try_print_visible_def_path(def_id)?;
                visible_path_success = success;
                Ok(printer)
            });
            if visible_path_success {
                return if let (Some(generics), Some(substs)) = (generics, substs) {
                    let has_own_self = generics.has_self && generics.parent_count == 0;
                    let params = &generics.params[has_own_self as usize..];
                    self.path_generic_args(|cx| Ok(cx.printer), params, substs, projections)
                } else {
                    Ok(self.printer)
                };
            }
        }

        let key = self.tcx.def_key(def_id);
        if let DefPathData::Impl = key.disambiguated_data.data {
            // Always use types for non-local impls, where types are always
            // available, and filename/line-number is mostly uninteresting.
            let use_types =
                !def_id.is_local() || {
                    // Otherwise, use filename/line-number if forced.
                    let force_no_types = FORCE_IMPL_FILENAME_LINE.with(|f| f.get());
                    !force_no_types
                };

            if !use_types {
                // If no type info is available, fall back to
                // pretty printing some span information. This should
                // only occur very early in the compiler pipeline.
                let parent_def_id = DefId { index: key.parent.unwrap(), ..def_id };
                let span = self.tcx.def_span(def_id);
                return self.path_append(
                    |cx| cx.print_def_path(parent_def_id, None, iter::empty()),
                    &format!("<impl at {:?}>", span),
                );
            }
        }

        self.default_print_def_path(def_id, substs, projections)
    }

    fn print_region(
        mut self: PrintCx<'_, '_, '_, Self>,
        region: ty::Region<'_>,
    ) -> Result<Self::Region, Self::Error> {
        // Watch out for region highlights.
        let highlight = self.printer.region_highlight_mode;
        if let Some(n) = highlight.region_highlighted(region) {
            write!(self.printer, "'{}", n)?;
            return Ok(self.printer);
        }

        if self.config.is_verbose {
            return region.print_debug(self);
        }

        // These printouts are concise.  They do not contain all the information
        // the user might want to diagnose an error, but there is basically no way
        // to fit that into a short string.  Hence the recommendation to use
        // `explain_region()` or `note_and_explain_region()`.
        match *region {
            ty::ReEarlyBound(ref data) => {
                if data.name != "'_" {
                    write!(self.printer, "{}", data.name)?;
                }
            }
            ty::ReLateBound(_, br) |
            ty::ReFree(ty::FreeRegion { bound_region: br, .. }) |
            ty::RePlaceholder(ty::Placeholder { name: br, .. }) => {
                if let ty::BrNamed(_, name) = br {
                    if name != "" && name != "'_" {
                        write!(self.printer, "{}", name)?;
                        return Ok(self.printer);
                    }
                }

                if let Some((region, counter)) = highlight.highlight_bound_region {
                    if br == region {
                        write!(self.printer, "'{}", counter)?;
                    }
                }
            }
            ty::ReScope(scope) if self.config.identify_regions => {
                match scope.data {
                    region::ScopeData::Node =>
                        write!(self.printer, "'{}s", scope.item_local_id().as_usize())?,
                    region::ScopeData::CallSite =>
                        write!(self.printer, "'{}cs", scope.item_local_id().as_usize())?,
                    region::ScopeData::Arguments =>
                        write!(self.printer, "'{}as", scope.item_local_id().as_usize())?,
                    region::ScopeData::Destruction =>
                        write!(self.printer, "'{}ds", scope.item_local_id().as_usize())?,
                    region::ScopeData::Remainder(first_statement_index) => write!(self.printer,
                        "'{}_{}rs",
                        scope.item_local_id().as_usize(),
                        first_statement_index.index()
                    )?,
                }
            }
            ty::ReVar(region_vid) if self.config.identify_regions => {
                write!(self.printer, "{:?}", region_vid)?;
            }
            ty::ReVar(_) => {}
            ty::ReScope(_) |
            ty::ReErased => {}
            ty::ReStatic => write!(self.printer, "'static")?,
            ty::ReEmpty => write!(self.printer, "'<empty>")?,

            // The user should never encounter these in unsubstituted form.
            ty::ReClosureBound(vid) => write!(self.printer, "{:?}", vid)?,
        }

        Ok(self.printer)
    }

    fn print_type(
        self: PrintCx<'_, '_, 'tcx, Self>,
        ty: Ty<'tcx>,
    ) -> Result<Self::Type, Self::Error> {
        self.pretty_print_type(ty)
    }

    fn path_crate(
        mut self: PrintCx<'_, '_, '_, Self>,
        cnum: CrateNum,
    ) -> Result<Self::Path, Self::Error> {
        if cnum == LOCAL_CRATE {
            if self.tcx.sess.rust_2018() {
                // We add the `crate::` keyword on Rust 2018, only when desired.
                if SHOULD_PREFIX_WITH_CRATE.with(|flag| flag.get()) {
                    write!(self.printer, "{}", keywords::Crate.name())?;
                }
            }
            Ok(self.printer)
        } else {
            write!(self.printer, "{}", self.tcx.crate_name(cnum))?;
            Ok(self.printer)
        }
    }
    fn path_qualified(
        self: PrintCx<'_, '_, 'tcx, Self>,
        self_ty: Ty<'tcx>,
        trait_ref: Option<ty::TraitRef<'tcx>>,
    ) -> Result<Self::Path, Self::Error> {
        self.pretty_path_qualified(self_ty, trait_ref)
    }

    fn path_append_impl<'gcx, 'tcx>(
        self: PrintCx<'_, 'gcx, 'tcx, Self>,
        print_prefix: impl FnOnce(
            PrintCx<'_, 'gcx, 'tcx, Self>,
        ) -> Result<Self::Path, Self::Error>,
        self_ty: Ty<'tcx>,
        trait_ref: Option<ty::TraitRef<'tcx>>,
    ) -> Result<Self::Path, Self::Error> {
        self.pretty_path_append_impl(|cx| {
            let mut printer = print_prefix(cx)?;

            // HACK(eddyb) this accounts for `generic_delimiters`
            // printing `::<` instead of `<` if `in_value` is set.
            if !printer.empty && !printer.in_value {
                write!(printer, "::")?;
            }

            Ok(printer)
        }, self_ty, trait_ref)
    }
    fn path_append<'gcx, 'tcx>(
        self: PrintCx<'_, 'gcx, 'tcx, Self>,
        print_prefix: impl FnOnce(
            PrintCx<'_, 'gcx, 'tcx, Self>,
        ) -> Result<Self::Path, Self::Error>,
        text: &str,
    ) -> Result<Self::Path, Self::Error> {
        let mut printer = print_prefix(self)?;

        // FIXME(eddyb) `text` should never be empty, but it
        // currently is for `extern { ... }` "foreign modules".
        if !text.is_empty() {
            if !printer.empty {
                write!(printer, "::")?;
            }
            write!(printer, "{}", text)?;
        }

        Ok(printer)
    }
    fn path_generic_args<'gcx, 'tcx>(
        self: PrintCx<'_, 'gcx, 'tcx, Self>,
        print_prefix: impl FnOnce(
            PrintCx<'_, 'gcx, 'tcx, Self>,
        ) -> Result<Self::Path, Self::Error>,
        params: &[ty::GenericParamDef],
        substs: SubstsRef<'tcx>,
        projections: impl Iterator<Item = ty::ExistentialProjection<'tcx>>,
    ) -> Result<Self::Path, Self::Error> {
        self.pretty_path_generic_args(print_prefix, params, substs, projections)
    }
}

impl<F: fmt::Write> PrettyPrinter for FmtPrinter<F> {
    fn nest<'a, 'gcx, 'tcx, E>(
        mut self: PrintCx<'a, 'gcx, 'tcx, Self>,
        f: impl FnOnce(PrintCx<'_, 'gcx, 'tcx, Self>) -> Result<Self, E>,
    ) -> Result<PrintCx<'a, 'gcx, 'tcx, Self>, E> {
        let was_empty = std::mem::replace(&mut self.printer.empty, true);
        let mut printer = f(PrintCx {
            tcx: self.tcx,
            printer: self.printer,
            config: self.config,
        })?;
        printer.empty &= was_empty;
        Ok(PrintCx {
            tcx: self.tcx,
            printer,
            config: self.config,
        })
    }

    fn print_value_path(
        mut self: PrintCx<'_, '_, 'tcx, Self>,
        def_id: DefId,
        substs: Option<SubstsRef<'tcx>>,
    ) -> Result<Self::Path, Self::Error> {
        let was_in_value = std::mem::replace(&mut self.printer.in_value, true);
        let mut printer = self.print_def_path(def_id, substs, iter::empty())?;
        printer.in_value = was_in_value;

        Ok(printer)
    }

    fn generic_delimiters<'gcx, 'tcx>(
        mut self: PrintCx<'_, 'gcx, 'tcx, Self>,
        f: impl FnOnce(PrintCx<'_, 'gcx, 'tcx, Self>) -> Result<Self, Self::Error>,
    ) -> Result<Self, Self::Error> {
        if !self.printer.empty && self.printer.in_value {
            write!(self.printer, "::<")?;
        } else {
            write!(self.printer, "<")?;
        }

        let was_in_value = std::mem::replace(&mut self.printer.in_value, false);
        let mut printer = f(self)?;
        printer.in_value = was_in_value;

        write!(printer, ">")?;
        Ok(printer)
    }

    fn always_print_region_in_paths(
        self: &PrintCx<'_, '_, '_, Self>,
        region: ty::Region<'_>,
    ) -> bool {
        *region != ty::ReErased
    }

    fn print_region_outputs_anything(
        self: &PrintCx<'_, '_, '_, Self>,
        region: ty::Region<'_>,
    ) -> bool {
        let highlight = self.printer.region_highlight_mode;
        if highlight.region_highlighted(region).is_some() {
            return true;
        }

        if self.config.is_verbose {
            return true;
        }

        match *region {
            ty::ReEarlyBound(ref data) => {
                data.name != "" && data.name != "'_"
            }

            ty::ReLateBound(_, br) |
            ty::ReFree(ty::FreeRegion { bound_region: br, .. }) |
            ty::RePlaceholder(ty::Placeholder { name: br, .. }) => {
                if let ty::BrNamed(_, name) = br {
                    if name != "" && name != "'_" {
                        return true;
                    }
                }

                if let Some((region, _)) = highlight.highlight_bound_region {
                    if br == region {
                        return true;
                    }
                }

                false
            }

            ty::ReScope(_) |
            ty::ReVar(_) if self.config.identify_regions => true,

            ty::ReVar(_) |
            ty::ReScope(_) |
            ty::ReErased => false,

            ty::ReStatic |
            ty::ReEmpty |
            ty::ReClosureBound(_) => true,
        }
    }
}
