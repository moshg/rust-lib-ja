// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Utility Functions.

use super::{CrateDebugContext};
use super::namespace::item_namespace;

use rustc::hir::def_id::DefId;
use rustc::ty::DefIdTree;

use llvm;
use llvm::debuginfo::{DIScope, DIBuilderRef, DIDescriptor, DIArray};
use common::{CrateContext};

use syntax_pos::{self, Span};
use syntax::ast;

pub fn is_node_local_to_unit(cx: &CrateContext, node_id: ast::NodeId) -> bool
{
    // The is_local_to_unit flag indicates whether a function is local to the
    // current compilation unit (i.e. if it is *static* in the C-sense). The
    // *reachable* set should provide a good approximation of this, as it
    // contains everything that might leak out of the current crate (by being
    // externally visible or by being inlined into something externally
    // visible). It might better to use the `exported_items` set from
    // `driver::CrateAnalysis` in the future, but (atm) this set is not
    // available in the translation pass.
    let def_id = cx.tcx.hir.local_def_id(node_id);
    !cx.tcx.is_exported_symbol(def_id)
}

#[allow(non_snake_case)]
pub fn create_DIArray(builder: DIBuilderRef, arr: &[DIDescriptor]) -> DIArray {
    return unsafe {
        llvm::LLVMRustDIBuilderGetOrCreateArray(builder, arr.as_ptr(), arr.len() as u32)
    };
}

/// Return syntax_pos::Loc corresponding to the beginning of the span
pub fn span_start(cx: &CrateContext, span: Span) -> syntax_pos::Loc {
    cx.sess().codemap().lookup_char_pos(span.lo())
}

#[inline]
pub fn debug_context<'a, 'tcx>(cx: &'a CrateContext<'a, 'tcx>)
                           -> &'a CrateDebugContext<'tcx> {
    cx.dbg_cx.as_ref().unwrap()
}

#[inline]
#[allow(non_snake_case)]
pub fn DIB(cx: &CrateContext) -> DIBuilderRef {
    cx.dbg_cx.as_ref().unwrap().builder
}

pub fn get_namespace_for_item(cx: &CrateContext, def_id: DefId) -> DIScope {
    item_namespace(cx, cx.tcx.parent(def_id)
        .expect("get_namespace_for_item: missing parent?"))
}
