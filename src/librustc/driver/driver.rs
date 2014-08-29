// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.


use back::link;
use driver::session::Session;
use driver::{config, PpMode, PpSourceMode};
use driver::{PpmFlowGraph, PpmExpanded, PpmExpandedIdentified, PpmTyped};
use driver::{PpmIdentified, PpmNormal, PpmSource};
use front;
use lint;
use llvm::{ContextRef, ModuleRef};
use metadata::common::LinkMeta;
use metadata::creader;
use middle::borrowck::{FnPartsWithCFG};
use middle::borrowck;
use middle::borrowck::graphviz as borrowck_dot;
use middle::cfg;
use middle::cfg::graphviz::LabelledCFG;
use middle::{trans, freevars, stability, kind, ty, typeck, reachable};
use middle::dependency_format;
use middle;
use plugin::load::Plugins;
use plugin::registry::Registry;
use plugin;

use util::common::time;
use util::ppaux;
use util::nodemap::{NodeSet};

use graphviz as dot;

use serialize::{json, Encodable};

use std::from_str::FromStr;
use std::io;
use std::io::fs;
use std::io::MemReader;
use std::option;
use syntax::ast;
use syntax::ast_map;
use syntax::ast_map::blocks;
use syntax::ast_map::NodePrinter;
use syntax::attr;
use syntax::attr::{AttrMetaMethods};
use syntax::diagnostics;
use syntax::parse;
use syntax::parse::token;
use syntax::print::{pp, pprust};
use syntax;

pub fn host_triple() -> &'static str {
    // Get the host triple out of the build environment. This ensures that our
    // idea of the host triple is the same as for the set of libraries we've
    // actually built.  We can't just take LLVM's host triple because they
    // normalize all ix86 architectures to i386.
    //
    // Instead of grabbing the host triple (for the current host), we grab (at
    // compile time) the target triple that this rustc is built with and
    // calling that (at runtime) the host triple.
    (option_env!("CFG_COMPILER_HOST_TRIPLE")).
        expect("CFG_COMPILER_HOST_TRIPLE")
}

pub fn compile_input(sess: Session,
                     cfg: ast::CrateConfig,
                     input: &Input,
                     outdir: &Option<Path>,
                     output: &Option<Path>,
                     addl_plugins: Option<Plugins>) {
    // We need nested scopes here, because the intermediate results can keep
    // large chunks of memory alive and we want to free them as soon as
    // possible to keep the peak memory usage low
    let (outputs, trans, sess) = {
        let (outputs, expanded_crate, ast_map, id) = {
            let krate = phase_1_parse_input(&sess, cfg, input);
            if stop_after_phase_1(&sess) { return; }
            let outputs = build_output_filenames(input,
                                                 outdir,
                                                 output,
                                                 krate.attrs.as_slice(),
                                                 &sess);
            let id = link::find_crate_name(Some(&sess), krate.attrs.as_slice(),
                                           input);
            let (expanded_crate, ast_map)
                = match phase_2_configure_and_expand(&sess, krate, id.as_slice(),
                                                     addl_plugins) {
                    None => return,
                    Some(p) => p,
                };

            (outputs, expanded_crate, ast_map, id)
        };
        write_out_deps(&sess, input, &outputs, id.as_slice());

        if stop_after_phase_2(&sess) { return; }

        let analysis = phase_3_run_analysis_passes(sess, &expanded_crate,
                                                   ast_map, id);
        phase_save_analysis(&analysis.ty_cx.sess, &expanded_crate, &analysis, outdir);
        if stop_after_phase_3(&analysis.ty_cx.sess) { return; }
        let (tcx, trans) = phase_4_translate_to_llvm(expanded_crate, analysis);

        // Discard interned strings as they are no longer required.
        token::get_ident_interner().clear();

        (outputs, trans, tcx.sess)
    };
    phase_5_run_llvm_passes(&sess, &trans, &outputs);
    if stop_after_phase_5(&sess) { return; }
    phase_6_link_output(&sess, &trans, &outputs);
}

/**
 * The name used for source code that doesn't originate in a file
 * (e.g. source from stdin or a string)
 */
pub fn anon_src() -> String {
    "<anon>".to_string()
}

pub fn source_name(input: &Input) -> String {
    match *input {
        // FIXME (#9639): This needs to handle non-utf8 paths
        FileInput(ref ifile) => ifile.as_str().unwrap().to_string(),
        StrInput(_) => anon_src()
    }
}

pub enum Input {
    /// Load source from file
    FileInput(Path),
    /// The string is the source
    StrInput(String)
}

impl Input {
    fn filestem(&self) -> String {
        match *self {
            FileInput(ref ifile) => ifile.filestem_str().unwrap().to_string(),
            StrInput(_) => "rust_out".to_string(),
        }
    }
}


pub fn phase_1_parse_input(sess: &Session, cfg: ast::CrateConfig, input: &Input)
    -> ast::Crate {
    let krate = time(sess.time_passes(), "parsing", (), |_| {
        match *input {
            FileInput(ref file) => {
                parse::parse_crate_from_file(&(*file), cfg.clone(), &sess.parse_sess)
            }
            StrInput(ref src) => {
                parse::parse_crate_from_source_str(anon_src().to_string(),
                                                   src.to_string(),
                                                   cfg.clone(),
                                                   &sess.parse_sess)
            }
        }
    });

    if sess.opts.debugging_opts & config::AST_JSON_NOEXPAND != 0 {
        let mut stdout = io::BufferedWriter::new(io::stdout());
        let mut json = json::PrettyEncoder::new(&mut stdout);
        // unwrapping so IoError isn't ignored
        krate.encode(&mut json).unwrap();
    }

    if sess.show_span() {
        front::show_span::run(sess, &krate);
    }

    krate
}

