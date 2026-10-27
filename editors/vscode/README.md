# Cadastre for VS Code

Language support for [Cadastre](../../README.md) registries (`.ca`), plans (`.cav`) and checks (`.cac`):

- **Syntax highlighting**, immediately, from a TextMate grammar ([syntaxes/cadastre.tmLanguage.json](syntaxes/cadastre.tmLanguage.json)).
- **Semantic highlighting** from the language server, computed with the Tree-sitter grammar and its [`highlights.scm`](../../grammar/queries/highlights.scm): the same coloring as the playground.
- **Diagnostics** for the whole project as you type: errors, warnings and incomplete elements, as `cadastre check` reports them.
- **Outline** (and breadcrumbs, `Ctrl+Shift+O`): resources nested as written, links, views and style blocks.
- **Go to definition** (`F12`, `Ctrl+click`) on any reference: `on:`, link ends, `carries`, names in plans such as `follow(data, from: app)`. Names are resolved like the compiler does (suffixes, scope by system, expected kind); an ambiguous name lists its candidates.
- **Formatting** (*Format Document*, `Shift+Alt+F`, or on save with `editor.formatOnSave`): the canonical form of `cadastre fmt`: one statement per line, short blocks on one line and long ones one item per line, canonical indentation and spacing, links, single-line declarations and style rules aligned in columns; comments stay in place.
- **Find references** (`Shift+F12`, `Alt+Shift+F12`) of a resource: its declaration and every name that designates it, in the registry and in the plans (link ends, `on:`, `follow`, names in `where` filters such as `name in [datacore01]`); the occurrences of the name under the cursor are highlighted in the file.
- **Rename** (`F2`) a resource, on its declaration or on any name designating it: the declaration and every reference change, in the registry and the plans. A qualified name keeps its prefix (`middleware.esb` → `middleware.broker`), a short name stays short; renaming a system or a site changes its segment in the names of its descendants. The new name is checked first (valid, not taken by a sibling), then the project compiled again: a renaming that would give an error, or leave a name it cannot follow (a `for_each`, a host name), is refused with the reason, and nothing changes.
- **Hover** on a resource or a reference: kind, qualified name, description, attributes (with the inherited `owner` / `operator`), contents, incoming and outgoing flows.
- **Views**: a code lens above each view of a `.cav` file, **Preview** and **Build**.
- **Checks**: above each check, its result, kept up to date as you type: `Passed · no node selected, none expected`, `Failed · 3 nodes selected, none expected`, or `Not run` while the project has errors. A click writes the detail to the **Cadastre checks** output: the expectation, the message, the nodes or edges selected and those failing it, each with the file and line it is declared at (a click opens it).
  - *Preview* opens a panel rendering the view as you type, one tab per format: the diagrams (SVG and DOT laid out by a WebAssembly build of Graphviz, no `dot` needed; Mermaid drawn with its ELK layout), the tables (HTML, and Markdown rendered by VS Code) and the text outputs; **Source** shows what a rendered format is written as. A click on a resource opens its `url` or `console` outside VS Code. While the project has errors, the last rendering stays, dimmed, under the error messages.
  - *Build* runs `cadastre build --view <name>` and writes the renderings to `out/`. Without Graphviz, the extension lays out the SVG itself.
  - Also from the command palette (**Cadastre: Preview view**, **Build view**, **Build all views**), the editor title bar and the explorer context menu.
- **Completion** (`Ctrl+Space`, and after `:`, `->`, `[`, `,`): resource kinds that fit where you are (and `virtual` before a machine), the keys of the enclosing resource (with their type), closed values (`hosting: on-prem…`), the resources a reference expects (`on:`, `carries:`, link targets, host names of virtual IPs) by their shortest unambiguous name, protocols and layers used in the project; in plans, keywords, kinds, formats, render options and style properties with their values.
- Comment toggling, bracket matching and auto-closing.

A project is the workspace folder containing the file (or, outside any workspace folder, the file's directory): all its `.ca`, `.cav` and `.cac` files are compiled together.

## Installation

Download the `.vsix` of your platform from the releases (`cadastre-0.1.0-win32-x64.vsix`, `linux-x64`, `darwin-arm64`, `darwin-x64`) and install it with **Extensions: Install from VSIX…**, or `code --install-extension cadastre-0.1.0-win32-x64.vsix`. It ships the `cadastre` executable, started as `cadastre lsp`.

The executable is looked up in this order: the `cadastre.server.path` setting, the `CADASTRE_SERVER` environment variable, the executable shipped with the extension, then `cadastre` from the `PATH` (a `.vsix` built locally has none). Without it, highlighting still works.

```bash
cargo install --path compiler/crates/cadastre-cli
```

## Settings

| Setting | Default | |
|---|---|---|
| `cadastre.server.path` | `""` | Path of the `cadastre` executable; empty: the one shipped, or from the `PATH` |
| `cadastre.diagnostics.incomplete` | `hint` | `hint`, `information`, `warning` or `off` |
| `cadastre.diagnostics.strict` | `false` | Incomplete elements are errors, like `cadastre check --strict` |
| `cadastre.trace.server` | `off` | Traces the LSP messages in the output panel |

Command: **Cadastre: Restart language server**.

## Development

```bash
npm install
npm run compile
npm test
```

`npm run compile` also bundles Mermaid and its ELK layout into `media/mermaid.js` (from `media-src/`), loaded by the preview when a Mermaid tab is shown. `npm test` tokenizes snippets with VS Code's TextMate engine; `node test/grammar.test.js file.ca` prints the scopes of a file.

To try the extension, open this folder in VS Code and run **Run extension (examples/retail)** (F5): it uses the debug build of the compiler (`cargo build` in `compiler/`). `npm run package` builds a `.vsix`, installed with **Extensions: Install from VSIX…**; with an executable copied to `bin/` (`cadastre` or `cadastre.exe`), it ships it, as the releases do (`npx vsce package --target win32-x64`).
