// The composer of views: a view written by taking words among those proposed, each showing what
// it changes in the diagram before it is taken.
//
// Three things on the page: the view as a sentence (the words of its code, each taken out with
// its ×), its diagram, and the proposals: the views one word away from it, a kind more, a layer
// of links, a level of frames, the links lifted. The compiler evaluates them all (`variants`);
// those that are valid and change the drawing are proposed, with what they add. Pointing at one
// draws it, what it adds outlined; a click takes it. The code of the view stays the source of
// truth (the compiler gives each clause with where it is written: `cadastre.parts`), folded
// until asked, for what the words do not say.
import { load } from "./cadastre.js";

const VIZ_CDN = "https://cdn.jsdelivr.net/npm/@viz-js/viz@3.11.0/lib/viz-standalone.js";
// The examples, read beside the page (a self-hosted playground) or from the repository.
const EXAMPLES = {
  tutorial: ["registry.ca", "views.cav", "checks.cac"],
  reference: ["schema.ca", "teams.ca", "infra.ca", "data.ca", "apps.ca", "saas.ca", "views.cav", "checks.cac"],
  retail: ["schema.ca", "teams.ca", "infra.ca", "product.ca", "data.ca", "saas.ca", "systems.ca", "views.cav"],
};
// The file the view composed is built in, and the names of the proposals evaluated with it.
const COMPOSED = "composed.cav";
const CANDIDATE = "cadastre-candidate-";
// A view handed over by the playground (an opened folder): its files, for this tab only.
const HANDOVER = "cadastre-compose";

// The words of the page in the language of the reader. English is the key.
const LANGUAGES = {
  fr: {
    "Compose a view": "Composer une vue",
    "Project": "Projet",
    "Recipes…": "Recettes…",
    "Start from a recipe": "Partir d'une recette",
    "Code": "Code",
    "Finish…": "Terminer…",
    "Keep the view": "Garder la vue",
    "Copy the code": "Copier le code",
    "Copy the link": "Copier le lien",
    "Copied": "Copié",
    "Save": "Enregistrer",
    "Save in {file}": "Enregistrer dans {file}",
    "Saved in {file}": "Enregistré dans {file}",
    "Cannot save: {error}": "Enregistrement impossible : {error}",
    "File": "Fichier",
    "The .cav file to write the view in": "Le fichier .cav où écrire la vue",
    "Site": "Site",
    "This view on the site": "Cette vue sur le site",
    "Code of the view": "Code de la vue",
    "The view": "La vue",
    "Proposals": "Propositions",
    "loading…": "chargement…",
    "Write a view: view name { … }": "Écrivez une vue : view nom { … }",
    "Fix the errors to draw the view.": "Corrigez les erreurs pour dessiner la vue.",
    "Nothing to draw.": "Rien à dessiner.",
    "No diagnostic.": "Aucun diagnostic.",
    "{count} error(s): see the code": "{count} erreur(s) : voir le code",
    "error": "erreur", "warning": "avertissement", "info": "info", "incomplete": "incomplet", "help:": "aide :",
    "Take out": "Retirer",
    "Swap these two levels": "Inverser ces deux niveaux",
    // The proposals.
    "Point at a word to see what it changes, click to take it.": "Pointez un mot pour voir ce qu'il change, cliquez pour le prendre.",
    "Nothing more to propose: the code says the rest.": "Rien de plus à proposer : le code dit le reste.",
    "Show also": "Montrer aussi",
    "Link by": "Relier par",
    "Frame by": "Ranger par",
    "Simplify": "Simplifier",
    "Drawing": "Dessin",
    "their neighbours": "leurs voisins",
    "without the isolated ones": "sans les isolés",
    "links lifted onto their server": "liens remontés sur leur serveur",
    "links lifted onto their system": "liens remontés sur leur système",
    "links lifted onto their site": "liens remontés sur leur site",
    "links lifted onto their subnet": "liens remontés sur leur sous-réseau",
    "overview by {kind}": "vue d'ensemble par {kind}",
    "in the direction of the data": "dans le sens des données",
    "legend": "légende",
    "Only": "Seulement",
    "system {name}": "système {name}",
    "site {name}": "site {name}",
    "Process": "Processus",
    "process {name}": "processus {name}",
    "as racks": "en baies",
    "as a network traversal": "en traversée réseau",
    "as a sequence": "en séquence",
    "as a diagram": "en diagramme",
    "drawing…": "dessin…",
    "{n} more": "{n} de plus",
    "Preview: {label}": "Aperçu : {label}",
    "link": "lien", "links": "liens", "frame": "cadre", "frames": "cadres",
    // The menus of the diagram.
    "Hide {name}": "Masquer {name}",
    "Only {name} and its neighbours": "Seulement {name} et ses voisins",
    "Only the system {name}": "Seulement le système {name}",
    "Only the site {name}": "Seulement le site {name}",
    "Its page on the site": "Sa page sur le site",
    "Without the layer {layer}": "Sans la couche {layer}",
    "an edge": "une arête",
    // The end.
    "Name": "Nom", "Title": "Titre", "Description": "Description", "Folder": "Dossier",
    "Network/Security": "Réseau/Sécurité",
    // The recipes.
    "Pick a resource…": "Choisir une ressource…",
    "To…": "Vers…",
    "Racks": "Les baies",
    "The devices in their racks, at their height.": "Les équipements dans leurs baies, à leur hauteur.",
    "{x}, as a sequence": "{x}, en séquence",
    "The steps of a process, its calls and replies, as a sequence diagram.": "Les étapes d'un processus, ses appels et réponses, en diagramme de séquence.",
    "{x}, through the network": "{x}, à travers le réseau",
    "The devices each step of a process goes through.": "Les équipements que traverse chaque étape d'un processus.",
    "The path from {x} to {y}": "Le chemin de {x} à {y}",
    "The devices between two resources, deduced from the cabling, or why there is no path.": "Les équipements entre deux ressources, déduits du câblage, ou pourquoi il n'y a pas de chemin.",
    "Create": "Créer",
    "More recipes": "Plus de recettes",
    "Or start from nothing:": "Ou partir de rien :",
    "An empty view": "Une vue vide",
    "What {x} exchanges with": "Ce qu'échange {x}",
    "The resource and the ends of its flows, in the direction of the data.": "La ressource et les extrémités de ses flux, dans le sens des données.",
    "The flows of {x}": "Les flux de {x}",
    "Its apps, jobs, databases and SaaS, the flows between them and with the rest.": "Ses applications, traitements, bases et SaaS, leurs flux entre eux et avec le reste.",
    "Landscape": "Paysage",
    "The flows between the systems and the SaaS, lifted from those of their apps.": "Les flux entre les systèmes et les SaaS, remontés de ceux de leurs applications.",
    "The network of {x}": "Le réseau de {x}",
    "Its servers by subnet and those they exchange with, the application flows lifted onto them.": "Ses serveurs par sous-réseau et ceux avec qui ils échangent, les flux applicatifs remontés sur eux.",
    "Personal data": "Données personnelles",
    "The flows carrying personal data, with what they carry.": "Les flux qui portent des données personnelles, avec ce qu'ils portent.",
    "The trail of {x}": "Le chemin de {x}",
    "Where the data goes, link after link, from where it lives.": "Où va la donnée, lien après lien, depuis là où elle vit.",
    "If {x} fails": "Si {x} tombe",
    "What becomes unavailable with it, in red.": "Ce qui devient indisponible avec, en rouge.",
    "What crosses {x}": "Ce qui traverse {x}",
    "The flows through it, merged by port: its rule table.": "Les flux qui le traversent, fusionnés par port : sa table de règles.",
    "Ownership": "Propriété",
    "The apps, databases and SaaS by owner team, with their operator.": "Les applications, bases et SaaS par équipe propriétaire, avec leur exploitant.",
    "Inventory": "Inventaire",
    "Each app with its servers and their site, as a table.": "Chaque application avec ses serveurs et leur site, en tableau.",
  },
};
let lang = "en";
const t = (text, values = {}) =>
  Object.entries(values).reduce((s, [k, v]) => s.replaceAll(`{${k}}`, v), LANGUAGES[lang]?.[text] ?? text);

