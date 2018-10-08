// Copyright 2016 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use super::OverlapError;

use hir::def_id::DefId;
use ich::{self, StableHashingContext};
use rustc_data_structures::stable_hasher::{HashStable, StableHasher,
                                           StableHasherResult};
use traits;
use ty::{self, TyCtxt, TypeFoldable};
use ty::fast_reject::{self, SimplifiedType};
use rustc_data_structures::sync::Lrc;
use syntax::ast::Ident;
use util::captures::Captures;
use util::nodemap::{DefIdMap, FxHashMap};

/// A per-trait graph of impls in specialization order. At the moment, this
/// graph forms a tree rooted with the trait itself, with all other nodes
/// representing impls, and parent-child relationships representing
/// specializations.
///
/// The graph provides two key services:
///
/// - Construction, which implicitly checks for overlapping impls (i.e., impls
///   that overlap but where neither specializes the other -- an artifact of the
///   simple "chain" rule.
///
/// - Parent extraction. In particular, the graph can give you the *immediate*
///   parents of a given specializing impl, which is needed for extracting
///   default items amongst other things. In the simple "chain" rule, every impl
///   has at most one parent.
#[derive(RustcEncodable, RustcDecodable)]
pub struct Graph {
    // all impls have a parent; the "root" impls have as their parent the def_id
    // of the trait
    parent: DefIdMap<DefId>,

    // the "root" impls are found by looking up the trait's def_id.
    children: DefIdMap<Children>,
}

/// Children of a given impl, grouped into blanket/non-blanket varieties as is
/// done in `TraitDef`.
#[derive(Default, RustcEncodable, RustcDecodable)]
struct Children {
    // Impls of a trait (or specializations of a given impl). To allow for
    // quicker lookup, the impls are indexed by a simplified version of their
    // `Self` type: impls with a simplifiable `Self` are stored in
    // `nonblanket_impls` keyed by it, while all other impls are stored in
    // `blanket_impls`.
    //
    // A similar division is used within `TraitDef`, but the lists there collect
    // together *all* the impls for a trait, and are populated prior to building
    // the specialization graph.

    /// Impls of the trait.
    nonblanket_impls: FxHashMap<fast_reject::SimplifiedType, Vec<DefId>>,

    /// Blanket impls associated with the trait.
    blanket_impls: Vec<DefId>,
}

/// The result of attempting to insert an impl into a group of children.
enum Inserted {
    /// The impl was inserted as a new child in this group of children.
    BecameNewSibling(Option<OverlapError>),

    /// The impl should replace existing impls [X1, ..], because the impl specializes X1, X2, etc.
    ReplaceChildren(Vec<DefId>),

    /// The impl is a specialization of an existing child.
    ShouldRecurseOn(DefId),
}