// For continuing compilation after a parsed crate has been
// modified

/// Run the "early phases" of the compiler: initial `cfg` processing,
/// loading compiler plugins (including those from `addl_plugins`),
/// syntax expansion, secondary `cfg` expansion, synthesis of a test
/// harness if one is to be provided and injection of a dependency on the
/// standard library and prelude.
///
/// Returns `None` if we're aborting after handling -W help.
pub fn phase_2_configure_and_expand(sess: &Session,
                                    mut krate: ast::Crate,
                                    crate_name: &str,
                                    addl_plugins: Option<Plugins>)
                                    -> Option<(ast::Crate, syntax::ast_map::Map)> {
    let time_passes = sess.time_passes();

    *sess.crate_types.borrow_mut() =
        collect_crate_types(sess, krate.attrs.as_slice());
    *sess.crate_metadata.borrow_mut() =
        collect_crate_metadata(sess, krate.attrs.as_slice());

    time(time_passes, "gated feature checking", (), |_|
         front::feature_gate::check_crate(sess, &krate));

    krate = time(time_passes, "crate injection", krate, |krate|
                 front::std_inject::maybe_inject_crates_ref(sess, krate));

    // strip before expansion to allow macros to depend on
    // configuration variables e.g/ in
    //
    //   #[macro_escape] #[cfg(foo)]
    //   mod bar { macro_rules! baz!(() => {{}}) }
    //
    // baz! should not use this definition unless foo is enabled.

    krate = time(time_passes, "configuration 1", krate, |krate|
                 front::config::strip_unconfigured_items(krate));

    let mut addl_plugins = Some(addl_plugins);
    let Plugins { macros, registrars }
        = time(time_passes, "plugin loading", (), |_|
               plugin::load::load_plugins(sess, &krate, addl_plugins.take().unwrap()));

    let mut registry = Registry::new(&krate);

    time(time_passes, "plugin registration", (), |_| {
        if sess.features.rustc_diagnostic_macros.get() {
            registry.register_macro("__diagnostic_used",
                diagnostics::plugin::expand_diagnostic_used);
            registry.register_macro("__register_diagnostic",
                diagnostics::plugin::expand_register_diagnostic);
            registry.register_macro("__build_diagnostic_array",
                diagnostics::plugin::expand_build_diagnostic_array);
        }

        for &registrar in registrars.iter() {
            registrar(&mut registry);
        }
    });

    let Registry { syntax_exts, lint_passes, lint_groups, .. } = registry;

    {
        let mut ls = sess.lint_store.borrow_mut();
        for pass in lint_passes.move_iter() {
            ls.register_pass(Some(sess), true, pass);
        }

        for (name, to) in lint_groups.move_iter() {
            ls.register_group(Some(sess), true, name, to);
        }
    }

    // Lint plugins are registered; now we can process command line flags.
    if sess.opts.describe_lints {
        super::describe_lints(&*sess.lint_store.borrow(), true);
        return None;
    }
    sess.lint_store.borrow_mut().process_command_line(sess);

    // Abort if there are errors from lint processing or a plugin registrar.
    sess.abort_if_errors();

    krate = time(time_passes, "expansion", (krate, macros, syntax_exts),
        |(krate, macros, syntax_exts)| {
            // Windows dlls do not have rpaths, so they don't know how to find their
            // dependencies. It's up to us to tell the system where to find all the
            // dependent dlls. Note that this uses cfg!(windows) as opposed to
            // targ_cfg because syntax extensions are always loaded for the host
            // compiler, not for the target.
            if cfg!(windows) {
                sess.host_filesearch().add_dylib_search_paths();
            }
            let cfg = syntax::ext::expand::ExpansionConfig {
                deriving_hash_type_parameter: sess.features.default_type_params.get(),
                crate_name: crate_name.to_string(),
            };
            syntax::ext::expand::expand_crate(&sess.parse_sess,
                                              cfg,
                                              macros,
                                              syntax_exts,
                                              krate)
        }
    );

    // JBC: make CFG processing part of expansion to avoid this problem:

    // strip again, in case expansion added anything with a #[cfg].
    krate = time(time_passes, "configuration 2", krate, |krate|
                 front::config::strip_unconfigured_items(krate));

    krate = time(time_passes, "maybe building test harness", krate, |krate|
                 front::test::modify_for_testing(sess, krate));

    krate = time(time_passes, "prelude injection", krate, |krate|
                 front::std_inject::maybe_inject_prelude(sess, krate));

    let (krate, map) = time(time_passes, "assigning node ids and indexing ast", krate, |krate|
         front::assign_node_ids_and_map::assign_node_ids_and_map(sess, krate));

    if sess.opts.debugging_opts & config::AST_JSON != 0 {
        let mut stdout = io::BufferedWriter::new(io::stdout());
        let mut json = json::PrettyEncoder::new(&mut stdout);
        // unwrapping so IoError isn't ignored
        krate.encode(&mut json).unwrap();
    }

    time(time_passes, "checking that all macro invocations are gone", &krate, |krate|
         syntax::ext::expand::check_for_macros(&sess.parse_sess, krate));

    Some((krate, map))
}