// The recipes, in the words of the page: their body comes from the compiler (`facts.recipes`).
// The first nine are shown; the others behind *More recipes*.
const RECIPES = {
  around: ["What {x} exchanges with", "The resource and the ends of its flows, in the direction of the data."],
  "system-flows": ["The flows of {x}", "Its apps, jobs, databases and SaaS, the flows between them and with the rest."],
  landscape: ["Landscape", "The flows between the systems and the SaaS, lifted from those of their apps."],
  "site-network": ["The network of {x}", "Its servers by subnet and those they exchange with, the application flows lifted onto them."],
  "process-sequence": ["{x}, as a sequence", "The steps of a process, its calls and replies, as a sequence diagram."],
  "process-network": ["{x}, through the network", "The devices each step of a process goes through."],
  racks: ["Racks", "The devices in their racks, at their height."],
  "network-path": ["The path from {x} to {y}", "The devices between two resources, deduced from the cabling, or why there is no path."],
  failure: ["If {x} fails", "What becomes unavailable with it, in red."],
  "personal-data": ["Personal data", "The flows carrying personal data, with what they carry."],
  "data-trail": ["The trail of {x}", "Where the data goes, link after link, from where it lives."],
  firewall: ["What crosses {x}", "The flows through it, merged by port: its rule table."],
  ownership: ["Ownership", "The apps, databases and SaaS by owner team, with their operator."],
  inventory: ["Inventory", "Each app with its servers and their site, as a table."],
};
const FIRST_RECIPES = 9;
const MERMAID_CDN = "https://cdn.jsdelivr.net/npm/mermaid@11.4.1/dist/mermaid.esm.min.mjs";

// The preview of a view: that of the first of its formats.
const PREVIEW_OF = { svg: "svg", dot: "svg", drawio: "svg", png: "svg", html: "html", xlsx: "html", csv: "html", tsv: "html", markdown: "html", text: "text", ascii: "text", mermaid: "mermaid", sequence: "sequence", traversal: "traversal", racks: "racks" };
// The order of the clauses, where a missing one is written.
const ORDER = ["select", "zones", "edges", "style", "render"];
// What `lift` lifts the ends of the edges onto (`via`), with the words proposing it.
const LIFTS = [["on", "links lifted onto their server"], ["ancestor(system)", "links lifted onto their system"], ["ancestor(site)", "links lifted onto their site"], ["ancestor(subnet)", "links lifted onto their subnet"]];
// The groups of the proposals, in their order, and how many of each are shown at first.
const GROUPS = [["show", "Show also"], ["link", "Link by"], ["frame", "Frame by"], ["only", "Only"], ["simplify", "Simplify"], ["process", "Process"], ["draw", "Drawing"]];
// Those that change a drawing of its own: racks are drawn whole, or those of a site; a sequence
// and a traversal draw the processes and the paths of the view only.
const GROUPS_OF = { racks: ["only", "draw"], traversal: ["process", "draw"], sequence: ["process", "draw"] };
// The drawings: a view is previewed as the first of them among its formats.
const DRAWINGS = [["svg", "as a diagram"], ["racks", "as racks"], ["traversal", "as a network traversal"], ["sequence", "as a sequence"]];
const SHOWN = 6;

const $ = (id) => document.getElementById(id);
const state = {
  files: [], modules: [], satellites: false, example: null,
  // The facts of the project: kinds, layers, keys, resources, recipes.
  facts: null,
  // The views of the project, by name.
  views: [],
  // The view composed: the text of its block, `view name { … }`.
  text: "",
  // The view of the project it edits: `{ path, name }`, replaced by it when built and saved.
  origin: null,
  insight: null,
  // The proposals: `{ group, label, text, effect }`, and the groups shown whole.
  proposals: [],
  unfolded: new Set(),
};
let cadastre, viz, project, timer;
// The range of the last change, highlighted in the code.
let changed = null;

// ——— Text: the view, clause by clause ———

const partsOf = (text) => (cadastre ? cadastre.parts(text)[0] ?? null : null);
const view = () => partsOf(state.text);
const splice = (text, [start, end], replacement) => text.slice(0, start) + replacement + text.slice(end);
const lineStart = (text, at) => text.lastIndexOf("\n", at - 1) + 1;
const quoted = (text) => JSON.stringify(text);
const names = (list) => list.split(",").map((s) => s.trim()).filter(Boolean);

// Removes a clause, with its line when it stands alone on it.
function remove(text, [start, end]) {
  const begin = lineStart(text, start);
  const next = text.indexOf("\n", end);
  const stop = next < 0 ? text.length : next;
  if (!text.slice(begin, start).trim() && !text.slice(end, stop).trim()) return text.slice(0, begin) + text.slice(next < 0 ? stop : stop + 1);
  return text.slice(0, start) + text.slice(end).replace(/^[ \t]*/, "");
}

// Writes a clause before the one starting at `at`, or before the closing brace.
function insert(text, parts, at, clause) {
  if (at != null) return splice(text, [at, at], `${clause}\n${parts.indent}`);
  const begin = lineStart(text, parts.end);
  if (!text.slice(begin, parts.end).trim()) return splice(text, [begin, begin], `${parts.indent}${clause}\n`);
  return splice(text, [parts.end, parts.end], `${clause} `);
}

// `text` with the clause `key` (`select`, `edges`, `render`) written as `clause`, removed when empty.
function withClause(key, clause, text = state.text) {
  const parts = partsOf(text);
  if (!parts) return text;
  const current = parts[key];
  if (current?.range) return clause ? splice(text, current.range, clause) : remove(text, current.range);
  if (!clause) return text;
  const next = ORDER.slice(ORDER.indexOf(key) + 1).map((k) => (k === "zones" ? parts.zones[0] : parts[k])).find((p) => p?.range);
  return insert(text, parts, next?.range[0], clause);
}

// The text with the attribute `key` (`title`, `group`…) set, removed when empty; a new one goes first.
function withAttribute(key, value) {
  const parts = view();
  if (!parts) return state.text;
  const clause = value ? `${key}: ${quoted(value)}` : "";
  const current = parts.attributes[key];
  if (current) return clause ? splice(state.text, current.range, clause) : remove(state.text, current.range);
  if (!clause) return state.text;
  const starts = [...Object.values(parts.attributes), ...ORDER.map((k) => (k === "zones" ? parts.zones[0] : parts[k]))]
    .filter((p) => p?.range).map((p) => p.range[0]);
  return insert(state.text, parts, starts.length ? Math.min(...starts) : null, clause);
}

const selectText = (s) => (!s.kinds.length ? "" : `select ${s.kinds.join(", ")}${s.where ? ` where ${s.where}` : ""}${s.with_ends ? " with ends" : ""}${s.without_isolated ? " without isolated" : ""}${s.group_by.length ? ` group-by ${s.group_by.join(" > ")}` : ""}`);
const selectOf = (parts) => ({ kinds: [], where: null, with_ends: false, without_isolated: false, group_by: [], ...(parts.select ?? {}) });

