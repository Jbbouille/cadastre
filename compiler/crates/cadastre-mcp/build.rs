//! Gives the documentation of the language to the crate (`docs/*.md` in `OUT_DIR`): read in place
//! in a clone of the repository, from `vendor/` in a package of crates.io, where
//! `compiler/vendor.sh` copies it.

use std::path::{Path, PathBuf};

fn main() {
    let root: PathBuf = [env!("CARGO_MANIFEST_DIR"), "..", "..", ".."].iter().collect();
    let docs = if root.join("grammar").join("grammar.js").is_file() {
        root.join("docs")
    } else {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("vendor").join("docs")
    };
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("set by cargo"));
    for name in ["reference.md", "views.md", "tutorial.md"] {
        std::fs::copy(docs.join(name), out.join(name)).expect("the documentation of the language");
        println!("cargo:rerun-if-changed={}", docs.join(name).display());
    }
}