pub struct CrateAnalysis {
    pub exp_map2: middle::resolve::ExportMap2,
    pub exported_items: middle::privacy::ExportedItems,
    pub public_items: middle::privacy::PublicItems,
    pub ty_cx: ty::ctxt,
    pub reachable: NodeSet,
    pub name: String,
}


/// Run the resolution, typechecking, region checking and other
/// miscellaneous analysis passes on the crate. Return various
/// structures carrying the results of the analysis.
pub fn phase_3_run_analysis_passes(sess: Session,
                                   krate: &ast::Crate,
                                   ast_map: syntax::ast_map::Map,
                                   name: String) -> CrateAnalysis {
    let time_passes = sess.time_passes();

    time(time_passes, "external crate/lib resolution", (), |_|
         creader::read_crates(&sess, krate));

    let lang_items = time(time_passes, "language item collection", (), |_|
                          middle::lang_items::collect_language_items(krate, &sess));

    let middle::resolve::CrateMap {
        def_map: def_map,
        exp_map2: exp_map2,
        trait_map: trait_map,
        external_exports: external_exports,
        last_private_map: last_private_map
    } =
        time(time_passes, "resolution", (), |_|
             middle::resolve::resolve_crate(&sess, &lang_items, krate));

    // Discard MTWT tables that aren't required past resolution.
    syntax::ext::mtwt::clear_tables();

    let named_region_map = time(time_passes, "lifetime resolution", (),
                                |_| middle::resolve_lifetime::krate(&sess, krate));

    time(time_passes, "looking for entry point", (),
         |_| middle::entry::find_entry_point(&sess, krate, &ast_map));

    sess.plugin_registrar_fn.set(
        time(time_passes, "looking for plugin registrar", (), |_|
            plugin::build::find_plugin_registrar(
                sess.diagnostic(), krate)));

    let (freevars, capture_modes) =
        time(time_passes, "freevar finding", (), |_|
             freevars::annotate_freevars(&def_map, krate));

    let region_map = time(time_passes, "region resolution", (), |_|
                          middle::region::resolve_crate(&sess, krate));

    time(time_passes, "loop checking", (), |_|
         middle::check_loop::check_crate(&sess, krate));

    let stability_index = time(time_passes, "stability index", (), |_|
                               stability::Index::build(krate));

    let ty_cx = ty::mk_ctxt(sess,
                            def_map,
                            named_region_map,
                            ast_map,
                            freevars,
                            capture_modes,
                            region_map,
                            lang_items,
                            stability_index);

    // passes are timed inside typeck
    typeck::check_crate(&ty_cx, trait_map, krate);

    time(time_passes, "check static items", (), |_|
         middle::check_static::check_crate(&ty_cx, krate));

    // These next two const passes can probably be merged
    time(time_passes, "const marking", (), |_|
         middle::const_eval::process_crate(krate, &ty_cx));

    time(time_passes, "const checking", (), |_|
         middle::check_const::check_crate(krate, &ty_cx));

    let maps = (external_exports, last_private_map);
    let (exported_items, public_items) =
            time(time_passes, "privacy checking", maps, |(a, b)|
                 middle::privacy::check_crate(&ty_cx, &exp_map2, a, b, krate));

    time(time_passes, "intrinsic checking", (), |_|
         middle::intrinsicck::check_crate(&ty_cx, krate));

    time(time_passes, "effect checking", (), |_|
         middle::effect::check_crate(&ty_cx, krate));

    time(time_passes, "match checking", (), |_|
         middle::check_match::check_crate(&ty_cx, krate));

    time(time_passes, "liveness checking", (), |_|
         middle::liveness::check_crate(&ty_cx, krate));

    time(time_passes, "borrow checking", (), |_|
         middle::borrowck::check_crate(&ty_cx, krate));

    time(time_passes, "kind checking", (), |_|
         kind::check_crate(&ty_cx, krate));

    let reachable_map =
        time(time_passes, "reachability checking", (), |_|
             reachable::find_reachable(&ty_cx, &exported_items));

    time(time_passes, "death checking", (), |_| {
        middle::dead::check_crate(&ty_cx,
                                  &exported_items,
                                  &reachable_map,
                                  krate)
    });

    time(time_passes, "lint checking", (), |_|
         lint::check_crate(&ty_cx, krate, &exported_items));

    CrateAnalysis {
        exp_map2: exp_map2,
        ty_cx: ty_cx,
        exported_items: exported_items,
        public_items: public_items,
        reachable: reachable_map,
        name: name,
    }
}

pub fn phase_save_analysis(sess: &Session,
                           krate: &ast::Crate,
                           analysis: &CrateAnalysis,
                           odir: &Option<Path>) {
    if (sess.opts.debugging_opts & config::SAVE_ANALYSIS) == 0 {
        return;
    }
    time(sess.time_passes(), "save analysis", krate, |krate|
         middle::save::process_crate(sess, krate, analysis, odir));
}

pub struct CrateTranslation {
    pub context: ContextRef,
    pub module: ModuleRef,
    pub metadata_module: ModuleRef,
    pub link: LinkMeta,
    pub metadata: Vec<u8>,
    pub reachable: Vec<String>,
    pub crate_formats: dependency_format::Dependencies,
    pub no_builtins: bool,
}

