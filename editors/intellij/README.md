# Cadastre for IntelliJ

Support of the Cadastre language in the IDEs of the IntelliJ platform (IntelliJ IDEA, and the others from 2024.2), through [LSP4IJ](https://github.com/redhat-developer/lsp4ij): the plugin runs the Cadastre language server (`cadastre lsp`) on the `.ca` and `.cav` files.

- **Highlighting** from the first keystroke, with the TextMate grammar of the [VS Code extension](../vscode/README.md) (one grammar for both editors, copied at build time), refined by the semantic tokens of the language server.
- From the language server: **diagnostics**, **completion**, **go to definition**, **find references**, **rename** (`Shift+F6`), **hover**, **outline** (Structure) and **formatting** (*Reformat Code*), as in VS Code.
- **Views**: above each view of a `.cav` file, the code lenses **Preview** and **Build**; also in *Tools > Cadastre* (*Preview View…*, *Build View…*, *Build All Views*, the last one in the context menu of the project view too).
  - *Preview* renders the view from the open documents and writes each of its outputs as `cadastre build` names it, in a directory of the IDE; the main one opens in the editor of the IDE for its type: the SVG (laid out by Graphviz, `dot` on the `PATH`) in the image editor, the sequence and Mermaid diagrams (`.sequence.mmd`, `.mmd`) with the Mermaid plugin of the JetBrains Marketplace, the tables in the HTML or Markdown editor; a notification opens the others. The files are rewritten as you type, as long as one of them stays open; while the project has errors, the last rendering stays.
  - *Build* runs `cadastre build --view <name>` and writes the renderings to `out/`; its output goes to the **Cadastre** tool window (*Build*), the written files as links.
- **Checks**: above each check, its result, kept up to date as you type (`✓ Passed · no node selected, none expected`, `⚠ Failed · 3 nodes selected, none expected`); a click shows the detail in the **Cadastre** tool window (*Checks*): the expectation, the message, the nodes or edges selected and those failing it, each with the file and line it is declared at, as a link.
- **Export Model…** (*Tools > Cadastre*): the whole registry as ArchiMate, GraphML or JSON.
- **Settings > Tools > Cadastre**: the path of `cadastre` (otherwise the `CADASTRE_SERVER` environment variable, then the `PATH`), how incomplete elements are reported (`hint`, `information`, `warning`, `off`) and whether they are errors (`--strict`, for the builds too); applying them restarts the language server, as *Tools > Cadastre > Restart Language Server* does.

Unlike VS Code, the preview needs Graphviz for the diagrams, and the Mermaid plugin for the sequence diagrams; it shows one output per editor, rather than a panel of tabs.

## Installing

1. Install `cadastre` (the command line of the [releases](https://github.com/Jbbouille/cadastre/releases), or `cargo install --path compiler/crates/cadastre-cli`), on the `PATH` or anywhere: its path then goes in *Settings > Tools > Cadastre*.
2. Install LSP4IJ from the JetBrains Marketplace, then the plugin: *Settings > Plugins > ⚙ > Install Plugin from Disk…*, with the archive built below.

## Building

```bash
./gradlew buildPlugin      # build/distributions/cadastre-intellij-<version>.zip
./gradlew runIde           # an IDE with the plugin, to try it
```

Gradle downloads the IntelliJ platform the plugin is built against (`platformVersion` in `gradle.properties`) and LSP4IJ. A JDK 21 runs the build.