impl<'a, 'gcx, 'tcx> Children {
    /// Insert an impl into this set of children without comparing to any existing impls
    fn insert_blindly(&mut self,
                      tcx: TyCtxt<'a, 'gcx, 'tcx>,
                      impl_def_id: DefId) {
        let trait_ref = tcx.impl_trait_ref(impl_def_id).unwrap();
        if let Some(sty) = fast_reject::simplify_type(tcx, trait_ref.self_ty(), false) {
            debug!("insert_blindly: impl_def_id={:?} sty={:?}", impl_def_id, sty);
            self.nonblanket_impls.entry(sty).or_default().push(impl_def_id)
        } else {
            debug!("insert_blindly: impl_def_id={:?} sty=None", impl_def_id);
            self.blanket_impls.push(impl_def_id)
        }
    }

    /// Remove an impl from this set of children. Used when replacing
    /// an impl with a parent. The impl must be present in the list of
    /// children already.
    fn remove_existing(&mut self,
                      tcx: TyCtxt<'a, 'gcx, 'tcx>,
                      impl_def_id: DefId) {
        let trait_ref = tcx.impl_trait_ref(impl_def_id).unwrap();
        let vec: &mut Vec<DefId>;
        if let Some(sty) = fast_reject::simplify_type(tcx, trait_ref.self_ty(), false) {
            debug!("remove_existing: impl_def_id={:?} sty={:?}", impl_def_id, sty);
            vec = self.nonblanket_impls.get_mut(&sty).unwrap();
        } else {
            debug!("remove_existing: impl_def_id={:?} sty=None", impl_def_id);
            vec = &mut self.blanket_impls;
        }

        let index = vec.iter().position(|d| *d == impl_def_id).unwrap();
        vec.remove(index);
    }

    /// Attempt to insert an impl into this set of children, while comparing for
    /// specialization relationships.
    fn insert(&mut self,
              tcx: TyCtxt<'a, 'gcx, 'tcx>,
              impl_def_id: DefId,
              simplified_self: Option<SimplifiedType>)
              -> Result<Inserted, OverlapError>
    {
        let mut last_lint = None;
        let mut replace_children = Vec::new();

        debug!(
            "insert(impl_def_id={:?}, simplified_self={:?})",
            impl_def_id,
            simplified_self,
        );

        for possible_sibling in match simplified_self {
            Some(sty) => self.filtered(sty),
            None => self.iter(),
        } {
            debug!(
                "insert: impl_def_id={:?}, simplified_self={:?}, possible_sibling={:?}",
                impl_def_id,
                simplified_self,
                possible_sibling,
            );

            let overlap_error = |overlap: traits::coherence::OverlapResult<'_>| {
                // overlap, but no specialization; error out
                let trait_ref = overlap.impl_header.trait_ref.unwrap();
                let self_ty = trait_ref.self_ty();
                OverlapError {
                    with_impl: possible_sibling,
                    trait_desc: trait_ref.to_string(),
                    // only report the Self type if it has at least
                    // some outer concrete shell; otherwise, it's
                    // not adding much information.
                    self_desc: if self_ty.has_concrete_skeleton() {
                        Some(self_ty.to_string())
                    } else {
                        None
                    },
                    intercrate_ambiguity_causes: overlap.intercrate_ambiguity_causes,
                }
            };

            let tcx = tcx.global_tcx();
            let (le, ge) = traits::overlapping_impls(
                tcx,
                possible_sibling,
                impl_def_id,
                traits::IntercrateMode::Issue43355,
                |overlap| {
                    if tcx.impls_are_allowed_to_overlap(impl_def_id, possible_sibling) {
                        return Ok((false, false));
                    }

                    let le = tcx.specializes((impl_def_id, possible_sibling));
                    let ge = tcx.specializes((possible_sibling, impl_def_id));

                    if le == ge {
                        Err(overlap_error(overlap))
                    } else {
                        Ok((le, ge))
                    }
                },
                || Ok((false, false)),
            )?;

            if le && !ge {
                debug!("descending as child of TraitRef {:?}",
                       tcx.impl_trait_ref(possible_sibling).unwrap());

                // the impl specializes possible_sibling
                return Ok(Inserted::ShouldRecurseOn(possible_sibling));
            } else if ge && !le {
                debug!("placing as parent of TraitRef {:?}",
                       tcx.impl_trait_ref(possible_sibling).unwrap());

                replace_children.push(possible_sibling);
            } else {
                if !tcx.impls_are_allowed_to_overlap(impl_def_id, possible_sibling) {
                    traits::overlapping_impls(
                        tcx,
                        possible_sibling,
                        impl_def_id,
                        traits::IntercrateMode::Fixed,
                        |overlap| last_lint = Some(overlap_error(overlap)),
                        || (),
                    );
                }

                // no overlap (error bailed already via ?)
            }
        }

        if !replace_children.is_empty() {
            return Ok(Inserted::ReplaceChildren(replace_children));
        }

        // no overlap with any potential siblings, so add as a new sibling
        debug!("placing as new sibling");
        self.insert_blindly(tcx, impl_def_id);
        Ok(Inserted::BecameNewSibling(last_lint))
    }

    fn iter(&mut self) -> Box<dyn Iterator<Item = DefId> + '_> {
        let nonblanket = self.nonblanket_impls.iter_mut().flat_map(|(_, v)| v.iter());
        Box::new(self.blanket_impls.iter().chain(nonblanket).cloned())
    }

    fn filtered(&mut self, sty: SimplifiedType) -> Box<dyn Iterator<Item = DefId> + '_> {
        let nonblanket = self.nonblanket_impls.entry(sty).or_default().iter();
        Box::new(self.blanket_impls.iter().chain(nonblanket).cloned())
    }
}

