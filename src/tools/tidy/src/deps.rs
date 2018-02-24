// Copyright 2016 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Check license of third-party deps by inspecting src/vendor

use std::collections::HashSet;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::process::Command;

use serde_json;

static LICENSES: &'static [&'static str] = &[
    "MIT/Apache-2.0",
    "MIT / Apache-2.0",
    "Apache-2.0/MIT",
    "Apache-2.0 / MIT",
    "MIT OR Apache-2.0",
    "MIT",
    "Unlicense/MIT",
];

// These are exceptions to Rust's permissive licensing policy, and
// should be considered bugs. Exceptions are only allowed in Rust
// tooling. It is _crucial_ that no exception crates be dependencies
// of the Rust runtime (std / test).
static EXCEPTIONS: &'static [&'static str] = &[
    "mdbook",             // MPL2, mdbook
    "openssl",            // BSD+advertising clause, cargo, mdbook
    "pest",               // MPL2, mdbook via handlebars
    "thread-id",          // Apache-2.0, mdbook
    "toml-query",         // MPL-2.0, mdbook
    "is-match",           // MPL-2.0, mdbook
    "cssparser",          // MPL-2.0, rustdoc
    "smallvec",           // MPL-2.0, rustdoc
    "fuchsia-zircon-sys", // BSD-3-Clause, rustdoc, rustc, cargo
    "fuchsia-zircon",     // BSD-3-Clause, rustdoc, rustc, cargo (jobserver & tempdir)
    "cssparser-macros",   // MPL-2.0, rustdoc
    "selectors",          // MPL-2.0, rustdoc
    "clippy_lints",       // MPL-2.0 rls
];

// Whitelist of crates rustc is allowed to depend on. Avoid adding to the list if possible.
static WHITELIST: &'static [(&'static str, &'static str)] = &[];

// Some types for Serde to deserialize the output of `cargo metadata` to...

#[derive(Deserialize)]
struct Output {
    packages: Vec<Package>,

    // Not used, but needed to not confuse serde :P
    #[allow(dead_code)] resolve: Resolve,
}

#[derive(Deserialize)]
struct Package {
    name: String,
    version: String,

    // Not used, but needed to not confuse serde :P
    #[allow(dead_code)] id: String,
    #[allow(dead_code)] source: Option<String>,
    #[allow(dead_code)] manifest_path: String,
}

// Not used, but needed to not confuse serde :P
#[allow(dead_code)]
#[derive(Deserialize)]
struct Resolve {
    nodes: Vec<ResolveNode>,
}

// Not used, but needed to not confuse serde :P
#[allow(dead_code)]
#[derive(Deserialize)]
struct ResolveNode {
    id: String,
    dependencies: Vec<String>,
}

/// Checks the dependency at the given path. Changes `bad` to `true` if a check failed.
///
/// Specifically, this checks that the license is correct.
pub fn check(path: &Path, bad: &mut bool) {
    // Check licences
    let path = path.join("vendor");
    assert!(path.exists(), "vendor directory missing");
    let mut saw_dir = false;
    for dir in t!(path.read_dir()) {
        saw_dir = true;
        let dir = t!(dir);

        // skip our exceptions
        if EXCEPTIONS.iter().any(|exception| {
            dir.path()
                .to_str()
                .unwrap()
                .contains(&format!("src/vendor/{}", exception))
        }) {
            continue;
        }

        let toml = dir.path().join("Cargo.toml");
        *bad = *bad || !check_license(&toml);
    }
    assert!(saw_dir, "no vendored source");
}

/// Checks the dependency at the given path. Changes `bad` to `true` if a check failed.
///
/// Specifically, this checks that the dependencies are on the whitelist.
pub fn check_whitelist(path: &Path, bad: &mut bool) {
    // Check dependencies
    let deps: HashSet<_> = get_deps(&path)
        .into_iter()
        .map(|Package { name, version, .. }| (name, version))
        .collect();
    let whitelist: HashSet<(String, String)> = WHITELIST
        .iter()
        .map(|&(n, v)| (n.to_owned(), v.to_owned()))
        .collect();

    // Dependencies not in the whitelist
    let mut unapproved: Vec<_> = deps.difference(&whitelist).collect();

    // For ease of reading
    unapproved.sort();

    if unapproved.len() > 0 {
        println!("Dependencies not on the whitelist:");
        for dep in unapproved {
            println!("* {} {}", dep.0, dep.1); // name version
        }
        *bad = true;
    }
}

fn check_license(path: &Path) -> bool {
    if !path.exists() {
        panic!("{} does not exist", path.display());
    }
    let mut contents = String::new();
    t!(t!(File::open(path)).read_to_string(&mut contents));

    let mut found_license = false;
    for line in contents.lines() {
        if !line.starts_with("license") {
            continue;
        }
        let license = extract_license(line);
        if !LICENSES.contains(&&*license) {
            println!("invalid license {} in {}", license, path.display());
            return false;
        }
        found_license = true;
        break;
    }
    if !found_license {
        println!("no license in {}", path.display());
        return false;
    }

    true
}

fn extract_license(line: &str) -> String {
    let first_quote = line.find('"');
    let last_quote = line.rfind('"');
    if let (Some(f), Some(l)) = (first_quote, last_quote) {
        let license = &line[f + 1..l];
        license.into()
    } else {
        "bad-license-parse".into()
    }
}

/// Get the dependencies of the crate at the given path using `cargo metadata`.
fn get_deps(path: &Path) -> Vec<Package> {
    // Run `cargo metadata` to get the set of dependencies
    let output = Command::new("cargo")
        .arg("metadata")
        .arg("--format-version")
        .arg("1")
        .arg("--manifest-path")
        .arg(path.join("Cargo.toml"))
        .output()
        .expect("Unable to run `cargo metadata`")
        .stdout;
    let output = String::from_utf8_lossy(&output);
    let output: Output = serde_json::from_str(&output).unwrap();

    output.packages
}
