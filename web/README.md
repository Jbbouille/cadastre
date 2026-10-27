# Cadastre in the browser

The compiler core built as WebAssembly, its JavaScript wrapper, and a playground.

## Building the module

The parser is C code, so building for `wasm32-unknown-unknown` needs a C compiler able to target WebAssembly, such as clang or `zig cc`, given through `CC_wasm32_unknown_unknown` and `AR_wasm32_unknown_unknown`.

```bash
cd compiler
cargo build -p cadastre-wasm --target wasm32-unknown-unknown --profile wasm
cp target/wasm32-unknown-unknown/wasm/cadastre_wasm.wasm ../web/
```

**With zig** (Windows especially, where clang is rarely installed): zig names the target `wasm32-freestanding` and refuses the `--target=wasm32-unknown-unknown` that cargo passes; the scripts of [`zig/`](zig) translate it.

```bash
# Windows (Git Bash): the .cmd scripts, by their Windows path
CC_wasm32_unknown_unknown="$(cygpath -w ../web/zig/cc.cmd)" AR_wasm32_unknown_unknown="$(cygpath -w ../web/zig/ar.cmd)" \
  cargo build -p cadastre-wasm --target wasm32-unknown-unknown --profile wasm
# Linux, macOS
CC_wasm32_unknown_unknown=../web/zig/cc.sh AR_wasm32_unknown_unknown=../web/zig/ar.sh \
  cargo build -p cadastre-wasm --target wasm32-unknown-unknown --profile wasm
```

The module carries the version and the commit it is built from (`version`, `0.1.0 (a1b2c3d)`), as the compiler does.

The `wasm` profile optimizes for size (`opt-level = "z"`), and Tree-sitter is built without `std`, so without the regular expressions of query predicates, which our queries do not use: about 1.4 MB, 465 KB compressed. `wasm-opt -Oz` would save 13 % more uncompressed, but the compressed module would grow slightly.

## Interface

The module has no imports and a JSON-in, JSON-out interface, wrapped by [`cadastre.js`](cadastre.js):

```js
import { load } from "./cadastre.js";
const cadastre = await load("./cadastre_wasm.wasm");
const { diagnostics, outputs } = cadastre.build({ files: [{ path: "infra.ca", text: "…" }], view: "network" });
const ranges = cadastre.highlight("server web01"); // [[0, 6, "type"], [7, 12, "variable"]]
```

## Publishing a project

The compiler shows a project in the playground, read-only and without the examples: its views, its files, and the ad-hoc views written in the page.

```bash
cadastre serve src                        # http://127.0.0.1:8080/: the site of the project, with the playground under /playground/
cadastre serve src --host 0.0.0.0 --port 8080 --title "Our IS"
cadastre site src --out public            # the same site, as a folder for a static web server
```

- `cadastre serve` serves the site of the project (see the guide, § The site) and, under `playground/`, this page with `project.json`, the files of the project read again as they change: always up to date, nothing to rebuild. It listens on `127.0.0.1` unless given `--host`.
- `cadastre site` writes the site of the project and, under `playground/`, this page, the module, Graphviz for the browser and `project.json`, to publish from the continuous integration:

  ```yaml
  pages:
    script:
      - cadastre site src --out public --title "Our IS"
    artifacts:
      paths: [public]
  ```

- The page comes with the executable of the releases (`playground/` beside it), otherwise from `--playground <folder>` or the `CADASTRE_PLAYGROUND` variable: a folder built by `build-playground.py --no-examples`, or the `cadastre-playground` archive unzipped.
- The name shown is `--title`, otherwise the name of the directory (of its parent for `src`).

## Links

The address of the page names what it shows, in its hash (never sent to the server): `#view=flux-pricing&format=svg`, `#example=retail&view=landscape` in the demo. An ad-hoc view is in it too, compressed: `#q=K07NSU0u…&format=svg`. **Copy link** copies it; opening the link shows the same view, the same format, the same ad-hoc view.

## Self-hosting the playground

The releases hold `cadastre-playground-<version>.zip`: unzip it on any static web server (IIS, nginx, Apache…) and open `index.html`. It holds the page, the WebAssembly module, Graphviz for the browser and the examples, and loads nothing from the internet; the files of a project opened in it are read by the browser and sent nowhere.

To build it from the repository, the module first (see above), then:

```bash
python web/build-playground.py --wasm compiler/target/wasm32-unknown-unknown/wasm/cadastre_wasm.wasm --out playground
```

The script downloads Graphviz for the browser once, from the CDN the page uses (`--viz viz-standalone.js` to give a copy instead). Served from the repository, the page loads Graphviz from that CDN and the examples from `../examples/`.

**After a `cargo install`**, rebuild the playground too: `cargo install` replaces the executable, not the `playground/` folder beside it, and a playground of another build may report other diagnostics than `cadastre check`. `cadastre site` and `cadastre serve` warn when the module of the playground is not of the build of the compiler, and the published page shows *Another version than the compiler*, with both versions in its tooltip.

## Playground

[`index.html`](index.html) is a playground:

- edit the files of an example project, or open a folder of your own: its `.ca`, `.cav` and `.cac` files are read by the page and stay in your browser;
- browse the files in a tree, folders folding;
- show a view with the play icon in the margin before it, or with `Ctrl+Enter` in it; or write one on the fly (an ad-hoc view, such as `select server group-by site edges lift([flow], via: on)`), rendered instead; the name of the view shown, beside the formats, leads to its definition;
- see the diagram, the table, ASCII art, text, Mermaid or DOT output and the diagnostics as you type, and download a view as a draw.io file, laid out like the diagram, or its table as an Excel workbook or a CSV file, generated in the page;
- share a view by its link (see Links).

With `project.json` beside it (`cadastre serve`, `cadastre site`), the page shows that project only, read-only; its `compiler` field is the build of the compiler that published it, compared with the module's; its `site` field (`../`) is where the site of the project starts, and the header links back to it: to the page of the view shown, or to the home. Its `language` field (`en`, `fr`) is the language of the page and of the words of the diagrams; without it, the page follows the language of the browser.

The editor is highlighted by the module itself, from the grammar's [`highlights.scm`](../grammar/queries/highlights.scm), like the language server. SVG is rendered in the page by a WebAssembly build of Graphviz ([`@viz-js/viz`](https://github.com/mdaines/viz-js)). Serve the repository root and open `/web/`:

```bash
python -m http.server 8000
```