// The edges: the main selector (layers, maybe lifted), the others as written, the options.
function edgesOf(parts) {
  const selectors = parts.edges?.selectors ?? [];
  let main = null;
  const others = [];
  for (const selector of selectors) {
    if (!main) {
      let m = selector.match(/^\[([^\]]*)\]$/);
      if (m) { main = { layers: names(m[1]), lift: null }; continue; }
      m = selector.match(/^lift\(\s*\[([^\]]*)\]\s*(?:,\s*([\s\S]*))?\)$/);
      if (m) { main = { layers: names(m[1]), lift: (m[2] ?? "").trim() }; continue; }
    }
    others.push(selector);
  }
  return { main: main ?? { layers: [], lift: null }, others, options: parts.edges?.options ?? [] };
}

function edgesText({ main, others, options }) {
  const list = [];
  if (main.layers.length) {
    const layers = `[${main.layers.join(", ")}]`;
    list.push(main.lift !== null ? `lift(${layers}${main.lift ? `, ${main.lift}` : ""})` : layers);
  }
  list.push(...others, ...options.map((o) => `${o.key}: ${o.value}`));
  return list.length ? `edges  ${list.join(", ")}` : "";
}

const liftVia = (lift) => lift?.match(/(?:^|,)\s*via:\s*([^,]+)/)?.[1].trim() ?? "";
function withVia(lift, via) {
  const rest = (lift ?? "").split(/,(?![^(]*\))/).map((s) => s.trim()).filter((s) => s && !s.startsWith("via:"));
  return [via ? `via: ${via}` : "", ...rest].filter(Boolean).join(", ");
}

const renderOf = (parts) => ({ formats: [...(parts.render?.formats ?? [])], options: [...(parts.render?.options ?? [])] });
function renderText({ formats, options }, indent) {
  // An option needs a format to hold it: the diagram.
  if (!formats.length && options.length) formats = ["svg"];
  if (!formats.length) return "";
  let out = `render ${formats.join(", ")}`;
  const items = options.map((o) => `${o.key}: ${o.value}`);
  if (items.length) {
    out += items.some((i) => i.includes("\n")) || items.join(", ").length > 72
      ? ` {\n${items.map((i) => `${indent}${indent}${i}`).join("\n")}\n${indent}}`
      : ` { ${items.join(", ")} }`;
  }
  return out;
}
const option = (render, key) => render.options.find((o) => o.key === key)?.value ?? "";
function withOption(render, key, value) {
  const options = render.options.filter((o) => o.key !== key);
  const at = render.options.findIndex((o) => o.key === key);
  if (value !== "" && value !== null) options.splice(at < 0 ? options.length : at, 0, { key, value });
  return { ...render, options };
}

// A condition added to the filter of `select`: `and`, the filter parenthesized when it has an `or`.
function withCondition(select, condition) {
  const where = select.where ? `${/\bor\b/.test(select.where) ? `(${select.where})` : select.where} and ${condition}` : condition;
  return { ...select, where };
}

// ——— Changes: the text rewritten, the sentence, the diagram and the proposals following ———

function apply(text) {
  const old = state.text;
  if (text === old) return;
  // What changed, highlighted in the code.
  let start = 0;
  while (start < old.length && start < text.length && old[start] === text[start]) start++;
  let [end, oldEnd] = [text.length, old.length];
  while (end > start && oldEnd > start && old[oldEnd - 1] === text[end - 1]) { end--; oldEnd--; }
  changed = [start, end];
  setText(text);
}

function setText(text) {
  state.text = text;
  $("code").value = text;
  leave();
  hideMenu();
  paintCode();
  sentence();
  run();
}

// ——— The build: the project with the view composed in place of the one it edits ———

function projectFiles(name, composed) {
  const files = state.files.map((file) => {
    if (!file.path.endsWith(".cav")) return file;
    const replaced = cadastre.parts(file.text)
      .filter((v) => v.name === name || (state.origin && file.path === state.origin.path && v.name === state.origin.name))
      .sort((a, b) => b.range[0] - a.range[0]);
    if (!replaced.length) return file;
    let text = file.text;
    for (const v of replaced) text = text.slice(0, v.range[0]) + text.slice(v.range[1]);
    return { path: file.path, text };
  });
  return [...files, { path: COMPOSED, text: `${composed}\n` }];
}

function build(request) {
  return cadastre.build({ modules: state.modules, satellites: state.satellites, language: lang, ...request });
}

// The drawing of a view text: its build, and what to show of it.
function drawingOf(text) {
  const parts = partsOf(text);
  const format = PREVIEW_OF[renderOf(parts).formats.find((f) => PREVIEW_OF[f])] ?? "svg";
  const result = build({ files: projectFiles(parts.name, text), view: parts.name, formats: [format], insight: true });
  const output = result.outputs.find((o) => o.view === parts.name);
  let svg = null;
  if (output && ["svg", "traversal", "racks"].includes(format)) {
    svg = output.content.trimStart().startsWith("<svg") ? output.content : viz ? cadastre.draw(viz, output.content) : null;
  }
  return { parts, format, result, output, svg };
}

// The drawing shown when no proposal is looked at, kept to come back to it without building it again.
let current = null;

function run() {
  const parts = view();
  if (!parts) {
    state.insight = null;
    current = null;
    $("output").replaceChildren(empty(t("Write a view: view name { … }")));
    $("diagnostics").replaceChildren();
    $("proposals").replaceChildren();
    return;
  }
  try {
    current = drawingOf(state.text);
  } catch (error) {
    current = null;
    $("output").replaceChildren(Object.assign(document.createElement("pre"), { textContent: String(error) }));
    return;
  }
  state.insight = current.result.insight ?? null;
  // A new drawing: shown whole.
  zoomState = null;
  show(current, true);
  diagnostics(current.result.diagnostics);
  remember();
  // The proposals after the drawing: the page answers first.
  clearTimeout(timer);
  timer = setTimeout(propose, 30);
}

const empty = (text) => Object.assign(document.createElement("div"), { className: "empty", textContent: text });

// Shows a drawing; `keep`: with the zoom of the view composed, a proposal being fitted whole.
let zoom = null;
let zoomState = null;
function show(drawing, keep, added = null) {
  const target = $("output");
  const { format, result, output, svg, parts } = drawing;
  if (zoom && current && shownIs === current) zoomState = zoom.state();
  zoom = null;
  shownIs = drawing;
  target.replaceChildren();
  if (!output) {
    target.append(empty(t(result.summary.errors ? "Fix the errors to draw the view." : "Nothing to draw.")));
  } else if (svg) {
    const box = Object.assign(document.createElement("div"), { className: "zoom" });
    const element = document.importNode(new DOMParser().parseFromString(svg, "image/svg+xml").documentElement, true);
    // The name of each node and edge in the DOT, before zoom.js turns the titles into bubbles.
    for (const item of element.querySelectorAll("g.node, g.edge")) {
      const key = item.querySelector(":scope > title")?.textContent ?? "";
      item.dataset.compose = key;
      const node = key.match(/^n(\d+)/);
      if (added && node && added.has(`n${node[1]}`)) item.classList.add("added");
    }
    box.append(element);
    target.append(box);
    zoom = globalThis.cadastreZoom?.attach(box, { wheel: true, height: "calc(100% - 4px)", state: keep ? zoomState : undefined, language: lang }) ?? null;
  } else if (format === "sequence" || format === "mermaid") {
    // Drawn by Mermaid, loaded when first needed; its text meanwhile, and if it cannot be.
    const sheet = h("div", { className: "sheet", textContent: t("drawing…") });
    target.append(sheet);
    drawing.mermaid ??= mermaidSvg(output.content);
    drawing.mermaid.then(
      (drawn) => { if (shownIs === drawing) sheet.innerHTML = drawn; },
      () => { if (shownIs === drawing) sheet.replaceChildren(h("pre", { textContent: output.content })); },
    );
  } else if (format === "html") {
    target.append(Object.assign(document.createElement("iframe"), { srcdoc: output.content, sandbox: "", title: parts.name }));
  } else {
    target.append(Object.assign(document.createElement("pre"), { textContent: output.content }));
  }
}
let shownIs = null;

