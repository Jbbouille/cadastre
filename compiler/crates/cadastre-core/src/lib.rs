//! Core of the Cadastre compiler.
//!
//! No system access (files, processes): the core takes texts and returns
//! diagnostics and renderings, so that it runs natively as well as in WebAssembly.

pub mod anonymize;
pub mod api;
pub mod check;
pub mod complete;
pub mod csv;
pub mod declare;
pub mod diagnostic;
pub mod export;
pub mod format;
pub mod highlight;
pub mod i18n;
pub mod impact;
pub mod inventory;
pub mod lower;
pub mod net;
pub mod outline;
pub mod path;
pub mod place;
pub mod plan;
pub mod process;
pub mod project;
pub mod query;
#[cfg(test)]
mod reference;
pub mod registry;
pub mod rename;
pub mod render;
pub mod resolve;
pub mod rules;
pub mod schedule;
pub mod schema;
pub mod site;
pub mod source;
pub mod spdx;
pub mod suggest;
pub mod syntax;
pub mod terraform;
pub mod view;
pub mod vocabulary;

/// The build of the compiler, its version and commit, after a marker that a playground's
/// WebAssembly module is searched for: `cadastre-build:0.1.0 (a1b2c3d)`.
pub const BUILD_MARKED: &str =
    concat!("cadastre-build:", env!("CARGO_PKG_VERSION"), " (", env!("CADASTRE_COMMIT"), ")");

/// The version and commit of the compiler: `0.1.0 (a1b2c3d)`.
pub fn version() -> &'static str {
    &BUILD_MARKED["cadastre-build:".len()..]
}

pub use check::Options;
pub use diagnostic::{Diagnostic, Severity, Summary};
pub use i18n::Language;
pub use project::{
    Compilation, Module, Output, build, build_with, compile, compile_modules, compile_with, drawio, xlsx,
};
pub use source::SourceFile;
