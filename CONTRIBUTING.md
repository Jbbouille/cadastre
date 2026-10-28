# Contributing

How to build Cadastre, check a change before committing it, and publish a release. The layout of the repository is in the [README](README.md), the design in [docs/design.md](docs/design.md).

## Building

### Grammar

Requires Node.js and a C compiler (the Tree-sitter CLI compiles the generated parser).

```bash
cd grammar
npm install
npx tree-sitter generate
npx tree-sitter test
```

`src/parser.c` and `src/grammar.json` are generated from `grammar.js` and committed: regenerate them with every change of the grammar.

### Compiler

Requires Rust (see `compiler/rust-toolchain.toml`) and a C compiler for the generated parser (MSVC Build Tools on Windows, gcc or clang elsewhere).

```bash
cd compiler
cargo test
cargo run -p cadastre -- check ../examples/reference
cargo run -p cadastre -- build ../examples/reference
cargo install --path crates/cadastre-cli
```

SVG renderings require [Graphviz](https://graphviz.org/) (`dot` on the `PATH`); without it, `cadastre build` writes the DOT source instead.

### WebAssembly and the playground

See [web/README.md](web/README.md): the module, its JavaScript interface and the browser playground.

### VS Code extension

See [editors/vscode/README.md](editors/vscode/README.md#development).

```bash
cd editors/vscode
npm install
npm test
npm run package     # cadastre-0.1.0.vsix
```

## Before committing

The CI runs these checks on every push and pull request; run them locally first:

```bash
cd compiler
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

- **Rust style**: `cargo fmt`, with [`compiler/rustfmt.toml`](compiler/rustfmt.toml) (120 columns).
- **The reference**: [`docs/reference.md`](docs/reference.md) is generated from the grammar, the schema and the vocabulary of plans, and `cargo test` fails when it is out of date. Regenerate it after changing any of them:

  ```bash
  UPDATE_REFERENCE=1 cargo test -p cadastre-core reference
  ```

- **The grammar**: after a change of `grammar.js`, regenerate the parser and run its tests (above); the CI checks that the committed parser matches.
- **The examples** must stay valid and formatted: `cadastre check` and `cadastre fmt --check` on each folder of `examples/`.
- **Documentation**: the [guide](docs/guide.md) explains, the reference lists; a new keyword, option or kind appears in both.
- Code, comments, commit messages and documentation are in English.

## Continuous integration

[GitHub Actions](.github/workflows) check every push and pull request: the generated parser matches `grammar.js` and the grammar tests pass; `cargo fmt --check`, `cargo clippy` without warnings and `cargo test` (reference included); the examples are valid and formatted; the WebAssembly module builds with clang; the VS Code extension compiles and its tests pass.

## Releases

A version tag publishes a release. Before tagging, move the entries of `CHANGELOG.md` from *Unreleased* to the version, with what a project must change to upgrade. The tag must match the version of `compiler/Cargo.toml` (the workspace, and the versions its crates require of one another in `[workspace.dependencies]`) and of `editors/vscode/package.json`:

```bash
git tag v0.1.0
git push origin v0.1.0
```

The release holds the `cadastre` binaries for Linux (static), macOS (Apple silicon and Intel) and Windows, the VS Code extension of each platform with its binary, the WebAssembly module with `cadastre.js`, and their checksums. Then, each one once it is set up in the repository:

| Where | Set up | What |
|---|---|---|
| crates.io | the secret `CARGO_REGISTRY_TOKEN` | `cadastre-core`, `cadastre-project`, `cadastre-lsp`, `cadastre-mcp` and `cadastre` (the command: `cargo install cadastre`) |
| Visual Studio Marketplace | a Trusted Publishing policy of the publisher `cadastre` for `release.yml`, and the variable `MARKETPLACE` set to `true` | the extension of each platform (no token: OIDC) |
| Open VSX | a trusted publisher of the namespace `cadastre` for `release.yml`, and the variable `OPENVSX` set to `true` | the same, for VSCodium, Cursor and the other editors that read Open VSX |

The crates embed files of the repository outside their directory (the grammar for `cadastre-core`, the documentation of the language for `cadastre-mcp`): a clone reads them in place, a package from a copy in `vendor/` that `compiler/vendor.sh` makes before publishing. To check the packages without publishing them:

```bash
sh compiler/vendor.sh
cd compiler && cargo publish --dry-run --workspace --exclude cadastre-wasm --allow-dirty
```

The playground is deployed to GitHub Pages from `master` once the repository variable `PAGES` is set to `true` (with Pages enabled, source *GitHub Actions*).