let mermaid = null;
let drawings = 0;
async function mermaidSvg(text) {
  if (!mermaid) {
    mermaid = (await import(MERMAID_CDN)).default;
    mermaid.initialize({ startOnLoad: false, securityLevel: "strict" });
  }
  return (await mermaid.render(`compose-mermaid-${++drawings}`, text)).svg;
}

function diagnostics(list) {
  // Those of the view composed, and the errors of the project, which keep it from being drawn.
  const shown = list.filter((d) => d.severity === "error" || d.locations[0]?.path === COMPOSED);
  $("diagnostics").replaceChildren(...(shown.length ? shown.map((d) => {
    const item = document.createElement("div");
    item.className = d.severity;
    const where = d.locations[0];
    item.append(Object.assign(document.createElement("b"), { textContent: t(d.severity) }),
      `: ${d.message}${where && where.path !== COMPOSED ? `  (${where.path}:${where.line})` : ""}`);
    if (d.help) item.append(Object.assign(document.createElement("div"), { className: "help", textContent: `${t("help:")} ${d.help}` }));
    if (where?.path === COMPOSED) item.onclick = () => goTo(where.line, where.column);
    return item;
  }) : [Object.assign(document.createElement("div"), { className: "none", textContent: t("No diagnostic.") })]));
  // An error the sentence cannot show: the code says where.
  const errors = shown.filter((d) => d.severity === "error").length;
  if (errors) status(t("{count} error(s): see the code", { count: errors }), "ko");
  else if ($("status").className === "ko") status("");
}

function goTo(line, column) {
  const code = $("code");
  const offset = code.value.split("\n").slice(0, line - 1).reduce((n, l) => n + l.length + 1, 0) + column - 1;
  code.focus();
  code.setSelectionRange(offset, offset);
}

// A small DOM builder: `h("div", { className: "row" }, child, …)`.
function h(tag, props = {}, ...children) {
  const element = Object.assign(document.createElement(tag), props);
  element.append(...children.flat().filter((c) => c !== null && c !== undefined && c !== false));
  return element;
}

// ——— The sentence: the view as its words, each taken out with its × ———

function sentence() {
  const root = $("sentence");
  const parts = view();
  if (!parts) {
    root.replaceChildren();
    return;
  }
  const select = selectOf(parts);
  const edges = edgesOf(parts);
  const render = renderOf(parts);
  const setSelect = (s) => withClause("select", selectText(s));
  const setEdges = (e) => withClause("edges", edgesText(e));
  const setRender = (r) => withClause("render", renderText(r, parts.indent));
  // A word, taken out by its ×: pointing at the × shows the view without it.
  const word = (label, without) => {
    const chip = h("span", { className: `word${without ? "" : " fixed"}` }, h("span", { textContent: label, title: label }));
    if (without) {
      const button = h("button", { textContent: "×", title: t("Take out") });
      offer(button, () => ({ label: `− ${label}`, text: without() }));
      chip.append(button);
    }
    return chip;
  };
  const keyword = (text) => h("span", { className: "keyword", textContent: text });
  const plain = (text) => h("span", { className: "plain", textContent: text });
  const lines = [];

  // What the code says besides its clauses, as written: a scenario of failures.
  const others = Object.entries(parts.attributes).filter(([key]) => !["title", "description", "group"].includes(key));
  for (const [key, attribute] of others) {
    lines.push(h("div", { className: "line" }, keyword(key), word(attribute.raw, () => remove(state.text, attribute.range))));
  }

  const kinds = select.kinds.map((kind) => word(kind, select.kinds.length > 1 ? () => setSelect({ ...select, kinds: select.kinds.filter((k) => k !== kind) }) : null));
  lines.push(h("div", { className: "line" }, keyword("select"), kinds,
    select.where ? [plain("where"), word(select.where, () => setSelect({ ...select, where: null }))] : null,
    select.with_ends ? word("with ends", () => setSelect({ ...select, with_ends: false })) : null,
    select.without_isolated ? word("without isolated", () => setSelect({ ...select, without_isolated: false })) : null));

  if (parts.edges) {
    const layers = edges.main.layers.map((layer) => word(layer, () => setEdges({ ...edges, main: { ...edges.main, layers: edges.main.layers.filter((l) => l !== layer) } })));
    const lifted = edges.main.lift !== null && edges.main.layers.length;
    lines.push(h("div", { className: "line" }, keyword("edges"),
      lifted ? plain("lift(") : null, layers,
      lifted ? [word(edges.main.lift || "…", () => setEdges({ ...edges, main: { ...edges.main, lift: null } })), plain(")")] : null,
      edges.others.map((s, i) => word(s, () => setEdges({ ...edges, others: edges.others.filter((_, j) => j !== i) }))),
      edges.options.map((o, i) => word(`${o.key}: ${o.value}`, () => setEdges({ ...edges, options: edges.options.filter((_, j) => j !== i) })))));
  }

  if (select.group_by.length) {
    const levels = select.group_by;
    const items = [];
    levels.forEach((level, i) => {
      if (i > 0) {
        // The order of the frames, from the broadest: two levels swapped by what is between them.
        const swap = h("button", { className: "swap", textContent: "›", title: t("Swap these two levels") });
        offer(swap, () => {
          const swapped = [...levels];
          [swapped[i - 1], swapped[i]] = [swapped[i], swapped[i - 1]];
          return { label: swapped.join(" › "), text: setSelect({ ...select, group_by: swapped }) };
        });
        items.push(swap);
      }
      items.push(word(level, () => setSelect({ ...select, group_by: levels.filter((l) => l !== level) })));
    });
    lines.push(h("div", { className: "line" }, keyword("group-by"), items));
  }

  if (parts.render) {
    lines.push(h("div", { className: "line" }, keyword("render"), plain(render.formats.join(", ")),
      render.options.map((o) => word(`${o.key}: ${o.value.replace(/\s+/g, " ")}`, () => setRender(withOption(render, o.key, ""))))));
  }
  root.replaceChildren(...lines);
}

// ——— The proposals: the views one word away, those that change the drawing ———

