// Preview page of a view, independent of the VS Code API so that it can be tested in a browser.

export interface PreviewResources {
  // Mermaid for the page (media/mermaid.js), loaded when a Mermaid output is shown.
  mermaid: string;
  // The origin of the resources of the extension, for the Content Security Policy.
  cspSource: string;
}

// The preview page: the diagrams (SVG, DOT, Mermaid), the tables (HTML, Markdown) and the text
// outputs; the source of a rendered output a click away.
export function previewHtml(view: string, resources: PreviewResources): string {
  const nonce = Array.from({ length: 32 }, () => Math.floor(Math.random() * 36).toString(36)).join("");
  return /* html */ `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src data: ${resources.cspSource}; style-src 'unsafe-inline'; font-src data:; script-src 'nonce-${nonce}' ${resources.cspSource};">
<meta name="viewport" content="width=device-width, initial-scale=1">
<style>
  body { margin: 0; padding: 0; color: var(--vscode-foreground); background: var(--vscode-editor-background); font-family: var(--vscode-font-family); font-size: var(--vscode-font-size); }
  header { position: sticky; top: 0; z-index: 1; display: flex; gap: 8px; align-items: center; padding: 6px 12px; background: var(--vscode-editor-background); border-bottom: 1px solid var(--vscode-panel-border); }
  header h1 { font-size: 1em; font-weight: 600; margin: 0 8px 0 0; }
  .tabs { display: flex; gap: 2px; }
  button { font: inherit; color: var(--vscode-button-secondaryForeground); background: var(--vscode-button-secondaryBackground); border: none; padding: 3px 10px; border-radius: 2px; cursor: pointer; }
  button:hover { background: var(--vscode-button-secondaryHoverBackground); }
  button.active, button.primary { color: var(--vscode-button-foreground); background: var(--vscode-button-background); }
  .spacer { flex: 1; }
  #messages { margin: 8px 12px 0; padding: 6px 10px; white-space: pre-wrap; font-family: var(--vscode-editor-font-family); font-size: 0.9em; border-left: 3px solid var(--vscode-editorWarning-foreground); background: var(--vscode-textBlockQuote-background); }
  #messages.error { border-left-color: var(--vscode-editorError-foreground); }
  #messages:empty { display: none; }
  main { padding: 12px; }
  .canvas { background: #fff; color: #000; border-radius: 4px; padding: 12px; overflow: auto; }
  .canvas.fit svg { max-width: 100%; height: auto; }
  pre { margin: 0; padding: 12px; overflow: auto; font-family: var(--vscode-editor-font-family); font-size: var(--vscode-editor-font-size); background: var(--vscode-textCodeBlock-background); border-radius: 4px; }
  iframe { width: 100%; border: none; border-radius: 4px; background: #fff; }
  .markdown { overflow: auto; }
  .markdown table { border-collapse: collapse; }
  .markdown th, .markdown td { padding: 4px 10px; border: 1px solid var(--vscode-panel-border); text-align: left; vertical-align: top; }
  .markdown th { background: var(--vscode-textCodeBlock-background); }
  .markdown a, .canvas a { cursor: pointer; }
  .stale { opacity: 0.45; }
</style>
</head>
<body>
<header>
  <h1>${escape(view)}</h1>
  <div class="tabs" id="tabs"></div>
  <span class="spacer"></span>
  <button id="source" title="Toggle the rendering / its source">Source</button>
  <button id="zoom" title="Toggle fit to width / actual size">Actual size</button>
  <button id="build" class="primary" title="Write the renderings to out/ (cadastre build --view)">Build</button>
</header>
<div id="messages"></div>
<main id="content"><p>Rendering…</p></main>
<script nonce="${nonce}">
  const vscode = acquireVsCodeApi();
  const saved = vscode.getState() ?? {};
  const state = { format: saved.format, fit: saved.fit ?? true, source: saved.source ?? false, outputs: [] };
  const tabs = document.getElementById("tabs");
  const content = document.getElementById("content");
  const messages = document.getElementById("messages");
  const zoom = document.getElementById("zoom");
  const source = document.getElementById("source");
  const MERMAID = ${JSON.stringify(resources.mermaid)};
  const VIEW = ${JSON.stringify(view)};
  const NONCE = ${JSON.stringify(nonce)};
  let shown = 0;

  document.getElementById("build").onclick = () => vscode.postMessage({ type: "build" });
  zoom.onclick = () => { state.fit = !state.fit; save(); show(); };
  source.onclick = () => { state.source = !state.source; save(); show(); };

  function save() { vscode.setState({ format: state.format, fit: state.fit, source: state.source }); }

  // A click on a link (the url or console of a resource) opens it outside the panel.
  function links(root) {
    root.addEventListener("click", (event) => {
      const link = event.target.closest?.("a");
      const href = link && (link.getAttribute("href") || link.getAttribute("xlink:href"));
      if (!href || href.startsWith("#")) return;
      event.preventDefault();
      vscode.postMessage({ type: "open", url: href });
    });
  }
  links(content);

  // The formats to offer: one per output, the rendered ones (svg, dot, mermaid, html, markdown)
  // with their source a click away.
  function entries() {
    return state.outputs.map((o) => ({ label: o.format, format: o.format, content: o.content, svg: o.svg, html: o.html }));
  }

  // A draw.io output: a button saving the file, and its diagram.
  function drawio(current) {
    const element = document.createElement("div");
    const bar = document.createElement("p");
    const button = document.createElement("button");
    button.className = "primary";
    button.textContent = "Save " + VIEW + ".drawio…";
    button.onclick = () => vscode.postMessage({ type: "save", format: "drawio", content: current.content });
    bar.append(button, " editable in draw.io, laid out like the diagram below");
    element.append(bar);
    if (current.svg) element.append(canvas(current.svg));
    return element;
  }

  // An Excel output: a button saving the workbook, and its table.
  function workbook(current) {
    const element = document.createElement("div");
    const bar = document.createElement("p");
    const button = document.createElement("button");
    button.className = "primary";
    button.textContent = "Save " + VIEW + ".xlsx…";
    button.onclick = () => vscode.postMessage({ type: "save", format: "xlsx", content: current.content });
    bar.append(button);
    element.append(bar);
    if (current.html) element.append(page(current.html));
    return element;
  }

  // A diagram: SVG from Graphviz, parsed as XML; from Mermaid (\`markup\`), serialized as HTML.
  function canvas(svg, markup) {
    const element = document.createElement("div");
    element.className = "canvas" + (state.fit ? " fit" : "");
    if (markup) {
      element.innerHTML = svg;
      return element;
    }
    try {
      const parsed = new DOMParser().parseFromString(svg, "image/svg+xml").documentElement;
      if (parsed.nodeName !== "svg") throw new Error(parsed.textContent);
      element.append(document.importNode(parsed, true));
    } catch (error) {
      element.textContent = "Graphviz: " + error;
    }
    return element;
  }

  function text(value) {
    const pre = document.createElement("pre");
    pre.textContent = value;
    return pre;
  }

  // An HTML page (a table view), isolated in a frame without scripts; its links open outside.
  function page(html) {
    const frame = document.createElement("iframe");
    frame.setAttribute("sandbox", "allow-same-origin");
    frame.srcdoc = html;
    frame.onload = () => {
      const doc = frame.contentDocument;
      if (!doc) return;
      frame.style.height = doc.documentElement.scrollHeight + 24 + "px";
      links(doc);
    };
    return frame;
  }

  let mermaid;
  function loadMermaid() {
    mermaid ??= new Promise((resolve, reject) => {
      const script = document.createElement("script");
      script.src = MERMAID;
      script.nonce = NONCE;
      script.onload = () => {
        globalThis.mermaid.initialize({ startOnLoad: false, securityLevel: "loose", theme: "default" });
        resolve(globalThis.mermaid);
      };
      script.onerror = () => reject(new Error("Mermaid could not be loaded"));
      document.head.append(script);
    });
    return mermaid;
  }

  async function diagram(definition, element, turn) {
    try {
      const { svg } = await (await loadMermaid()).render("mermaid-" + turn, definition);
      if (turn === shown) element.replaceWith(canvas(svg, true));
    } catch (error) {
      if (turn === shown) element.replaceWith(text("Mermaid: " + (error?.message ?? error)));
    }
  }

  function show() {
    const list = entries();
    if (list.length === 0) return;
    const current = list.find((e) => e.label === state.format) ?? list[0];
    state.format = current.label;
    tabs.replaceChildren(...list.map((e) => {
      const b = document.createElement("button");
      b.textContent = e.label;
      b.className = e.label === current.label ? "active" : "";
      b.onclick = () => { state.format = e.label; save(); show(); };
      return b;
    }));
    const rendered = ["svg", "traversal", "dot", "mermaid", "sequence", "html", "markdown", "drawio"].includes(current.format);
    // A workbook has no readable source: no source button.
    const asSource = rendered && state.source;
    source.hidden = !rendered;
    source.textContent = state.source ? "Rendering" : "Source";
    zoom.hidden = asSource || !["svg", "traversal", "dot", "mermaid", "sequence", "drawio"].includes(current.format);
    zoom.textContent = state.fit ? "Actual size" : "Fit to width";
    const turn = ++shown;
    let element;
    if (asSource) {
      element = text(current.content);
    } else if (current.format === "svg" || current.format === "traversal") {
      element = canvas(current.content);
    } else if (current.format === "dot") {
      element = current.svg ? canvas(current.svg) : text(current.content);
    } else if (current.format === "mermaid" || current.format === "sequence") {
      element = document.createElement("p");
      element.textContent = "Rendering…";
      void diagram(current.content, element, turn);
    } else if (current.format === "html") {
      element = page(current.content);
    } else if (current.format === "drawio") {
      element = drawio(current);
    } else if (current.format === "xlsx") {
      element = workbook(current);
    } else if (current.format === "markdown" && current.html !== undefined) {
      element = document.createElement("div");
      element.className = "markdown";
      element.innerHTML = current.html;
    } else {
      element = text(current.content);
    }
    content.replaceChildren(element);
    content.classList.remove("stale");
  }

  window.addEventListener("message", (event) => {
    const message = event.data;
    if (message.type !== "render") return;
    messages.textContent = message.messages.join("\\n\\n");
    messages.className = message.ok ? "" : "error";
    if (message.ok) {
      state.outputs = message.outputs;
      show();
    } else {
      // Keep the last rendering, dimmed, while the project has errors.
      content.classList.add("stale");
    }
  });

  vscode.postMessage({ type: "ready" });
</script>
</body>
</html>`;
}

function escape(text: string): string {
  return text.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);
}
