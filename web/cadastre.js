// Cadastre in the browser: a thin wrapper around the WebAssembly module.
//
//   import { load } from "./cadastre.js";
//   const cadastre = await load("./cadastre_wasm.wasm");
//   const result = cadastre.build({ files: [{ path: "infra.ca", text: "…" }], view: "network" });
//
// `result` has `summary`, `views`, `diagnostics` and `outputs` (see
// compiler/crates/cadastre-wasm/src/lib.rs). For the `svg` format, an output's
// `content` is DOT, to be turned into SVG with a WebAssembly build of Graphviz.
//
//   cadastre.highlight("server web01")  // [[0, 6, "type"], [7, 12, "variable"]]

/**
 * Loads the module from a URL, a `Response`, or the bytes of the `.wasm` file.
 * @param {string | URL | Response | BufferSource} source
 * @returns {Promise<Cadastre>}
 */
export async function load(source = new URL("./cadastre_wasm.wasm", import.meta.url)) {
  let bytes = source;
  if (typeof source === "string" || source instanceof URL) {
    source = await fetch(source);
  }
  if (source instanceof Response) {
    if (!source.ok) throw new Error(`cannot load the Cadastre module: HTTP ${source.status}`);
    bytes = await source.arrayBuffer();
  }
  const { instance } = await WebAssembly.instantiate(bytes, {});
  return new Cadastre(instance.exports);
}

export class Cadastre {
  /** @param {WebAssembly.Exports} exports */
  constructor(exports) {
    this.exports = exports;
    this.encoder = new TextEncoder();
    this.decoder = new TextDecoder();
  }

  /**
   * Compiles a project and renders its views.
   * @param {{ files: { path: string, text: string }[], view?: string, strict?: boolean }} request
   */
  build(request) {
    const result = JSON.parse(this.call("build", JSON.stringify(request)));
    if (result.error) throw new Error(result.error);
    return result;
  }

  /**
   * Highlights one file with the grammar's `highlights.scm`.
   * @param {string} text
   * @returns {[number, number, string][]} non-overlapping `[start, end, capture]` ranges, in
   *   string indices (UTF-16 code units), such as `[0, 6, "keyword"]`
   */
  highlight(text) {
    return JSON.parse(this.call("highlight_text", text));
  }

  /**
   * Draws the icons of the kinds in an SVG laid out by Graphviz from the DOT of a `svg` output.
   * @param {string} svg
   * @returns {string}
   */
  withIcons(svg) {
    return this.call("with_icons", svg);
  }

  /**
   * Draws a `svg` output: Graphviz (a `Viz` instance) lays out each variant of its DOT (the frames
   * in several orders, the ranks over the whole view or frame by frame), the drawing that reads
   * best is kept, with its icons.
   * @param {{ renderString: (dot: string, options: { format: string }) => string }} viz
   * @param {string} dot
   * @returns {string}
   */
  draw(viz, dot) {
    const variants = JSON.parse(this.call("dot_variants", dot));
    const drawings = variants.map((d) => viz.renderString(d, { format: "svg" }));
    const best = drawings.length > 1 ? this.call("best_drawing", drawings.join("\n")) : drawings[0];
    return this.withIcons(best);
  }

  /**
   * Imports a Terraform state (`terraform show -json`, or a `.tfstate`) into `.ca` files to review.
   * @param {string} terraform
   * @returns {{ path: string, text: string }[]}
   */
  importTerraform(terraform) {
    const result = JSON.parse(this.call("import", JSON.stringify({ terraform })));
    if (result.error) throw new Error(result.error);
    return result.files;
  }

  /**
   * Imports an inventory in CSV (a resource per row: `kind`, `name`, `parent`, keys) into a `.ca`
   * file to review; `kind` is the kind of every row of a file without a `kind` column.
   * @param {string} csv
   * @param {string} name the name of the file, which names the imported one
   * @param {string} [kind]
   * @param {string} [map] the columns in the words of the language: `Hostname=name,Adresse IP=ip`
   * @param {string} [kinds] the kinds in the words of the language: `Serveur physique=server`
   * @returns {{ path: string, text: string }[]}
   */
  importCsv(csv, name, kind, map = "", kinds = "") {
    const result = JSON.parse(this.call("import", JSON.stringify({ csv, name, kind, map, kinds })));
    if (result.error) throw new Error(result.error);
    return result.files;
  }

  /** The version and commit of the module: `0.1.0 (a1b2c3d)`; `null` for an older module. */
  version() {
    return this.exports.version ? this.call("version", "") : null;
  }

  /** Passes a UTF-8 string to an export and reads back its UTF-8 result. */
  call(name, text) {
    const { alloc, dealloc, result_len, memory } = this.exports;
    const input = this.encoder.encode(text);
    const ptr = alloc(input.length);
    try {
      new Uint8Array(memory.buffer, ptr, input.length).set(input);
      const out = this.exports[name](ptr, input.length);
      // Read after the call: the memory may have grown, detaching earlier views.
      return this.decoder.decode(new Uint8Array(memory.buffer, out, result_len()));
    } finally {
      dealloc(ptr, input.length);
    }
  }
}