// The views one word away from the one composed: a kind, a layer, a level, the links lifted…
function candidates(parts) {
  const facts = state.facts;
  const select = selectOf(parts);
  const edges = edgesOf(parts);
  const render = renderOf(parts);
  const list = [];
  // The groups that change the drawing shown.
  const groups = GROUPS_OF[current.format];
  const add = (group, label, text, test) => { if (text !== state.text && (!groups || groups.includes(group))) list.push({ group, label, text, test }); };
  const short = (id) => id.split(".").pop();
  const setSelect = (s, text) => withClause("select", selectText(s), text);
  const setEdges = (e, text) => withClause("edges", edgesText(e), text);
  const setRender = (r) => withClause("render", renderText(r, parts.indent));

  for (const { kind, within } of facts.kinds) {
    if (!within && !select.kinds.includes(kind)) add("show", kind, setSelect({ ...select, kinds: [...select.kinds, kind] }), "nodes");
  }
  for (const { layer } of facts.layers) {
    if (!edges.main.layers.includes(layer)) add("link", layer, setEdges({ ...edges, main: { ...edges.main, layers: [...edges.main.layers, layer] } }), "edges");
  }
  if (!select.with_ends && select.kinds.length) add("link", t("their neighbours"), setSelect({ ...select, with_ends: true }), "nodes");
  // The processes: the links of one more, or another one in place of the one drawn.
  const processes = facts.resources.filter((r) => r.kind === "process");
  const drawn = edges.others.findIndex((s) => /^process\(\s*[^)]+\)$/.test(s));
  for (const process of processes) {
    const selector = `process(${process.id})`;
    // The one drawn, written by its name or its qualified name.
    if (edges.others.some((s) => { const name = s.match(/^process\(\s*([^)\s]+)\s*\)$/)?.[1]; return name && (name === process.id || process.id.endsWith(`.${name}`)); })) continue;
    if (drawn < 0) add("link", t("process {name}", { name: process.label ?? short(process.id) }), setEdges({ ...edges, others: [...edges.others, selector] }), "edges");
    else add("process", process.label ?? short(process.id), setEdges({ ...edges, others: edges.others.map((s, i) => (i === drawn ? selector : s)) }), "process");
  }
  // The levels of frames: the systems and the sites first, the usual ones.
  const levels = [...new Set(["system", "site", ...facts.containers, ...facts.references])]
    .filter((l) => (facts.containers.includes(l) || facts.references.includes(l)) && !select.group_by.includes(l) && !select.kinds.includes(l));
  for (const level of levels) {
    if (select.kinds.length) add("frame", level, setSelect({ ...select, group_by: [...select.group_by, level] }), "frames");
  }
  // Only a part of what is drawn: one of its systems, one of its sites.
  const nodes = Object.values(state.insight?.nodes ?? {});
  for (const [axis, label] of [["system", "system {name}"], ["site", "site {name}"]]) {
    const counts = new Map();
    for (const node of nodes) if (node[axis] && node.kind !== axis) counts.set(node[axis], (counts.get(node[axis]) ?? 0) + 1);
    const parts_ = [...counts].filter(([, n]) => n < nodes.length).sort((a, b) => b[1] - a[1]).slice(0, 8);
    for (const [id] of parts_) add("only", t(label, { name: short(id) }), setSelect(withCondition(select, `ancestor(${axis}) = ${id}`)), "only");
  }
  if (!select.without_isolated && select.kinds.length) add("simplify", t("without the isolated ones"), setSelect({ ...select, without_isolated: true }), "fewer");
  if (edges.main.layers.length) {
    for (const [via, label] of LIFTS) {
      if (liftVia(edges.main.lift) !== via) add("simplify", t(label), setEdges({ ...edges, main: { ...edges.main, lift: withVia(edges.main.lift, via) } }), "lifted");
    }
  }
  // The view from above: the systems, or the sites, as nodes, the links lifted onto them.
  for (const kind of ["system", "site"]) {
    if (!facts.kinds.some((k) => k.kind === kind) || (select.kinds[0] === kind && liftVia(edges.main.lift) === `ancestor(${kind})`)) continue;
    const kept = select.kinds.filter((k) => ["saas", "actor", "internet"].includes(k));
    const above = setSelect({ kinds: [kind, ...kept], where: null, with_ends: false, without_isolated: false, group_by: select.group_by.filter((l) => l !== kind) });
    const layers = edges.main.layers.length ? edges.main.layers : ["flow"];
    add("simplify", t("overview by {kind}", { kind }), setEdges({ main: { layers, lift: `via: ancestor(${kind})` }, others: [], options: edges.options }, above), "overview");
  }
  // Another drawing of the same view: those it has something for.
  const others = render.formats.filter((f) => !DRAWINGS.some(([format]) => format === f));
  for (const [format, label] of DRAWINGS) {
    const can = format === "svg" || (state.insight?.draws ?? []).includes(format);
    if (can && format !== current.format) add("draw", t(label), setRender({ ...render, formats: [format, ...others] }), "always");
  }
  if (current.format === "svg") {
    if (option(render, "orientation") !== "data") add("draw", t("in the direction of the data"), setRender(withOption(render, "orientation", "data")), "draw");
    if (option(render, "legend") !== "true") add("draw", t("legend"), setRender(withOption(render, "legend", "true")), "draw");
  }
  return list;
}

// What a proposal changes, in a few signs: `+4 · 6 links`.
function effectOf(v, base) {
  const signs = [];
  if (v.added_nodes) signs.push(`+${v.added_nodes}`);
  if (v.removed_nodes) signs.push(`−${v.removed_nodes}`);
  const links = v.added_edges - v.removed_edges;
  const count = Math.abs(links) || v.added_edges;
  if (v.added_edges || v.removed_edges) signs.push(`${links > 0 ? "+" : links < 0 ? "−" : "±"}${count} ${t(count > 1 ? "links" : "link")}`);
  if (v.frames !== base.frames) signs.push(`${v.frames} ${t(v.frames > 1 ? "frames" : "frame")}`);
  return signs.join(" · ");
}

// Evaluates the candidates with the view, in one build, and keeps those worth proposing.
function propose() {
  const parts = view();
  if (!parts || !state.facts || !current || current.result.summary.errors) {
    state.proposals = [];
    return proposals();
  }
  const list = candidates(parts);
  const file = [state.text, ...list.map((c, i) => splice(c.text, parts.name_range, `${CANDIDATE}${i}`))].join("\n");
  let answer;
  try {
    answer = build({ files: projectFiles(parts.name, file), view: parts.name, formats: [], variants: parts.name }).variants;
  } catch {
    answer = null;
  }
  if (!answer) {
    state.proposals = [];
    return proposals();
  }
  const base = answer.base;
  const kept = [];
  list.forEach((candidate, i) => {
    const v = answer.variants[`${CANDIDATE}${i}`];
    if (!v?.ok) return;
    const worth = {
      nodes: v.added_nodes > 0,
      edges: v.added_edges > 0,
      frames: v.frames > base.frames && v.nodes > 0,
      fewer: v.removed_nodes > 0 && v.nodes > 0,
      lifted: v.edges > 0 && v.added_edges + v.removed_edges > 0,
      overview: v.nodes > 1 && v.edges > 0,
      only: v.removed_nodes > 0 && v.nodes > 0,
      process: v.nodes > 0,
      draw: base.edges > 0,
      always: true,
    }[candidate.test];
    const effect = { draw: "", always: "", process: "", only: String(v.nodes) }[candidate.test] ?? effectOf(v, base);
    if (worth) kept.push({ ...candidate, effect, weight: v.added_edges * 1000 + v.added_nodes });
  });
  // What brings links first: the neighbours of what is drawn before what stands apart.
  for (const group of ["show", "link"]) {
    const ranked = kept.filter((c) => c.group === group).sort((a, b) => b.weight - a.weight);
    let at = 0;
    for (let i = 0; i < kept.length; i++) if (kept[i].group === group) kept[i] = ranked[at++];
  }
  state.proposals = kept;
  proposals();
}

function proposals() {
  const root = $("proposals");
  if (!view()) return root.replaceChildren();
  const groups = GROUPS.map(([group, title]) => {
    const items = state.proposals.filter((p) => p.group === group);
    if (!items.length) return null;
    const whole = state.unfolded.has(group) || items.length <= SHOWN + 1;
    const chips = (whole ? items : items.slice(0, SHOWN)).map((p) => {
      const chip = h("button", { className: "proposal" }, p.label, p.effect ? h("small", { textContent: p.effect }) : null);
      offer(chip, () => p);
      return chip;
    });
    if (!whole) {
      chips.push(h("button", { className: "proposal more", textContent: t("{n} more", { n: items.length - SHOWN }), onclick: () => { state.unfolded.add(group); proposals(); } }));
    }
    return h("div", { className: "group" }, h("b", { textContent: t(title) }), chips);
  }).filter(Boolean);
  root.replaceChildren(h("div", { className: "hint", textContent: t(groups.length ? "Point at a word to see what it changes, click to take it." : "Nothing more to propose: the code says the rest.") }), ...groups);
}