/// Run the translation phase to LLVM, after which the AST and analysis can
/// be discarded.
pub fn phase_4_translate_to_llvm(krate: ast::Crate,
                                 analysis: CrateAnalysis) -> (ty::ctxt, CrateTranslation) {
    let time_passes = analysis.ty_cx.sess.time_passes();

    time(time_passes, "resolving dependency formats", (), |_|
         dependency_format::calculate(&analysis.ty_cx));

    // Option dance to work around the lack of stack once closures.
    time(time_passes, "translation", (krate, analysis), |(krate, analysis)|
         trans::base::trans_crate(krate, analysis))
}

/// Run LLVM itself, producing a bitcode file, assembly file or object file
/// as a side effect.
pub fn phase_5_run_llvm_passes(sess: &Session,
                               trans: &CrateTranslation,
                               outputs: &OutputFilenames) {
    if sess.opts.cg.no_integrated_as {
        let output_type = link::OutputTypeAssembly;

        time(sess.time_passes(), "LLVM passes", (), |_|
            link::write::run_passes(sess, trans, [output_type], outputs));

        link::write::run_assembler(sess, outputs);

        // Remove assembly source, unless --save-temps was specified
        if !sess.opts.cg.save_temps {
            fs::unlink(&outputs.temp_path(link::OutputTypeAssembly)).unwrap();
        }
    } else {
        time(sess.time_passes(), "LLVM passes", (), |_|
            link::write::run_passes(sess,
                                    trans,
                                    sess.opts.output_types.as_slice(),
                                    outputs));
    }
}

/// Run the linker on any artifacts that resulted from the LLVM run.
/// This should produce either a finished executable or library.
pub fn phase_6_link_output(sess: &Session,
                           trans: &CrateTranslation,
                           outputs: &OutputFilenames) {
    time(sess.time_passes(), "linking", (), |_|
         link::link_binary(sess,
                           trans,
                           outputs,
                           trans.link.crate_name.as_slice()));
}

pub fn stop_after_phase_3(sess: &Session) -> bool {
   if sess.opts.no_trans {
        debug!("invoked with --no-trans, returning early from compile_input");
        return true;
    }
    return false;
}

pub fn stop_after_phase_1(sess: &Session) -> bool {
    if sess.opts.parse_only {
        debug!("invoked with --parse-only, returning early from compile_input");
        return true;
    }
    if sess.show_span() {
        return true;
    }
    return sess.opts.debugging_opts & config::AST_JSON_NOEXPAND != 0;
}

pub fn stop_after_phase_2(sess: &Session) -> bool {
    if sess.opts.no_analysis {
        debug!("invoked with --no-analysis, returning early from compile_input");
        return true;
    }
    return sess.opts.debugging_opts & config::AST_JSON != 0;
}

pub fn stop_after_phase_5(sess: &Session) -> bool {
    if !sess.opts.output_types.iter().any(|&i| i == link::OutputTypeExe) {
        debug!("not building executable, returning early from compile_input");
        return true;
    }
    return false;
}

fn write_out_deps(sess: &Session,
                  input: &Input,
                  outputs: &OutputFilenames,
                  id: &str) {

    let mut out_filenames = Vec::new();
    for output_type in sess.opts.output_types.iter() {
        let file = outputs.path(*output_type);
        match *output_type {
            link::OutputTypeExe => {
                for output in sess.crate_types.borrow().iter() {
                    let p = link::filename_for_input(sess, *output,
                                                     id, &file);
                    out_filenames.push(p);
                }
            }
            _ => { out_filenames.push(file); }
        }
    }

    // Write out dependency rules to the dep-info file if requested with
    // --dep-info
    let deps_filename = match sess.opts.write_dependency_info {
        // Use filename from --dep-file argument if given
        (true, Some(ref filename)) => filename.clone(),
        // Use default filename: crate source filename with extension replaced
        // by ".d"
        (true, None) => match *input {
            FileInput(..) => outputs.with_extension("d"),
            StrInput(..) => {
                sess.warn("can not write --dep-info without a filename \
                           when compiling stdin.");
                return
            },
        },
        _ => return,
    };

    let result = (|| {
        // Build a list of files used to compile the output and
        // write Makefile-compatible dependency rules
        let files: Vec<String> = sess.codemap().files.borrow()
                                   .iter().filter(|fmap| fmap.is_real_file())
                                   .map(|fmap| fmap.name.to_string())
                                   .collect();
        let mut file = try!(io::File::create(&deps_filename));
        for path in out_filenames.iter() {
            try!(write!(&mut file as &mut Writer,
                          "{}: {}\n\n", path.display(), files.connect(" ")));
        }
        Ok(())
    })();

    match result {
        Ok(()) => {}
        Err(e) => {
            sess.fatal(format!("error writing dependencies to `{}`: {}",
                               deps_filename.display(), e).as_slice());
        }
    }
}

// This slightly awkward construction is to allow for each PpMode to
// choose whether it needs to do analyses (which can consume the
// Session) and then pass through the session (now attached to the
// analysis results) on to the chosen pretty-printer, along with the
// `&PpAnn` object.
//
// Note that since the `&PrinterSupport` is freshly constructed on each
// call, it would not make sense to try to attach the lifetime of `self`
// to the lifetime of the `&PrinterObject`.
//
// (The `use_once_payload` is working around the current lack of once
// functions in the compiler.)
trait CratePrinter {
    /// Constructs a `PrinterSupport` object and passes it to `f`.
    fn call_with_pp_support<A,B>(&self,
                                 sess: Session,
                                 krate: &ast::Crate,
                                 ast_map: Option<syntax::ast_map::Map>,
                                 id: String,
                                 use_once_payload: B,
                                 f: |&PrinterSupport, B| -> A) -> A;
}

