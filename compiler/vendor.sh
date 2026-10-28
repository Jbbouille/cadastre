#!/bin/sh
# Copies the files of the repository that the crates embed into their `vendor/` directory, so that
# their packages build outside the repository (crates.io): the grammar and its highlighting query
# for cadastre-core, the documentation of the language for cadastre-mcp. Run before `cargo publish`;
# a clone of the repository reads them in place, and `vendor/` is ignored by Git.
set -e
cd "$(dirname "$0")"
core=crates/cadastre-core/vendor
mcp=crates/cadastre-mcp/vendor
rm -rf "$core" "$mcp"
mkdir -p "$core/grammar/src/tree_sitter" "$core/grammar/queries" "$mcp/docs"
cp ../grammar/src/parser.c "$core/grammar/src/"
cp ../grammar/src/tree_sitter/*.h "$core/grammar/src/tree_sitter/"
cp ../grammar/queries/highlights.scm "$core/grammar/queries/"
cp ../docs/reference.md ../docs/views.md ../docs/tutorial.md "$mcp/docs/"