// ——— Looking at a proposal: its drawing, what it adds outlined; taking it, or leaving ———

let looking = null;
let lookTimer = null;

// Makes `element` offer a view (`of()` gives its `{ label, text, effect }`): pointed at or
// focused, it is drawn; clicked, taken.
function offer(element, of) {
  const enter = () => {
    clearTimeout(lookTimer);
    lookTimer = setTimeout(() => look(of()), 140);
  };
  element.addEventListener("pointerenter", (e) => { if (e.pointerType !== "touch") enter(); });
  element.addEventListener("focus", enter);
  element.addEventListener("pointerleave", leave);
  element.addEventListener("blur", leave);
  element.addEventListener("click", () => {
    clearTimeout(lookTimer);
    const proposal = of();
    looking = null;
    setStage(null);
    apply(proposal.text);
  });
}

function look(proposal) {
  if (!current || looking === proposal.text) return;
  let drawing;
  try {
    drawing = drawingOf(proposal.text);
  } catch {
    return;
  }
  looking = proposal.text;
  // What it adds: its nodes that the view composed does not draw.
  const drawn = new Set(Object.values(state.insight?.nodes ?? {}).map((n) => n.id));
  const added = new Set(Object.entries(drawing.result.insight?.nodes ?? {}).filter(([, n]) => !drawn.has(n.id)).map(([key]) => key));
  show(drawing, false, added);
  setStage(`${t("Preview: {label}", { label: proposal.label })}${proposal.effect ? ` (${proposal.effect})` : ""}`);
}

function leave() {
  clearTimeout(lookTimer);
  if (!looking) return;
  looking = null;
  setStage(null);
  if (current) show(current, true);
}

function setStage(text) {
  $("stage").classList.toggle("previewing", Boolean(text));
  $("previewing").hidden = !text;
  $("previewing").textContent = text ?? "";
}

// ——— The menus of the diagram: a node or an edge clicked, what it can change ———

let pressed = null;
function menuOn(event) {
  if (looking) return;
  const item = event.target.closest("g.node, g.edge");
  const key = item?.dataset.compose ?? "";
  const node = key.match(/^n(\d+)/);
  const label = key.match(/^e(\d+)$/);
  const edge = item?.id.match(/^graph-edge-(\d+)$/) ?? label;
  const parts = view();
  if (!parts || !state.insight) return hideMenu();
  const actions = [];
  let head;
  const select = selectOf(parts);
  const setSelect = (s) => apply(withClause("select", selectText(s)));
  if (node && !label) {
    const info = state.insight.nodes[`n${node[1]}`];
    if (!info) return hideMenu();
    const short = info.label ?? info.id.split(".").pop();
    head = [short, `${info.kind} · ${info.id}`];
    actions.push([t("Hide {name}", { name: short }), () => setSelect(withCondition(select, `name != ${info.id}`))]);
    actions.push([t("Only {name} and its neighbours", { name: short }), () => setSelect({
      ...select, kinds: select.kinds.includes(info.kind) || !select.kinds.length ? select.kinds : [...select.kinds, info.kind], where: `name = ${info.id}`, with_ends: true,
    })]);
    if (info.system && info.kind !== "system") actions.push([t("Only the system {name}", { name: info.system }), () => setSelect(withCondition(select, `ancestor(system) = ${info.system}`))]);
    else if (info.site && info.kind !== "site") actions.push([t("Only the site {name}", { name: info.site }), () => setSelect(withCondition(select, `ancestor(site) = ${info.site}`))]);
    if (project?.site) actions.push([t("Its page on the site"), `${project.site}resources/${info.id.replace(/[^A-Za-z0-9._-]/g, "_")}.html`]);
  } else if (edge) {
    const info = state.insight.edges[Number(edge[1])];
    if (!info) return hideMenu();
    head = [t("an edge"), info.layers.join(", ")];
    const edges = edgesOf(parts);
    for (const layer of info.layers.filter((l) => edges.main.layers.includes(l))) {
      actions.push([t("Without the layer {layer}", { layer }), () => apply(withClause("edges", edgesText({ ...edges, main: { ...edges.main, layers: edges.main.layers.filter((l) => l !== layer) } })))]);
    }
    if (!actions.length) return hideMenu();
  } else {
    return hideMenu();
  }
  const menu = $("menu");
  menu.replaceChildren(h("div", { className: "head" }, h("b", { textContent: head[0] }), h("small", { textContent: head[1] })),
    ...actions.map(([text, action]) => (typeof action === "string"
      ? h("a", { href: action, target: "_blank", textContent: text })
      : h("button", { textContent: text, onclick: action }))));
  const box = menu.parentElement.getBoundingClientRect();
  menu.hidden = false;
  const x = Math.min(event.clientX - box.left + 8, box.width - menu.offsetWidth - 8);
  const y = Math.min(event.clientY - box.top + 8, box.height - menu.offsetHeight - 8);
  menu.style.left = `${Math.max(8, x)}px`;
  menu.style.top = `${Math.max(8, y)}px`;
}
function hideMenu() {
  $("menu").hidden = true;
}

// ——— The code ———

// The text colored with the module's highlighter, the range of the last change marked.
function colored(text, mark) {
  const pieces = [];
  const piece = (start, end, className) => {
    const cuts = [start, end, ...(mark ?? []).filter((b) => b > start && b < end)].sort((a, b) => a - b);
    for (let i = 0; i + 1 < cuts.length; i++) {
      const [a, b] = [cuts[i], cuts[i + 1]];
      if (a >= b) continue;
      let node = className ? h("span", { className, textContent: text.slice(a, b) }) : document.createTextNode(text.slice(a, b));
      if (mark && a >= mark[0] && b <= mark[1]) node = h("mark", {}, node);
      pieces.push(node);
    }
  };
  let at = 0;
  for (const [start, end, name] of cadastre.highlight(text)) {
    if (start < at || start >= end) continue;
    if (start > at) piece(at, start, "");
    const segments = name.split(".");
    piece(start, Math.min(end, text.length), segments.map((_, i) => `hl-${segments.slice(0, i + 1).join("-")}`).join(" "));
    at = end;
  }
  if (at < text.length) piece(at, text.length, "");
  return pieces;
}

function paintCode() {
  $("highlight").replaceChildren(...colored($("code").value, changed), "\n");
  syncScroll();
}

function syncScroll() {
  const [editor, target] = [$("code"), $("highlight")];
  const style = getComputedStyle(editor);
  target.style.paddingBottom = `${parseFloat(style.paddingBottom) + editor.offsetHeight - editor.clientHeight}px`;
  target.style.paddingRight = `${parseFloat(style.paddingRight) + editor.offsetWidth - editor.clientWidth}px`;
  target.scrollTop = editor.scrollTop;
  target.scrollLeft = editor.scrollLeft;
}

function toggleCode(shown = $("codebox").hidden) {
  $("codebox").hidden = !shown;
  $("toggle-code").classList.toggle("on", shown);
  if (shown) paintCode();
}

// ——— Starting points: a recipe, a view of the project, a link ———

