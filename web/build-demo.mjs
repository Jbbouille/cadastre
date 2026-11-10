// The home page of the project (cadastre-lang.org), from `demo.html`: its code colored by the
// grammar of the language, as the playground and the editors color it (the `highlight` of the
// WebAssembly module, the captures of `grammar/queries/highlights.scm`), and its examples taken
// from the files of `examples/`, so that the page never lags behind them.
//
//   node web/build-demo.mjs --wasm <cadastre_wasm.wasm> --examples examples --out site/index.html
//
// In `demo.html`, a `<pre class="code">` is colored, with as its text:
// - `data-file="tutorial/registry.ca"`: a file of an example;
// - `data-view="retail/landscape"`: a view of an example, its block and the comment above it;
// - otherwise, the code written in it.
// A `<pre class="mermaid" data-sequence="reference/invoicing-process">` gets the sequence diagram
// of that view as Mermaid text, drawn by the page. A view or a file not found fails the build.

import { readFileSync, readdirSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const args = Object.fromEntries(
  process.argv.slice(2).flatMap((arg, i, all) => (arg.startsWith("--") ? [[arg.slice(2), all[i + 1]]] : [])),
);
const wasm = args.wasm ?? join(here, "cadastre_wasm.wasm");
const examples = args.examples ?? join(here, "..", "examples");
const input = args.in ?? join(here, "demo.html");
const output = args.out;
if (!output) {
  console.error("usage: node web/build-demo.mjs [--wasm <file>] [--examples <dir>] [--in <demo.html>] --out <index.html>");
  process.exit(2);
}

// The module, through the same JSON-in, JSON-out interface as `cadastre.js`.
const { instance } = await WebAssembly.instantiate(readFileSync(wasm), {});
const exports = instance.exports;
function call(name, text) {
  const bytes = new TextEncoder().encode(text);
  const ptr = exports.alloc(bytes.length);
  try {
    new Uint8Array(exports.memory.buffer, ptr, bytes.length).set(bytes);
    const out = exports[name](ptr, bytes.length);
    return new TextDecoder().decode(new Uint8Array(exports.memory.buffer, out, exports.result_len()));
  } finally {
    exports.dealloc(ptr, bytes.length);
  }
}

const escape = (text) => text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
const unescape = (text) =>
  text.replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&quot;/g, '"').replace(/&#39;/g, "'").replace(/&amp;/g, "&");

// The code as HTML, each capture a span: `string.special` gives `hl-string hl-string-special`,
// as in the playground.
function colored(text) {
  let html = "";
  let at = 0;
  for (const [start, end, name] of JSON.parse(call("highlight_text", text))) {
    const parts = name.split(".");
    const classes = parts.map((_, i) => `hl-${parts.slice(0, i + 1).join("-")}`).join(" ");
    html += escape(text.slice(at, start)) + `<span class="${classes}">${escape(text.slice(start, end))}</span>`;
    at = end;
  }
  return html + escape(text.slice(at));
}

// The files of an example, at any depth, as the compiler reads them.
const files = new Map();
function filesOf(example) {
  if (!files.has(example)) {
    const found = [];
    const walk = (dir) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = join(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (/\.(ca|cav|cac)$/.test(entry.name)) {
          found.push({ path: path.slice(join(examples, example).length + 1).replace(/\\/g, "/"), text: readFileSync(path, "utf8") });
        }
      }
    };
    walk(join(examples, example));
    files.set(example, found);
  }
  return files.get(example);
}

// The block of a view in the files of an example, with the comment lines just above it.
function viewOf(designation) {
  const [example, name] = designation.split("/");
  for (const { text } of filesOf(example)) {
    const start = new RegExp(`^view ${name.replace(/[.-]/g, "\\$&")} \\{`, "m").exec(text);
    if (!start) continue;
    let depth = 0;
    let quoted = false;
    for (let i = start.index; i < text.length; i++) {
      const c = text[i];
      if (quoted) {
        if (c === "\\") i++;
        else if (c === '"') quoted = false;
      } else if (c === '"') {
        quoted = true;
      } else if (c === "/" && text[i + 1] === "/") {
        i = text.indexOf("\n", i);
        if (i < 0) break;
      } else if (c === "{") {
        depth++;
      } else if (c === "}" && --depth === 0) {
        const before = text.slice(0, start.index).split("\n").slice(0, -1);
        const comment = [];
        while (before.length && before[before.length - 1].startsWith("//")) comment.unshift(before.pop());
        return [...comment, text.slice(start.index, i + 1)].join("\n");
      }
    }
  }
  throw new Error(`demo.html: no view ${name} in examples/${example}`);
}

function fileOf(designation) {
  const [example, ...path] = designation.split("/");
  const file = filesOf(example).find((f) => f.path === path.join("/"));
  if (!file) throw new Error(`demo.html: no file ${designation} in examples/`);
  return file.text.replace(/\r\n/g, "\n").trimEnd();
}

// The sequence diagram of a view, as Mermaid text.
function sequenceOf(designation) {
  const [example, name] = designation.split("/");
  const result = JSON.parse(call("build", JSON.stringify({ files: filesOf(example), view: name })));
  const output = (result.outputs ?? []).find((o) => o.view === name && /sequence|mermaid/.test(o.format) && /sequenceDiagram/.test(o.content));
  if (!output) throw new Error(`demo.html: no sequence diagram for the view ${designation}: ${result.error ?? JSON.stringify(result.diagnostics ?? [])}`);
  return output.content;
}

let page = readFileSync(input, "utf8");
page = page.replace(/<pre ([^>]*\bclass="code"[^>]*)>([\s\S]*?)<\/pre>/g, (_, attributes, content) => {
  const file = /\bdata-file="([^"]+)"/.exec(attributes);
  const view = /\bdata-view="([^"]+)"/.exec(attributes);
  const text = file ? fileOf(file[1]) : view ? viewOf(view[1]) : unescape(content);
  return `<pre ${attributes}>${colored(text)}</pre>`;
});
page = page.replace(/<pre ([^>]*\bdata-sequence="([^"]+)"[^>]*)><\/pre>/g, (_, attributes, view) => {
  return `<pre ${attributes}>${escape(sequenceOf(view))}</pre>`;
});
mkdirSync(dirname(output), { recursive: true });
writeFileSync(output, page);
console.log(`wrote ${output}`);