impl<'a, 'gcx, 'tcx> Graph {
    pub fn new() -> Graph {
        Graph {
            parent: Default::default(),
            children: Default::default(),
        }
    }

    /// Insert a local impl into the specialization graph. If an existing impl
    /// conflicts with it (has overlap, but neither specializes the other),
    /// information about the area of overlap is returned in the `Err`.
    pub fn insert(&mut self,
                  tcx: TyCtxt<'a, 'gcx, 'tcx>,
                  impl_def_id: DefId)
                  -> Result<Option<OverlapError>, OverlapError> {
        assert!(impl_def_id.is_local());

        let trait_ref = tcx.impl_trait_ref(impl_def_id).unwrap();
        let trait_def_id = trait_ref.def_id;

        debug!("insert({:?}): inserting TraitRef {:?} into specialization graph",
               impl_def_id, trait_ref);

        // if the reference itself contains an earlier error (e.g., due to a
        // resolution failure), then we just insert the impl at the top level of
        // the graph and claim that there's no overlap (in order to suppress
        // bogus errors).
        if trait_ref.references_error() {
            debug!("insert: inserting dummy node for erroneous TraitRef {:?}, \
                    impl_def_id={:?}, trait_def_id={:?}",
                   trait_ref, impl_def_id, trait_def_id);

            self.parent.insert(impl_def_id, trait_def_id);
            self.children.entry(trait_def_id).or_default()
                .insert_blindly(tcx, impl_def_id);
            return Ok(None);
        }

        let mut parent = trait_def_id;
        let mut last_lint = None;
        let simplified = fast_reject::simplify_type(tcx, trait_ref.self_ty(), false);

        // Descend the specialization tree, where `parent` is the current parent node
        loop {
            use self::Inserted::*;

            let insert_result = self.children.entry(parent).or_default()
                .insert(tcx, impl_def_id, simplified)?;

            match insert_result {
                BecameNewSibling(opt_lint) => {
                    last_lint = opt_lint;
                    break;
                }
                ReplaceChildren(grand_children_to_be) => {
                    // We currently have
                    //
                    //     P
                    //     |
                    //     G
                    //
                    // and we are inserting the impl N. We want to make it:
                    //
                    //     P
                    //     |
                    //     N
                    //     |
                    //     G

                    // Adjust P's list of children: remove G and then add N.
                    {
                        let siblings = self.children
                            .get_mut(&parent)
                            .unwrap();
                        for &grand_child_to_be in &grand_children_to_be {
                            siblings.remove_existing(tcx, grand_child_to_be);
                        }
                        siblings.insert_blindly(tcx, impl_def_id);
                    }

                    // Set G's parent to N and N's parent to P
                    for &grand_child_to_be in &grand_children_to_be {
                        self.parent.insert(grand_child_to_be, impl_def_id);
                    }
                    self.parent.insert(impl_def_id, parent);

                    // Add G as N's child.
                    for &grand_child_to_be in &grand_children_to_be {
                        self.children.entry(impl_def_id).or_default()
                            .insert_blindly(tcx, grand_child_to_be);
                    }
                    break;
                }
                ShouldRecurseOn(new_parent) => {
                    parent = new_parent;
                }
            }
        }

        self.parent.insert(impl_def_id, parent);
        Ok(last_lint)
    }

    /// Insert cached metadata mapping from a child impl back to its parent.
    pub fn record_impl_from_cstore(&mut self,
                                   tcx: TyCtxt<'a, 'gcx, 'tcx>,
                                   parent: DefId,
                                   child: DefId) {
        if self.parent.insert(child, parent).is_some() {
            bug!("When recording an impl from the crate store, information about its parent \
                  was already present.");
        }

        self.children.entry(parent).or_default().insert_blindly(tcx, child);
    }

    /// The parent of a given impl, which is the def id of the trait when the
    /// impl is a "specialization root".
    pub fn parent(&self, child: DefId) -> DefId {
        *self.parent.get(&child).unwrap()
    }
}