function recipes(all = false) {
  const list = $("recipe-list");
  const resources = state.facts.resources;
  const known = Object.keys(RECIPES);
  const offered = state.facts.recipes.slice().sort((a, b) => known.indexOf(a.id) - known.indexOf(b.id))
    .filter((recipe) => !recipe.asks || resources.some((r) => recipe.asks === "any" || r.kind === recipe.asks));
  list.replaceChildren(...(all ? offered : offered.slice(0, FIRST_RECIPES)).map((recipe) => {
    const [title, text] = RECIPES[recipe.id] ?? [recipe.id, ""];
    const among = recipe.asks ? resources.filter((r) => recipe.asks === "any" || r.kind === recipe.asks) : [];
    const input = recipe.asks ? h("input", { type: "text", placeholder: t("Pick a resource…") }) : null;
    input?.setAttribute("list", `recipe-${recipe.id}`);
    const options = recipe.asks ? h("datalist", { id: `recipe-${recipe.id}` }, among.slice(0, 2000).map((r) => h("option", { value: r.id, label: r.label ?? r.kind }))) : null;
    // The other end of a path: any resource, from the same list.
    const to = recipe.asks_to ? h("input", { type: "text", placeholder: t("To…") }) : null;
    to?.setAttribute("list", `recipe-${recipe.id}`);
    const create = () => {
      const resource = recipe.asks ? among.find((r) => r.id === input.value.trim()) : null;
      if (recipe.asks && !resource) return input.focus();
      const other = recipe.asks_to ? among.find((r) => r.id === to.value.trim()) : null;
      if (recipe.asks_to && !other) return to.focus();
      $("recipes").close();
      fromRecipe(recipe, resource, other);
    };
    for (const field of [input, to]) if (field) field.onkeydown = (e) => { if (e.key === "Enter") create(); };
    return h("div", { className: "recipe" }, h("h3", { textContent: t(title, { x: recipe.asks ? "…" : "", y: "…" }) }), h("p", { textContent: t(text) }),
      h("div", { className: "ends" }, input, to), options,
      h("button", { className: "primary", textContent: t("Create"), onclick: create }));
  }));
  $("recipes-footer").replaceChildren(
    !all && offered.length > FIRST_RECIPES ? h("button", { textContent: t("More recipes"), onclick: () => recipes(true) }) : null,
    h("span", { className: "spacer" }), t("Or start from nothing:"),
    h("button", { textContent: t("An empty view"), onclick: () => { $("recipes").close(); fromBody("new-view", "", "select app\nedges  [flow]\nrender svg"); } }));
  if (!$("recipes").open) $("recipes").showModal();
}

function fromRecipe(recipe, resource, to = null) {
  const body = recipe.body.replaceAll("{x}", resource?.id ?? "").replaceAll("{kind}", resource?.kind ?? "").replaceAll("{y}", to?.id ?? "");
  const short = (r) => (r ? r.id.split(".").pop() : "");
  const title = t(RECIPES[recipe.id]?.[0] ?? recipe.id, { x: resource ? resource.label ?? short(resource) : "", y: to ? to.label ?? short(to) : "" });
  fromBody([recipe.id, short(resource), short(to)].filter(Boolean).join("-"), title, body);
}

function fromBody(base, title, body) {
  let name = base.replace(/[^A-Za-z0-9_-]/g, "-");
  for (let i = 2; state.views.includes(name); i++) name = `${base}-${i}`;
  const lines = [...(title ? [`title: ${quoted(title)}`] : []), ...body.split("\n")];
  state.origin = null;
  changed = null;
  zoomState = null;
  setText(`view ${name} {\n${lines.map((l) => `  ${l}`).join("\n")}\n}`);
}

// A view of the project, to edit: its block, and where it is written.
function edit(name) {
  for (const file of state.files.filter((f) => f.path.endsWith(".cav"))) {
    const found = cadastre.parts(file.text).find((v) => v.name === name);
    if (found) {
      state.origin = { path: file.path, name };
      changed = null;
      zoomState = null;
      setText(file.text.slice(found.range[0], found.range[1]));
      return true;
    }
  }
  return false;
}

// ——— The end: the view named, and kept by its file, its code or its link ———

function finish() {
  const parts = view();
  if (!parts) return;
  const attribute = (key) => parts.attributes[key]?.value ?? "";
  // A text input that changes the view once left or validated, not at each key.
  const input = (value, onChange, props = {}) => {
    const element = h("input", { type: "text", value: value ?? "", ...props });
    element.onchange = () => { onChange(element.value.trim()); finish(); };
    element.onkeydown = (e) => { if (e.key === "Enter") element.blur(); };
    return element;
  };
  const description = h("textarea", { value: attribute("description"), rows: 2 });
  description.onchange = () => { apply(withAttribute("description", description.value.trim())); finish(); };
  // A new view saved: the file it is written in, one of the project or a new one.
  const target = () => {
    if (!project?.edit || state.origin) return [];
    const files = state.files.filter((f) => f.path.endsWith(".cav")).map((f) => f.path);
    state.target ??= files[0] ?? "views.cav";
    const file = h("input", { type: "text", value: state.target, title: t("The .cav file to write the view in") });
    file.setAttribute("list", "end-files");
    file.oninput = () => { state.target = file.value.trim(); };
    return [h("span", { textContent: t("File") }), h("span", {}, file, h("datalist", { id: "end-files" }, files.map((f) => h("option", { value: f }))))];
  };
  $("end-form").replaceChildren(
    h("span", { textContent: t("Name") }), input(parts.name, (v) => { if (/^[A-Za-z_][A-Za-z0-9_-]*$/.test(v)) apply(splice(state.text, parts.name_range, v)); }),
    h("span", { textContent: t("Title") }), input(attribute("title"), (v) => apply(withAttribute("title", v))),
    h("span", { textContent: t("Description") }), description,
    h("span", { textContent: t("Folder") }), input(attribute("group"), (v) => apply(withAttribute("group", v)), { placeholder: t("Network/Security") }),
    ...target());
  $("end-code").replaceChildren(...colored(state.text, null));
  $("save").hidden = !project?.edit;
  $("save").textContent = state.origin ? t("Save in {file}", { file: state.origin.path }) : t("Save");
  if (!$("end").open) {
    $("end-status").textContent = "";
    $("end").showModal();
  }
}

let linked = "";
async function remember() {
  const params = new URLSearchParams();
  if (state.example) params.set("example", state.example);
  if (state.origin) params.set("view", state.origin.name);
  params.set("q", await pack(state.text));
  linked = `#${params}`;
  if (linked !== location.hash) history.replaceState(null, "", linked);
  const site = $("site");
  if (project?.site) {
    site.href = state.origin ? `${project.site}views/${state.origin.name.replace(/[^A-Za-z0-9._-]/g, "_")}.html` : `${project.site}index.html`;
    site.textContent = t(state.origin ? "This view on the site" : "Site");
  }
}

async function pack(text) {
  const stream = new Blob([text]).stream().pipeThrough(new CompressionStream("deflate-raw"));
  const bytes = new Uint8Array(await new Response(stream).arrayBuffer());
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

async function unpack(value) {
  const bytes = Uint8Array.from(atob(value.replace(/-/g, "+").replace(/_/g, "/")), (c) => c.charCodeAt(0));
  const stream = new Blob([bytes]).stream().pipeThrough(new DecompressionStream("deflate-raw"));
  return new Response(stream).text();
}

function status(text, kind = "") {
  $("status").textContent = text;
  $("status").className = kind;
}

async function copy(text, button, label) {
  try {
    await navigator.clipboard.writeText(text);
    button.textContent = t("Copied");
    setTimeout(() => { button.textContent = t(label); }, 1500);
  } catch {
    // Without the clipboard (a page served over plain HTTP, an editor): the text to copy by hand,
    // where the page can ask.
    try {
      prompt(t(label), text);
    } catch {
      // The code is in sight in the dialog.
    }
  }
}

// `cadastre serve --edit`: the view written in its file, the one it edits replaced.
async function save() {
  const parts = view();
  if (!parts) return;
  const path = state.origin?.path ?? state.target;
  if (!path) return;
  const told = $("end-status");
  try {
    const response = await fetch(`${project.site ?? "../"}__edit`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ path, replaces: state.origin?.name ?? null, text: state.text }),
    });
    const answer = await response.json().catch(() => ({ error: response.statusText }));
    if (!response.ok) throw new Error(answer.error ?? response.statusText);
    // The file as written: the project, and the view it now edits.
    const file = state.files.find((f) => f.path === path);
    if (file) file.text = answer.text; else state.files.push({ path, text: answer.text });
    state.origin = { path, name: parts.name };
    state.views = [...new Set([...state.views, parts.name])];
    told.textContent = t("Saved in {file}", { file: path });
    told.style.color = "var(--ok)";
    $("save").textContent = t("Save in {file}", { file: path });
    remember();
  } catch (error) {
    told.textContent = t("Cannot save: {error}", { error: error.message });
    told.style.color = "var(--error)";
  }
}