trait SessionCarrier {
    /// Provides a uniform interface for re-extracting a reference to a
    /// `Session` from a value that now owns it.
    fn sess<'a>(&'a self) -> &'a Session;
}

trait AstMapCarrier {
    /// Provides a uniform interface for re-extracting a reference to an
    /// `ast_map::Map` from a value that now owns it.
    fn ast_map<'a>(&'a self) -> Option<&'a ast_map::Map>;
}

trait PrinterSupport : SessionCarrier + AstMapCarrier {
    /// Produces the pretty-print annotation object.
    ///
    /// Usually implemented via `self as &pprust::PpAnn`.
    ///
    /// (Rust does not yet support upcasting from a trait object to
    /// an object for one of its super-traits.)
    fn pp_ann<'a>(&'a self) -> &'a pprust::PpAnn;
}

struct NoAnn {
    sess: Session,
    ast_map: Option<ast_map::Map>,
}

impl PrinterSupport for NoAnn {
    fn pp_ann<'a>(&'a self) -> &'a pprust::PpAnn { self as &pprust::PpAnn }
}

impl SessionCarrier for NoAnn {
    fn sess<'a>(&'a self) -> &'a Session { &self.sess }
}

impl AstMapCarrier for NoAnn {
    fn ast_map<'a>(&'a self) -> Option<&'a ast_map::Map> {
        self.ast_map.as_ref()
    }
}

impl pprust::PpAnn for NoAnn {}

struct IdentifiedAnnotation {
    sess: Session,
    ast_map: Option<ast_map::Map>,
}

impl PrinterSupport for IdentifiedAnnotation {
    fn pp_ann<'a>(&'a self) -> &'a pprust::PpAnn { self as &pprust::PpAnn }
}

impl SessionCarrier for IdentifiedAnnotation {
    fn sess<'a>(&'a self) -> &'a Session { &self.sess }
}

impl AstMapCarrier for IdentifiedAnnotation {
    fn ast_map<'a>(&'a self) -> Option<&'a ast_map::Map> {
        self.ast_map.as_ref()
    }
}

impl pprust::PpAnn for IdentifiedAnnotation {
    fn pre(&self,
           s: &mut pprust::State,
           node: pprust::AnnNode) -> io::IoResult<()> {
        match node {
            pprust::NodeExpr(_) => s.popen(),
            _ => Ok(())
        }
    }
    fn post(&self,
            s: &mut pprust::State,
            node: pprust::AnnNode) -> io::IoResult<()> {
        match node {
            pprust::NodeItem(item) => {
                try!(pp::space(&mut s.s));
                s.synth_comment(item.id.to_string())
            }
            pprust::NodeBlock(blk) => {
                try!(pp::space(&mut s.s));
                s.synth_comment(format!("block {}", blk.id))
            }
            pprust::NodeExpr(expr) => {
                try!(pp::space(&mut s.s));
                try!(s.synth_comment(expr.id.to_string()));
                s.pclose()
            }
            pprust::NodePat(pat) => {
                try!(pp::space(&mut s.s));
                s.synth_comment(format!("pat {}", pat.id))
            }
        }
    }
}

struct TypedAnnotation {
    analysis: CrateAnalysis,
}

impl PrinterSupport for TypedAnnotation {
    fn pp_ann<'a>(&'a self) -> &'a pprust::PpAnn { self as &pprust::PpAnn }
}

impl SessionCarrier for TypedAnnotation {
    fn sess<'a>(&'a self) -> &'a Session { &self.analysis.ty_cx.sess }
}

impl AstMapCarrier for TypedAnnotation {
    fn ast_map<'a>(&'a self) -> Option<&'a ast_map::Map> {
        Some(&self.analysis.ty_cx.map)
    }
}

impl pprust::PpAnn for TypedAnnotation {
    fn pre(&self,
           s: &mut pprust::State,
           node: pprust::AnnNode) -> io::IoResult<()> {
        match node {
            pprust::NodeExpr(_) => s.popen(),
            _ => Ok(())
        }
    }
    fn post(&self,
            s: &mut pprust::State,
            node: pprust::AnnNode) -> io::IoResult<()> {
        let tcx = &self.analysis.ty_cx;
        match node {
            pprust::NodeExpr(expr) => {
                try!(pp::space(&mut s.s));
                try!(pp::word(&mut s.s, "as"));
                try!(pp::space(&mut s.s));
                try!(pp::word(&mut s.s,
                              ppaux::ty_to_string(
                                  tcx,
                                  ty::expr_ty(tcx, expr)).as_slice()));
                s.pclose()
            }
            _ => Ok(())
        }
    }
}

fn gather_flowgraph_variants(sess: &Session) -> Vec<borrowck_dot::Variant> {
    let print_loans   = config::FLOWGRAPH_PRINT_LOANS;
    let print_moves   = config::FLOWGRAPH_PRINT_MOVES;
    let print_assigns = config::FLOWGRAPH_PRINT_ASSIGNS;
    let print_all     = config::FLOWGRAPH_PRINT_ALL;
    let opt = |print_which| sess.debugging_opt(print_which);
    let mut variants = Vec::new();
    if opt(print_all) || opt(print_loans) {
        variants.push(borrowck_dot::Loans);
    }
    if opt(print_all) || opt(print_moves) {
        variants.push(borrowck_dot::Moves);
    }
    if opt(print_all) || opt(print_assigns) {
        variants.push(borrowck_dot::Assigns);
    }
    variants
}