/// A node in the specialization graph is either an impl or a trait
/// definition; either can serve as a source of item definitions.
/// There is always exactly one trait definition node: the root.
#[derive(Debug, Copy, Clone)]
pub enum Node {
    Impl(DefId),
    Trait(DefId),
}

impl<'a, 'gcx, 'tcx> Node {
    pub fn is_from_trait(&self) -> bool {
        match *self {
            Node::Trait(..) => true,
            _ => false,
        }
    }

    /// Iterate over the items defined directly by the given (impl or trait) node.
    pub fn items(
        &self,
        tcx: TyCtxt<'a, 'gcx, 'tcx>,
    ) -> impl Iterator<Item = ty::AssociatedItem> + 'a {
        tcx.associated_items(self.def_id())
    }

    pub fn def_id(&self) -> DefId {
        match *self {
            Node::Impl(did) => did,
            Node::Trait(did) => did,
        }
    }
}

pub struct Ancestors {
    trait_def_id: DefId,
    specialization_graph: Lrc<Graph>,
    current_source: Option<Node>,
}

impl Iterator for Ancestors {
    type Item = Node;
    fn next(&mut self) -> Option<Node> {
        let cur = self.current_source.take();
        if let Some(Node::Impl(cur_impl)) = cur {
            let parent = self.specialization_graph.parent(cur_impl);

            self.current_source = if parent == self.trait_def_id {
                Some(Node::Trait(parent))
            } else {
                Some(Node::Impl(parent))
            };
        }
        cur
    }
}

pub struct NodeItem<T> {
    pub node: Node,
    pub item: T,
}

impl<T> NodeItem<T> {
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> NodeItem<U> {
        NodeItem {
            node: self.node,
            item: f(self.item),
        }
    }
}

impl<'a, 'gcx, 'tcx> Ancestors {
    /// Search the items from the given ancestors, returning each definition
    /// with the given name and the given kind.
    #[inline] // FIXME(#35870) Avoid closures being unexported due to impl Trait.
    pub fn defs(
        self,
        tcx: TyCtxt<'a, 'gcx, 'tcx>,
        trait_item_name: Ident,
        trait_item_kind: ty::AssociatedKind,
        trait_def_id: DefId,
    ) -> impl Iterator<Item = NodeItem<ty::AssociatedItem>> + Captures<'gcx> + Captures<'tcx> + 'a {
        self.flat_map(move |node| {
            use ty::AssociatedKind::*;
            node.items(tcx).filter(move |impl_item| match (trait_item_kind, impl_item.kind) {
                | (Const, Const)
                | (Method, Method)
                | (Type, Type)
                | (Type, Existential)
                => tcx.hygienic_eq(impl_item.ident, trait_item_name, trait_def_id),

                | (Const, _)
                | (Method, _)
                | (Type, _)
                | (Existential, _)
                => false,
            }).map(move |item| NodeItem { node: node, item: item })
        })
    }
}

/// Walk up the specialization ancestors of a given impl, starting with that
/// impl itself.
pub fn ancestors(tcx: TyCtxt<'_, '_, '_>,
                 trait_def_id: DefId,
                 start_from_impl: DefId)
                 -> Ancestors {
    let specialization_graph = tcx.specialization_graph_of(trait_def_id);
    Ancestors {
        trait_def_id,
        specialization_graph,
        current_source: Some(Node::Impl(start_from_impl)),
    }
}

impl<'a> HashStable<StableHashingContext<'a>> for Children {
    fn hash_stable<W: StableHasherResult>(&self,
                                          hcx: &mut StableHashingContext<'a>,
                                          hasher: &mut StableHasher<W>) {
        let Children {
            ref nonblanket_impls,
            ref blanket_impls,
        } = *self;

        ich::hash_stable_trait_impls(hcx, hasher, blanket_impls, nonblanket_impls);
    }
}

impl_stable_hash_for!(struct self::Graph {
    parent,
    children
});