// ——— Loading ———

function script(src) {
  return new Promise((resolve, reject) => {
    document.head.append(Object.assign(document.createElement("script"), { src, onload: resolve, onerror: reject }));
  });
}

async function graphviz() {
  try {
    await script("./viz-standalone.js");
  } catch {
    await script(VIZ_CDN);
  }
  return Viz.instance();
}

async function published() {
  try {
    const response = await fetch("project.json", { cache: "no-store" });
    return response.ok ? await response.json() : null;
  } catch {
    return null;
  }
}

async function example(name) {
  return Promise.all(EXAMPLES[name].map(async (path) => {
    for (const base of ["examples", "../examples"]) {
      const response = await fetch(`${base}/${name}/${path}`);
      if (response.ok) return { path, text: await response.text() };
    }
    return { path, text: "" };
  }));
}

// The facts of the project and its views, once: what the proposals are made of.
function learn() {
  const result = build({ files: state.files, view: "\u0000", formats: [], facts: true });
  state.facts = result.facts;
  state.views = result.views;
}

// What the address asks: a view of the project to edit, a recipe around a resource, a view
// shared by its link, or nothing yet (the recipes).
async function follow() {
  const params = new URLSearchParams(location.hash.slice(1));
  if (params.get("q")) {
    try {
      const text = await unpack(params.get("q"));
      const name = params.get("view");
      state.origin = name ? state.files.filter((f) => f.path.endsWith(".cav") && cadastre.parts(f.text).some((v) => v.name === name))
        .map((f) => ({ path: f.path, name }))[0] ?? null : null;
      changed = null;
      return setText(text);
    } catch {
      // A truncated link: as if there were none.
    }
  }
  if (params.get("view") && edit(params.get("view"))) return;
  const around = params.get("around");
  const resource = around && state.facts.resources.find((r) => r.id === around);
  if (resource) return fromRecipe(state.facts.recipes.find((r) => r.id === "around"), resource);
  fromBody("new-view", "", "select app\nedges  [flow]\nrender svg");
  recipes();
}

function applyLanguage() {
  document.documentElement.lang = lang;
  document.title = `${t("Compose a view")} · Cadastre`;
  for (const [id, text] of [["start", "Recipes…"], ["toggle-code", "Code"], ["finish", "Finish…"], ["copy", "Copy the code"], ["share", "Copy the link"],
    ["save", "Save"], ["recipes-title", "Start from a recipe"], ["end-title", "Keep the view"]]) {
    $(id).textContent = t(text);
  }
  $("example-label").firstElementChild.textContent = t("Project");
  $("code").setAttribute("aria-label", t("Code of the view"));
  $("sentence").setAttribute("aria-label", t("The view"));
  $("proposals").setAttribute("aria-label", t("Proposals"));
}

async function open(files) {
  state.files = files;
  state.unfolded.clear();
  learn();
  await follow();
}

// ——— Events ———

$("code").addEventListener("input", () => {
  state.text = $("code").value;
  changed = null;
  paintCode();
  hideMenu();
  clearTimeout(timer);
  timer = setTimeout(() => { sentence(); run(); }, 300);
});
$("code").addEventListener("scroll", syncScroll);
$("code").addEventListener("keydown", (e) => {
  if (e.key !== "Tab" || e.shiftKey || e.ctrlKey || e.altKey || e.metaKey) return;
  e.preventDefault();
  document.execCommand("insertText", false, "  ");
});
// A click on the diagram: a menu, never the link of a node; a drag moves the drawing.
$("output").addEventListener("pointerdown", (e) => { pressed = [e.clientX, e.clientY]; });
$("output").addEventListener("click", (e) => {
  if (e.target.closest("a")) e.preventDefault();
  if (pressed && Math.hypot(e.clientX - pressed[0], e.clientY - pressed[1]) > 4) return;
  menuOn(e);
}, true);
document.addEventListener("keydown", (e) => { if (e.key === "Escape") hideMenu(); });
document.addEventListener("click", (e) => { if (!e.target.closest("#menu, #output")) hideMenu(); });
$("start").addEventListener("click", () => recipes());
$("recipes-close").addEventListener("click", () => $("recipes").close());
$("toggle-code").addEventListener("click", () => toggleCode());
$("status").addEventListener("click", () => { if ($("status").className === "ko") toggleCode(true); });
$("finish").addEventListener("click", finish);
$("end-close").addEventListener("click", () => $("end").close());
$("copy").addEventListener("click", () => copy(state.text, $("copy"), "Copy the code"));
$("share").addEventListener("click", async () => { await remember(); copy(location.href, $("share"), "Copy the link"); });
$("save").addEventListener("click", save);
$("example").replaceChildren(...Object.keys(EXAMPLES).map((name) => new Option(name, name)));
$("example").addEventListener("change", async (e) => {
  state.example = e.target.value;
  history.replaceState(null, "", `#example=${state.example}`);
  await open(await example(state.example));
});
window.addEventListener("hashchange", () => { if (location.hash !== linked) follow(); });

status(t("loading…"));
[cadastre, viz, project] = await Promise.all([load(new URL("./cadastre_wasm.wasm", import.meta.url)), graphviz(), published()]);
const wanted = project?.language ?? navigator.language ?? "en";
lang = Object.keys(LANGUAGES).find((code) => wanted.toLowerCase().startsWith(code)) ?? "en";
applyLanguage();
status("");
const handed = (() => {
  try {
    return JSON.parse(sessionStorage.getItem(HANDOVER) ?? "null");
  } catch {
    return null;
  }
})();
if (project) {
  // A project published with the compiler: its files; saved in them under `cadastre serve --edit`.
  $("example-label").hidden = true;
  state.modules = project.modules ?? [];
  state.satellites = project.satellites ?? false;
  if (project.site) {
    $("title").replaceChildren(h("a", { href: `${project.site}index.html`, textContent: project.name }));
    $("site").hidden = false;
  } else {
    $("title").textContent = project.name;
  }
  // Edited in the playground of this tab: those files, not the published ones.
  await open(handed && new URLSearchParams(location.hash.slice(1)).has("local") ? handed.files : project.files);
} else if (handed && new URLSearchParams(location.hash.slice(1)).has("local")) {
  // A folder opened in the playground of this tab.
  $("example-label").hidden = true;
  state.modules = handed.modules ?? [];
  await open(handed.files);
} else {
  state.example = new URLSearchParams(location.hash.slice(1)).get("example") ?? "reference";
  if (!(state.example in EXAMPLES)) state.example = "reference";
  $("example").value = state.example;
  await open(await example(state.example));
}
document.documentElement.dataset.ready = "true";