#[deriving(Clone, Show)]
pub enum UserIdentifiedItem {
    ItemViaNode(ast::NodeId),
    ItemViaPath(Vec<String>),
}

impl FromStr for UserIdentifiedItem {
    fn from_str(s: &str) -> Option<UserIdentifiedItem> {
        let extract_path_parts = || {
            let v : Vec<_> = s.split_str("::")
                .map(|x|x.to_string())
                .collect();
            Some(ItemViaPath(v))
        };

        from_str(s).map(ItemViaNode).or_else(extract_path_parts)
    }
}

enum NodesMatchingUII<'a> {
    NodesMatchingDirect(option::Item<ast::NodeId>),
    NodesMatchingSuffix(ast_map::NodesMatchingSuffix<'a, String>),
}

impl<'a> Iterator<ast::NodeId> for NodesMatchingUII<'a> {
    fn next(&mut self) -> Option<ast::NodeId> {
        match self {
            &NodesMatchingDirect(ref mut iter) => iter.next(),
            &NodesMatchingSuffix(ref mut iter) => iter.next(),
        }
    }
}

impl UserIdentifiedItem {
    fn reconstructed_input(&self) -> String {
        match *self {
            ItemViaNode(node_id) => node_id.to_string(),
            ItemViaPath(ref parts) => parts.connect("::"),
        }
    }

    fn all_matching_node_ids<'a>(&'a self, map: &'a ast_map::Map) -> NodesMatchingUII<'a> {
        match *self {
            ItemViaNode(node_id) =>
                NodesMatchingDirect(Some(node_id).move_iter()),
            ItemViaPath(ref parts) =>
                NodesMatchingSuffix(map.nodes_matching_suffix(parts.as_slice())),
        }
    }

    fn to_one_node_id(self, user_option: &str, sess: &Session, map: &ast_map::Map) -> ast::NodeId {
        let fail_because = |is_wrong_because| -> ast::NodeId {
            let message =
                format!("{:s} needs NodeId (int) or unique \
                         path suffix (b::c::d); got {:s}, which {:s}",
                        user_option,
                        self.reconstructed_input(),
                        is_wrong_because);
            sess.fatal(message.as_slice())
        };

        let mut saw_node = ast::DUMMY_NODE_ID;
        let mut seen = 0u;
        for node in self.all_matching_node_ids(map) {
            saw_node = node;
            seen += 1;
            if seen > 1 {
                fail_because("does not resolve uniquely");
            }
        }
        if seen == 0 {
            fail_because("does not resolve to any item");
        }

        assert!(seen == 1);
        return saw_node;
    }
}

impl CratePrinter for PpSourceMode {
    fn call_with_pp_support<A,B>(&self,
                                 sess: Session,
                                 krate: &ast::Crate,
                                 ast_map: Option<syntax::ast_map::Map>,
                                 id: String,
                                 payload: B,
                                 f: |&PrinterSupport, B| -> A) -> A {
        match *self {
            PpmNormal | PpmExpanded => {
                let annotation = NoAnn { sess: sess, ast_map: ast_map };
                f(&annotation, payload)
            }

            PpmIdentified | PpmExpandedIdentified => {
                let annotation = IdentifiedAnnotation { sess: sess, ast_map: ast_map };
                f(&annotation, payload)
            }
            PpmTyped => {
                let ast_map = ast_map.expect("--pretty=typed missing ast_map");
                let analysis = phase_3_run_analysis_passes(sess, krate, ast_map, id);
                let annotation = TypedAnnotation { analysis: analysis };
                f(&annotation, payload)
            }
        }
    }
}

fn needs_ast_map(ppm: &PpMode, opt_uii: &Option<UserIdentifiedItem>) -> bool {
    match *ppm {
        PpmSource(PpmNormal) |
        PpmSource(PpmIdentified) => opt_uii.is_some(),

        PpmSource(PpmExpanded) |
        PpmSource(PpmExpandedIdentified) |
        PpmSource(PpmTyped) |
        PpmFlowGraph => true
    }
}

fn needs_expansion(ppm: &PpMode) -> bool {
    match *ppm {
        PpmSource(PpmNormal) |
        PpmSource(PpmIdentified) => false,

        PpmSource(PpmExpanded) |
        PpmSource(PpmExpandedIdentified) |
        PpmSource(PpmTyped) |
        PpmFlowGraph => true
    }
}
pub fn pretty_print_input(sess: Session,
                          cfg: ast::CrateConfig,
                          input: &Input,
                          ppm: PpMode,
                          opt_uii: Option<UserIdentifiedItem>,
                          ofile: Option<Path>) {
    let krate = phase_1_parse_input(&sess, cfg, input);
    let id = link::find_crate_name(Some(&sess), krate.attrs.as_slice(), input);

    let is_expanded = needs_expansion(&ppm);
    let (krate, ast_map) = if needs_ast_map(&ppm, &opt_uii) {
        let k = phase_2_configure_and_expand(&sess, krate, id.as_slice(), None);
        let (krate, ast_map) = match k {
            None => return,
            Some(p) => p,
        };
        (krate, Some(ast_map))
    } else {
        (krate, None)
    };

    let src_name = source_name(input);
    let src = Vec::from_slice(sess.codemap()
                                  .get_filemap(src_name.as_slice())
                                  .src
                                  .as_bytes());
    let mut rdr = MemReader::new(src);

    let out = match ofile {
        None => box io::stdout() as Box<Writer+'static>,
        Some(p) => {
            let r = io::File::create(&p);
            match r {
                Ok(w) => box w as Box<Writer+'static>,
                Err(e) => fail!("print-print failed to open {} due to {}",
                                p.display(), e),
            }
        }
    };

    match (ppm, opt_uii) {
        (PpmSource(s), None) =>
            s.call_with_pp_support(
                sess, &krate, ast_map, id, out, |annotation, out| {
                    debug!("pretty printing source code {}", s);
                    let sess = annotation.sess();
                    pprust::print_crate(sess.codemap(),
                                        sess.diagnostic(),
                                        &krate,
                                        src_name.to_string(),
                                        &mut rdr,
                                        out,
                                        annotation.pp_ann(),
                                        is_expanded)
                }),

        (PpmSource(s), Some(uii)) =>
            s.call_with_pp_support(
                sess, &krate, ast_map, id, (out,uii), |annotation, (out,uii)| {
                    debug!("pretty printing source code {}", s);
                    let sess = annotation.sess();
                    let ast_map = annotation.ast_map()
                        .expect("--pretty missing ast_map");
                    let mut pp_state =
                        pprust::State::new_from_input(sess.codemap(),
                                                      sess.diagnostic(),
                                                      src_name.to_string(),
                                                      &mut rdr,
                                                      out,
                                                      annotation.pp_ann(),
                                                      is_expanded);
                    for node_id in uii.all_matching_node_ids(ast_map) {
                        let node = ast_map.get(node_id);
                        try!(pp_state.print_node(&node));
                        try!(pp::space(&mut pp_state.s));
                        try!(pp_state.synth_comment(ast_map.path_to_string(node_id)));
                        try!(pp::hardbreak(&mut pp_state.s));
                    }
                    pp::eof(&mut pp_state.s)
                }),

        (PpmFlowGraph, opt_uii) => {
            debug!("pretty printing flow graph for {}", opt_uii);
            let uii = opt_uii.unwrap_or_else(|| {
                sess.fatal(format!("`pretty flowgraph=..` needs NodeId (int) or
                                     unique path suffix (b::c::d)").as_slice())

            });
            let ast_map = ast_map.expect("--pretty flowgraph missing ast_map");
            let nodeid = uii.to_one_node_id("--pretty", &sess, &ast_map);

            let node = ast_map.find(nodeid).unwrap_or_else(|| {
                sess.fatal(format!("--pretty flowgraph couldn't find id: {}",
                                   nodeid).as_slice())
            });

            let code = blocks::Code::from_node(node);
            match code {
                Some(code) => {
                    let variants = gather_flowgraph_variants(&sess);
                    let analysis = phase_3_run_analysis_passes(sess, &krate,
                                                               ast_map, id);
                    print_flowgraph(variants, analysis, code, out)
                }
                None => {
                    let message = format!("--pretty=flowgraph needs \
                                           block, fn, or method; got {:?}",
                                          node);

                    // point to what was found, if there's an
                    // accessible span.
                    match ast_map.opt_span(nodeid) {
                        Some(sp) => sess.span_fatal(sp, message.as_slice()),
                        None => sess.fatal(message.as_slice())
                    }
                }
            }
        }
    }.unwrap()
}

fn print_flowgraph<W:io::Writer>(variants: Vec<borrowck_dot::Variant>,
                                 analysis: CrateAnalysis,
                                 code: blocks::Code,
                                 mut out: W) -> io::IoResult<()> {
    let ty_cx = &analysis.ty_cx;
    let cfg = match code {
        blocks::BlockCode(block) => cfg::CFG::new(ty_cx, &*block),
        blocks::FnLikeCode(fn_like) => cfg::CFG::new(ty_cx, &*fn_like.body()),
    };
    debug!("cfg: {:?}", cfg);

    match code {
        _ if variants.len() == 0 => {
            let lcfg = LabelledCFG {
                ast_map: &ty_cx.map,
                cfg: &cfg,
                name: format!("node_{}", code.id()),
            };
            let r = dot::render(&lcfg, &mut out);
            return expand_err_details(r);
        }
        blocks::BlockCode(_) => {
            ty_cx.sess.err("--pretty flowgraph with -Z flowgraph-print \
                            annotations requires fn-like node id.");
            return Ok(())
        }
        blocks::FnLikeCode(fn_like) => {
            let fn_parts = FnPartsWithCFG::from_fn_like(&fn_like, &cfg);
            let (bccx, analysis_data) =
                borrowck::build_borrowck_dataflow_data_for_fn(ty_cx, fn_parts);

            let lcfg = LabelledCFG {
                ast_map: &ty_cx.map,
                cfg: &cfg,
                name: format!("node_{}", code.id()),
            };
            let lcfg = borrowck_dot::DataflowLabeller {
                inner: lcfg,
                variants: variants,
                borrowck_ctxt: &bccx,
                analysis_data: &analysis_data,
            };
            let r = dot::render(&lcfg, &mut out);
            return expand_err_details(r);
        }
    }

    fn expand_err_details(r: io::IoResult<()>) -> io::IoResult<()> {
        r.map_err(|ioerr| {
            let orig_detail = ioerr.detail.clone();
            let m = "graphviz::render failed";
            io::IoError {
                detail: Some(match orig_detail {
                    None => m.into_string(),
                    Some(d) => format!("{}: {}", m, d)
                }),
                ..ioerr
            }
        })
    }
}

pub fn collect_crate_types(session: &Session,
                           attrs: &[ast::Attribute]) -> Vec<config::CrateType> {
    // Unconditionally collect crate types from attributes to make them used
    let attr_types: Vec<config::CrateType> = attrs.iter().filter_map(|a| {
        if a.check_name("crate_type") {
            match a.value_str() {
                Some(ref n) if n.equiv(&("rlib")) => {
                    Some(config::CrateTypeRlib)
                }
                Some(ref n) if n.equiv(&("dylib")) => {
                    Some(config::CrateTypeDylib)
                }
                Some(ref n) if n.equiv(&("lib")) => {
                    Some(config::default_lib_output())
                }
                Some(ref n) if n.equiv(&("staticlib")) => {
                    Some(config::CrateTypeStaticlib)
                }
                Some(ref n) if n.equiv(&("bin")) => Some(config::CrateTypeExecutable),
                Some(_) => {
                    session.add_lint(lint::builtin::UNKNOWN_CRATE_TYPE,
                                     ast::CRATE_NODE_ID,
                                     a.span,
                                     "invalid `crate_type` \
                                      value".to_string());
                    None
                }
                _ => {
                    session.add_lint(lint::builtin::UNKNOWN_CRATE_TYPE,
                                     ast::CRATE_NODE_ID,
                                     a.span,
                                     "`crate_type` requires a \
                                      value".to_string());
                    None
                }
            }
        } else {
            None
        }
    }).collect();

    // If we're generating a test executable, then ignore all other output
    // styles at all other locations
    if session.opts.test {
        return vec!(config::CrateTypeExecutable)
    }

    // Only check command line flags if present. If no types are specified by
    // command line, then reuse the empty `base` Vec to hold the types that
    // will be found in crate attributes.
    let mut base = session.opts.crate_types.clone();
    if base.len() == 0 {
        base.extend(attr_types.move_iter());
        if base.len() == 0 {
            base.push(link::default_output_for_target(session));
        }
        base.as_mut_slice().sort();
        base.dedup();
    }

    base.move_iter().filter(|crate_type| {
        let res = !link::invalid_output_for_target(session, *crate_type);

        if !res {
            session.warn(format!("dropping unsupported crate type `{}` \
                                   for target os `{}`",
                                 *crate_type, session.targ_cfg.os).as_slice());
        }

        res
    }).collect()
}

pub fn collect_crate_metadata(session: &Session,
                              _attrs: &[ast::Attribute]) -> Vec<String> {
    session.opts.cg.metadata.clone()
}

pub struct OutputFilenames {
    pub out_directory: Path,
    pub out_filestem: String,
    pub single_output_file: Option<Path>,
    extra: String,
}

impl OutputFilenames {
    pub fn path(&self, flavor: link::OutputType) -> Path {
        match self.single_output_file {
            Some(ref path) => return path.clone(),
            None => {}
        }
        self.temp_path(flavor)
    }

    pub fn temp_path(&self, flavor: link::OutputType) -> Path {
        let base = self.out_directory.join(self.filestem());
        match flavor {
            link::OutputTypeBitcode => base.with_extension("bc"),
            link::OutputTypeAssembly => base.with_extension("s"),
            link::OutputTypeLlvmAssembly => base.with_extension("ll"),
            link::OutputTypeObject => base.with_extension("o"),
            link::OutputTypeExe => base,
        }
    }

    pub fn with_extension(&self, extension: &str) -> Path {
        self.out_directory.join(self.filestem()).with_extension(extension)
    }

    fn filestem(&self) -> String {
        format!("{}{}", self.out_filestem, self.extra)
    }
}

pub fn build_output_filenames(input: &Input,
                              odir: &Option<Path>,
                              ofile: &Option<Path>,
                              attrs: &[ast::Attribute],
                              sess: &Session)
                           -> OutputFilenames {
    match *ofile {
        None => {
            // "-" as input file will cause the parser to read from stdin so we
            // have to make up a name
            // We want to toss everything after the final '.'
            let dirpath = match *odir {
                Some(ref d) => d.clone(),
                None => Path::new(".")
            };

            // If a crate name is present, we use it as the link name
            let stem = sess.opts.crate_name.clone().or_else(|| {
                attr::find_crate_name(attrs).map(|n| n.get().to_string())
            }).or_else(|| {
                // NB: this clause can be removed once #[crate_id] is no longer
                // deprecated.
                //
                // Also note that this will be warned about later so we don't
                // warn about it here.
                use syntax::crateid::CrateId;
                attrs.iter().find(|at| at.check_name("crate_id"))
                     .and_then(|at| at.value_str())
                     .and_then(|s| from_str::<CrateId>(s.get()))
                     .map(|id| id.name)
            }).unwrap_or(input.filestem());

            OutputFilenames {
                out_directory: dirpath,
                out_filestem: stem,
                single_output_file: None,
                extra: sess.opts.cg.extra_filename.clone(),
            }
        }

        Some(ref out_file) => {
            let ofile = if sess.opts.output_types.len() > 1 {
                sess.warn("ignoring specified output filename because multiple \
                           outputs were requested");
                None
            } else {
                Some(out_file.clone())
            };
            if *odir != None {
                sess.warn("ignoring --out-dir flag due to -o flag.");
            }
            OutputFilenames {
                out_directory: out_file.dir_path(),
                out_filestem: out_file.filestem_str().unwrap().to_string(),
                single_output_file: ofile,
                extra: sess.opts.cg.extra_filename.clone(),
            }
        }
    }
}
