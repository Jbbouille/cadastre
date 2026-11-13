// The composer of views: a view written with a panel and on its diagram, its code kept in sight.
//
// The code of the view is the source of truth: the panel, the menus of the diagram and the
// recipes rewrite one clause of it at a time (the compiler gives each clause with where it is
// written: `cadastre.parts`), the rest of the text left as it is; editing the code updates the
// panel. The view is built with the project at each change, in place of the view it edits.
import { load } from "./cadastre.js";

const VIZ_CDN = "https://cdn.jsdelivr.net/npm/@viz-js/viz@3.11.0/lib/viz-standalone.js";
// The examples, read beside the page (a self-hosted playground) or from the repository.
const EXAMPLES = {
  tutorial: ["registry.ca", "views.cav", "checks.cac"],
  reference: ["schema.ca", "teams.ca", "infra.ca", "data.ca", "apps.ca", "saas.ca", "views.cav", "checks.cac"],
  retail: ["schema.ca", "teams.ca", "infra.ca", "product.ca", "data.ca", "saas.ca", "systems.ca", "views.cav"],
};
// The file the view composed is built in when it is new.
const COMPOSED = "composed.cav";
// A view handed over by the playground (an opened folder): its files, for this tab only.
const HANDOVER = "cadastre-compose";

// The words of the page in the language of the reader. English is the key.
const LANGUAGES = {
  fr: {
    "Compose a view": "Composer une vue",
    "Project": "Projet",
    "Start from a recipe…": "Partir d'une recette…",
    "Start from a recipe": "Partir d'une recette",
    "Copy the code": "Copier le code",
    "Copy the link": "Copier le lien",
    "Copied": "Copié",
    "Save": "Enregistrer",
    "Save in {file}": "Enregistrer dans {file}",
    "Saved in {file}": "Enregistré dans {file}",
    "Cannot save: {error}": "Enregistrement impossible : {error}",
    "The file to write the view in:": "Le fichier où écrire la vue :",
    "Site": "Site",
    "This view on the site": "Cette vue sur le site",
    "Code of the view": "Code de la vue",
    "loading…": "chargement…",
    "Diagram": "Diagramme", "Table": "Tableau", "Text": "Texte", "Sequence": "Séquence", "Traversal": "Traversée", "Racks": "Baies",
    "Click a node or an edge to change the view": "Cliquez un nœud ou une arête pour changer la vue",
    "Write a view: view name { … }": "Écrivez une vue : view nom { … }",
    "Fix the errors to draw the view.": "Corrigez les erreurs pour dessiner la vue.",
    "Nothing to draw.": "Rien à dessiner.",
    "No diagnostic.": "Aucun diagnostic.",
    "error": "erreur", "warning": "avertissement", "info": "info", "incomplete": "incomplet", "help:": "aide :",
    // The panel.
    "The view": "La vue",
    "Name": "Nom", "Title": "Titre", "Description": "Description", "Folder": "Dossier",
    "Network/Security": "Réseau/Sécurité",
    "What": "Quoi",
    "the kinds drawn": "les types dessinés",
    "Add a kind…": "Ajouter un type…",
    "with their neighbours (with ends)": "avec leurs voisins (with ends)",
    "without the isolated ones": "sans les isolés",
    "Filter": "Filtre",
    "which of them": "lesquels",
    "Add": "Ajouter",
    "value": "valeur",
    "Links": "Liens",
    "the edges between them": "les arêtes entre eux",
    "Lifted onto": "Remontés sur",
    "themselves": "eux-mêmes",
    "their server (on)": "leur serveur (on)",
    "their system": "leur système",
    "their site": "leur site",
    "their subnet": "leur sous-réseau",
    "Other selectors": "Autres sélecteurs",
    "follow(data), process(name), path(a, b)…": "follow(donnée), process(nom), path(a, b)…",
    "Frames": "Cadres",
    "group-by, from the broadest": "group-by, du plus large",
    "Add a level…": "Ajouter un niveau…",
    "Drawing": "Rendu",
    "the formats and their options": "les formats et leurs options",
    "Add a format…": "Ajouter un format…",
    "Direction": "Direction",
    "default": "par défaut",
    "left to right": "gauche à droite", "top to bottom": "haut en bas", "right to left": "droite à gauche", "bottom to top": "bas en haut",
    "in the direction of the data": "dans le sens des données",
    "legend": "légende",
    "a copy of a shared node in each frame": "une copie d'un nœud partagé dans chaque cadre",
    "Node text": "Texte des nœuds",
    "Edge labels": "Étiquettes",
    "Cut at": "Couper à",
    "characters": "caractères",
    "Other options": "Autres options",
    "What it draws": "Ce qu'elle dessine",
    "{nodes} nodes · {edges} edges · {frames} frames": "{nodes} nœuds · {edges} arêtes · {frames} cadres",
    "{count} {layer} link(s) to {kind} out of the view, not drawn: {examples}": "{count} lien(s) {layer} vers {kind} hors de la vue, non dessiné(s) : {examples}",
    "Links lost because one of their ends is not selected:": "Des liens perdus parce qu'une de leurs extrémités n'est pas sélectionnée :",
    "Add {kind}": "Ajouter {kind}",
    "Add the ends (with ends)": "Ajouter les extrémités (with ends)",
    "{nodes} nodes and no frame: grouped, it reads better.": "{nodes} nœuds et aucun cadre : regroupée, elle se lit mieux.",
    "Group by {level}": "Regrouper par {level}",
    // The menus of the diagram.
    "Hide {name}": "Masquer {name}",
    "Only {name} and its neighbours": "Seulement {name} et ses voisins",
    "Only the system {name}": "Seulement le système {name}",
    "Only the site {name}": "Seulement le site {name}",
    "Without the kind {kind}": "Sans le type {kind}",
    "Its page on the site": "Sa page sur le site",
    "Only the layer {layer}": "Seulement la couche {layer}",
    "Without the layer {layer}": "Sans la couche {layer}",
    "an edge": "une arête",
    // The recipes.
    "Pick a resource…": "Choisir une ressource…",
    "Create": "Créer",
    "Or start from nothing: an empty view.": "Ou partir de rien : une vue vide.",
    "An empty view": "Une vue vide",
    "What {x} exchanges with": "Ce qu'échange {x}",
    "The resource and the ends of its flows, in the direction of the data.": "La ressource et les extrémités de ses flux, dans le sens des données.",
    "The flows of {x}": "Les flux de {x}",
    "Its apps, jobs, databases and SaaS, the flows between them and with the rest.": "Ses applications, traitements, bases et SaaS, leurs flux entre eux et avec le reste.",
    "Landscape": "Paysage",
    "The flows between the systems and the SaaS, lifted from those of their apps.": "Les flux entre les systèmes et les SaaS, remontés de ceux de leurs applications.",
    "The network of {x}": "Le réseau de {x}",
    "Its servers by subnet, the application flows lifted onto them.": "Ses serveurs par sous-réseau et ceux avec qui ils échangent, les flux applicatifs remontés sur eux.",
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
const RECIPES = {
  around: ["What {x} exchanges with", "The resource and the ends of its flows, in the direction of the data."],
  "system-flows": ["The flows of {x}", "Its apps, jobs, databases and SaaS, the flows between them and with the rest."],
  landscape: ["Landscape", "The flows between the systems and the SaaS, lifted from those of their apps."],
  "site-network": ["The network of {x}", "Its servers by subnet and those they exchange with, the application flows lifted onto them."],
  "personal-data": ["Personal data", "The flows carrying personal data, with what they carry."],
  "data-trail": ["The trail of {x}", "Where the data goes, link after link, from where it lives."],
  failure: ["If {x} fails", "What becomes unavailable with it, in red."],
  firewall: ["What crosses {x}", "The flows through it, merged by port: its rule table."],
  ownership: ["Ownership", "The apps, databases and SaaS by owner team, with their operator."],
  inventory: ["Inventory", "Each app with its servers and their site, as a table."],
};

// The previews: the format shown, and the formats of a view it stands for.
const PREVIEWS = [["svg", "Diagram"], ["html", "Table"], ["text", "Text"], ["mermaid", "Mermaid"], ["sequence", "Sequence"], ["traversal", "Traversal"], ["racks", "Racks"]];
const PREVIEW_OF = { svg: "svg", dot: "svg", drawio: "svg", png: "svg", html: "html", xlsx: "html", csv: "html", tsv: "html", markdown: "html", text: "text", ascii: "text", mermaid: "mermaid", sequence: "sequence", traversal: "traversal", racks: "racks" };
const FORMATS = ["svg", "html", "xlsx", "csv", "markdown", "text", "mermaid", "sequence", "traversal", "racks", "drawio", "dot"];
// The order of the clauses, where a missing one is written.
const ORDER = ["select", "zones", "edges", "style", "render"];
// What `lift` lifts the ends of the edges onto (`via`).
const LIFTS = [["", "themselves"], ["on", "their server (on)"], ["ancestor(system)", "their system"], ["ancestor(site)", "their site"], ["ancestor(subnet)", "their subnet"]];

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
  // The format previewed; `null`: that of the view.
  format: null,
  insight: null,
};
let cadastre, viz, project, timer;
// The range of the last change, highlighted in the code.
let changed = null;

// ——— Text: the view, clause by clause ———

const view = () => (cadastre ? cadastre.parts(state.text)[0] ?? null : null);
const splice = (text, [start, end], replacement) => text.slice(0, start) + replacement + text.slice(end);
const lineStart = (text, at) => text.lastIndexOf("\n", at - 1) + 1;

// A value written as the language reads it: a name as is, anything else quoted.
const literal = (value) => (/^-?[A-Za-z0-9_][A-Za-z0-9_.\-/:]*$/.test(value) ? value : JSON.stringify(value));
const quoted = (text) => JSON.stringify(text);

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

// The text with the clause `key` (`select`, `edges`, `render`) written as `clause`, removed when empty.
function withClause(key, clause) {
  const parts = view();
  if (!parts) return state.text;
  const current = parts[key];
  if (current?.range) return clause ? splice(state.text, current.range, clause) : remove(state.text, current.range);
  if (!clause) return state.text;
  const next = ORDER.slice(ORDER.indexOf(key) + 1).map((k) => (k === "zones" ? parts.zones[0] : parts[k])).find((p) => p?.range);
  return insert(state.text, parts, next?.range[0], clause);
}

// The text with the attribute `key` (`title`, `group`…) set, removed when empty; a new one goes first.
function withAttribute(key, value, raw = false) {
  const parts = view();
  if (!parts) return state.text;
  const clause = value ? `${key}: ${raw ? value : quoted(value)}` : "";
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
const names = (list) => list.split(",").map((s) => s.trim()).filter(Boolean);

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

// ——— Changes: the text rewritten, the code, the panel and the diagram following ———

function apply(text) {
  const old = state.text;
  if (text === old) return;
  // What changed, highlighted in the code a moment.
  let start = 0;
  while (start < old.length && start < text.length && old[start] === text[start]) start++;
  let [end, oldEnd] = [text.length, old.length];
  while (end > start && oldEnd > start && old[oldEnd - 1] === text[end - 1]) { end--; oldEnd--; }
  changed = [start, end];
  state.text = text;
  $("code").value = text;
  paintCode();
  hideMenu();
  panel();
  schedule(0);
}

function schedule(delay = 250) {
  clearTimeout(timer);
  timer = setTimeout(run, delay);
}

// ——— The build: the project with the view composed in place of the one it edits ———

function projectFiles(name) {
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
  return [...files, { path: COMPOSED, text: `${state.text}\n` }];
}

function build(request) {
  return cadastre.build({ modules: state.modules, satellites: state.satellites, language: lang, ...request });
}

function run() {
  const parts = view();
  const target = $("output");
  if (!parts) {
    state.insight = null;
    target.replaceChildren(empty(t("Write a view: view name { … }")));
    $("diagnostics").replaceChildren();
    insight();
    return;
  }
  const render = renderOf(parts);
  const format = state.format ?? PREVIEW_OF[render.formats.find((f) => PREVIEW_OF[f])] ?? "svg";
  tabs(format);
  let result;
  try {
    result = build({ files: projectFiles(parts.name), view: parts.name, formats: [format], insight: true });
  } catch (error) {
    target.replaceChildren(Object.assign(document.createElement("pre"), { textContent: String(error) }));
    return;
  }
  state.insight = result.insight ?? null;
  const output = result.outputs.find((o) => o.view === parts.name);
  target.replaceChildren();
  if (!output) {
    target.append(empty(t(result.summary.errors ? "Fix the errors to draw the view." : "Nothing to draw.")));
  } else if (["svg", "traversal", "racks"].includes(format) && output.content.trimStart().startsWith("<svg")) {
    drawn(target, output.content);
  } else if (format === "svg" && viz) {
    try {
      drawn(target, cadastre.draw(viz, output.content));
    } catch (error) {
      target.append(Object.assign(document.createElement("pre"), { textContent: String(error) }));
    }
  } else if (format === "html") {
    target.append(Object.assign(document.createElement("iframe"), { srcdoc: output.content, sandbox: "", title: parts.name }));
  } else {
    target.append(Object.assign(document.createElement("pre"), { textContent: output.content }));
  }
  diagnostics(result.diagnostics);
  insight();
  remember();
}

const empty = (text) => Object.assign(document.createElement("div"), { className: "empty", textContent: text });

function tabs(active) {
  const nav = $("formats");
  nav.replaceChildren(...PREVIEWS.map(([format, label]) => {
    const button = Object.assign(document.createElement("button"), { textContent: t(label) });
    button.classList.toggle("active", format === active);
    button.onclick = () => { state.format = format; run(); };
    return button;
  }), Object.assign(document.createElement("span"), { className: "hint", textContent: t("Click a node or an edge to change the view") }));
}

// The diagram, zoomed and moved as on the site, its nodes and edges answering a click.
let zoomState = null;
let zoom = null;
function drawn(target, svg) {
  const box = Object.assign(document.createElement("div"), { className: "zoom" });
  const element = document.importNode(new DOMParser().parseFromString(svg, "image/svg+xml").documentElement, true);
  // The name of each node and edge in the DOT, before zoom.js turns the titles into bubbles.
  for (const item of element.querySelectorAll("g.node, g.edge")) {
    item.dataset.compose = item.querySelector(":scope > title")?.textContent ?? "";
  }
  box.append(element);
  target.append(box);
  if (zoom) zoomState = zoom.state();
  zoom = globalThis.cadastreZoom?.attach(box, { wheel: true, height: "calc(100% - 4px)", state: zoomState, language: lang }) ?? null;
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
}

function goTo(line, column) {
  const code = $("code");
  const offset = code.value.split("\n").slice(0, line - 1).reduce((n, l) => n + l.length + 1, 0) + column - 1;
  code.focus();
  code.setSelectionRange(offset, offset);
}

// ——— The panel ———

// A small DOM builder: `h("div", { className: "row" }, child, …)`.
function h(tag, props = {}, ...children) {
  const element = Object.assign(document.createElement(tag), props);
  element.append(...children.flat().filter((c) => c !== null && c !== undefined && c !== false));
  return element;
}
const section = (title, note, ...children) => h("section", {}, h("h2", {}, title, note ? h("small", {}, ` — ${note}`) : null), ...children);
const chip = (label, onRemove, extra = "") => h("span", { className: `chip ${extra}` }, label, onRemove ? h("button", { textContent: "×", title: "✕", onclick: onRemove }) : null);
function choice(placeholder, options, onPick) {
  const select = h("select", {}, h("option", { value: "", textContent: placeholder }), ...options.map(([value, label]) => h("option", { value, textContent: label })));
  select.onchange = () => { if (select.value) onPick(select.value); };
  return select;
}
const checkbox = (label, checked, onChange) => h("label", {}, h("input", { type: "checkbox", checked, onchange: (e) => onChange(e.target.checked) }), label);
function field(label, input) {
  return h("div", { className: "field" }, h("span", { textContent: label }), input);
}
// A text input that changes the view once left or validated, not at each key.
function textInput(value, onChange, props = {}) {
  const input = h("input", { type: "text", value: value ?? "", ...props });
  input.onchange = () => onChange(input.value.trim());
  input.onkeydown = (e) => { if (e.key === "Enter") input.blur(); };
  return input;
}

function panel() {
  const parts = view();
  const root = $("panel");
  const scroll = root.scrollTop;
  if (!parts || !state.facts) {
    root.replaceChildren();
    return;
  }
  const facts = state.facts;
  const select = selectOf(parts);
  const edges = edgesOf(parts);
  const render = renderOf(parts);
  const attribute = (key) => parts.attributes[key]?.value ?? "";

  // The view: its name and what its readers see of it.
  const name = textInput(parts.name, (value) => {
    if (/^[A-Za-z_][A-Za-z0-9_-]*$/.test(value)) apply(splice(state.text, parts.name_range, value));
  });
  const description = h("textarea", { value: attribute("description"), rows: 2 });
  description.onchange = () => apply(withAttribute("description", description.value.trim()));
  const about = section(t("The view"), null,
    field(t("Name"), name),
    field(t("Title"), textInput(attribute("title"), (v) => apply(withAttribute("title", v)))),
    field(t("Description"), description),
    field(t("Folder"), textInput(attribute("group"), (v) => apply(withAttribute("group", v)), { placeholder: t("Network/Security") })),
  );

  // What: the kinds, with their count in the project.
  const count = (kind) => facts.kinds.find((k) => k.kind === kind)?.count;
  const setSelect = (s) => apply(withClause("select", selectText(s)));
  const what = section(t("What"), t("the kinds drawn"),
    h("div", { className: "row" },
      select.kinds.map((kind) => chip(h("span", {}, kind, count(kind) ? h("span", { className: "count", textContent: ` ${count(kind)}` }) : null),
        () => setSelect({ ...select, kinds: select.kinds.filter((k) => k !== kind) }))),
      choice(t("Add a kind…"), facts.kinds.filter((k) => !select.kinds.includes(k.kind)).map((k) => [k.kind, `${k.kind} (${k.count})`]),
        (kind) => setSelect({ ...select, kinds: [...select.kinds, kind] }))),
    h("div", { className: "row" }, checkbox(t("with their neighbours (with ends)"), select.with_ends, (v) => setSelect({ ...select, with_ends: v }))),
    h("div", { className: "row" }, checkbox(t("without the isolated ones"), select.without_isolated, (v) => setSelect({ ...select, without_isolated: v }))),
  );

  // Filter: the text of `where`, and a condition built from the facts.
  const keys = conditionKeys(select.kinds);
  const key = choice("…", keys.map((k) => [k, k]), () => valuesFor());
  const operator = h("select", {}, ...["=", "!=", "in"].map((o) => h("option", { value: o, textContent: o })));
  const value = h("input", { type: "text", placeholder: t("value"), size: 12 });
  const values = h("datalist", { id: "filter-values" });
  value.setAttribute("list", "filter-values");
  const valuesFor = () => values.replaceChildren(...conditionValues(key.value, select.kinds).map((v) => h("option", { value: v })));
  const add = h("button", { textContent: t("Add"), onclick: () => {
    if (!key.value || !value.value.trim()) return;
    const written = operator.value === "in" ? `[${names(value.value).map(literal).join(", ")}]` : literal(value.value.trim());
    setSelect(withCondition(select, `${key.value} ${operator.value} ${written}`));
  } });
  const filter = section(t("Filter"), t("which of them"),
    textInput(select.where ?? "", (v) => setSelect({ ...select, where: v || null }), { placeholder: "ancestor(site) = dc-1 and criticality = high" }),
    h("div", { className: "row" }, key, operator, value, values, add),
  );

  // Links: the layers, lifted or not, the other selectors as written.
  const setEdges = (e) => apply(withClause("edges", edgesText(e)));
  const via = liftVia(edges.main.lift);
  const lifted = h("select", {}, ...LIFTS.map(([v, label]) => h("option", { value: v, textContent: t(label), selected: (edges.main.lift === null ? "" : via) === v })));
  lifted.onchange = () => setEdges({ ...edges, main: { ...edges.main, lift: lifted.value ? withVia(edges.main.lift, lifted.value) : null } });
  const links = section(t("Links"), t("the edges between them"),
    h("div", { className: "layers" }, facts.layers.map(({ layer, count: n }) =>
      checkbox(h("span", {}, layer, h("span", { className: "muted", textContent: ` ${n}` })), edges.main.layers.includes(layer), (on) => setEdges({
        ...edges, main: { ...edges.main, layers: on ? [...edges.main.layers, layer] : edges.main.layers.filter((l) => l !== layer) },
      })))),
    h("div", { className: "row" }, h("span", { className: "muted", textContent: t("Lifted onto") }), lifted),
    edges.others.length ? h("div", { className: "row" }, edges.others.map((s, i) => chip(s, () => setEdges({ ...edges, others: edges.others.filter((_, j) => j !== i) }), "verbatim"))) : null,
    h("div", { className: "row" }, textInput("", (v) => { if (v) setEdges({ ...edges, others: [...edges.others, v] }); }, { placeholder: t("follow(data), process(name), path(a, b)…") })),
  );

  // Frames: the levels of `group-by`, from the broadest.
  const levels = select.group_by;
  const setLevels = (group_by) => setSelect({ ...select, group_by });
  const move = (i, by) => { const l = [...levels]; [l[i], l[i + by]] = [l[i + by], l[i]]; setLevels(l); };
  const candidates = [...new Set([...facts.containers, ...facts.references])].filter((l) => !levels.includes(l)).sort();
  const frames = section(t("Frames"), t("group-by, from the broadest"),
    levels.map((level, i) => h("div", { className: "level" },
      h("span", { className: "chip" }, level),
      h("button", { textContent: "↑", disabled: i === 0, onclick: () => move(i, -1) }),
      h("button", { textContent: "↓", disabled: i === levels.length - 1, onclick: () => move(i, 1) }),
      h("button", { textContent: "×", onclick: () => setLevels(levels.filter((l) => l !== level)) }))),
    h("div", { className: "row" }, choice(t("Add a level…"), candidates.map((c) => [c, c]), (level) => setLevels([...levels, level]))),
  );

  // Drawing: the formats, then the options most views need; the others as written.
  const setRender = (r) => apply(withClause("render", renderText(r, parts.indent)));
  const known = ["direction", "orientation", "legend", "satellites", "node_labels", "edge_labels", "node_labels_wrap"];
  const direction = h("select", {}, ...[["", "default"], ["lr", "left to right"], ["tb", "top to bottom"], ["rl", "right to left"], ["bt", "bottom to top"]]
    .map(([v, label]) => h("option", { value: v, textContent: t(label), selected: option(render, "direction") === v })));
  direction.onchange = () => setRender(withOption(render, "direction", direction.value));
  const list = (key) => (option(render, key).match(/^\[(.*)\]$/s)?.[1] ?? option(render, key));
  const drawing = section(t("Drawing"), t("the formats and their options"),
    h("div", { className: "row" },
      render.formats.map((f) => chip(f, () => setRender({ ...render, formats: render.formats.filter((x) => x !== f) }))),
      choice(t("Add a format…"), FORMATS.filter((f) => !render.formats.includes(f)).map((f) => [f, f]), (f) => setRender({ ...render, formats: [...render.formats, f] }))),
    field(t("Direction"), direction),
    h("div", { className: "row" }, checkbox(t("in the direction of the data"), option(render, "orientation") === "data",
      (on) => setRender(withOption(render, "orientation", on ? "data" : "")))),
    h("div", { className: "row" }, checkbox(t("legend"), option(render, "legend") === "true", (on) => setRender(withOption(render, "legend", on ? "true" : "")))),
    h("div", { className: "row" }, checkbox(t("a copy of a shared node in each frame"), option(render, "satellites") === "true",
      (on) => setRender(withOption(render, "satellites", on ? "true" : "")))),
    field(t("Node text"), textInput(list("node_labels"), (v) => setRender(withOption(render, "node_labels", v ? `[${names(v).join(", ")}]` : "")), { placeholder: "label, description" })),
    field(t("Edge labels"), textInput(list("edge_labels"), (v) => setRender(withOption(render, "edge_labels", v ? `[${names(v).join(", ")}]` : "")), { placeholder: "port, carries" })),
    field(t("Cut at"), h("span", { className: "row" }, textInput(option(render, "node_labels_wrap"), (v) => setRender(withOption(render, "node_labels_wrap", /^\d+$/.test(v) ? v : "")), { size: 4, style: "width: 5em" }), t("characters"))),
    render.options.some((o) => !known.includes(o.key)) ? field(t("Other options"), h("div", { className: "row" },
      render.options.filter((o) => !known.includes(o.key)).map((o) => chip(`${o.key}: ${o.value}`, () => setRender(withOption(render, o.key, "")), "verbatim")))) : null,
  );

  root.replaceChildren(about, what, filter, links, frames, drawing, h("section", { id: "insight" }));
  insight();
  root.scrollTop = scroll;
}

// The keys a condition can test: the name, the containers, then the keys of the kinds selected.
function conditionKeys(kinds) {
  const facts = state.facts;
  const ancestors = facts.containers.filter((c) => ["site", "system", "network", "subnet", "vpc", "cluster"].includes(c)).map((c) => `ancestor(${c})`);
  const own = facts.keys.filter((k) => !kinds.length || k.kinds.some((kind) => kinds.includes(kind))).map((k) => k.key);
  return [...new Set(["name", "kind", ...ancestors, ...own])];
}

function conditionValues(key, kinds) {
  const facts = state.facts;
  const of = (kind) => facts.resources.filter((r) => r.kind === kind).map((r) => r.id);
  if (key === "name") return facts.resources.filter((r) => !kinds.length || kinds.includes(r.kind)).map((r) => r.id);
  if (key === "kind") return facts.kinds.map((k) => k.kind);
  const ancestor = key.match(/^ancestor\((\w+)\)$/);
  if (ancestor) return of(ancestor[1]);
  return facts.keys.find((k) => k.key === key)?.values ?? [];
}

// What the view draws, what it loses, and what would make it read better.
function insight() {
  const box = $("insight");
  const parts = view();
  if (!box || !parts) return;
  const data = state.insight;
  if (!data) {
    box.replaceChildren();
    return;
  }
  const select = selectOf(parts);
  const setSelect = (s) => apply(withClause("select", selectText(s)));
  // The links lost, by the kind of their end out of the view: that kind added, or the ends.
  const lost = data.lost.length ? h("div", { className: "note" },
    t("Links lost because one of their ends is not selected:"),
    data.lost.map((l) => h("div", { className: "row" },
      h("span", { textContent: t("{count} {layer} link(s) to {kind} out of the view, not drawn: {examples}", {
        count: l.count, layer: l.layer, kind: l.kind, examples: l.examples.map((e) => e.split(".").pop()).join(", "),
      }) }),
      select.kinds.length && !select.kinds.includes(l.kind)
        ? h("button", { textContent: t("Add {kind}", { kind: l.kind }), onclick: () => setSelect({ ...select, kinds: [...select.kinds, l.kind] }) }) : null)),
    !select.with_ends ? h("div", { className: "row" }, h("button", { textContent: t("Add the ends (with ends)"), onclick: () => setSelect({ ...select, with_ends: true }) })) : null) : null;
  const levels = ["system", "site"].filter((l) => state.facts.containers.includes(l));
  const dense = data.counts.nodes > 40 && !select.group_by.length && levels.length
    ? h("div", { className: "note info" }, t("{nodes} nodes and no frame: grouped, it reads better.", { nodes: data.counts.nodes }),
      h("div", { className: "row" }, levels.map((level) => h("button", { textContent: t("Group by {level}", { level }), onclick: () => setSelect({ ...select, group_by: [level] }) }))))
    : null;
  box.replaceChildren(...[h("h2", {}, t("What it draws")),
    h("div", { className: "counts", textContent: t("{nodes} nodes · {edges} edges · {frames} frames", data.counts) }),
    lost, dense].filter(Boolean));
}

// ——— The menus of the diagram: a node or an edge clicked, what it can change ———

let pressed = null;
function menuOn(event) {
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
    if (info.site && info.kind !== "site") actions.push([t("Only the site {name}", { name: info.site }), () => setSelect(withCondition(select, `ancestor(site) = ${info.site}`))]);
    for (const level of ["system", "site"].filter((l) => state.facts.containers.includes(l) && !select.group_by.includes(l))) {
      actions.push([t("Group by {level}", { level }), () => setSelect({ ...select, group_by: [...select.group_by, level] })]);
    }
    if (select.kinds.includes(info.kind) && select.kinds.length > 1) {
      actions.push([t("Without the kind {kind}", { kind: info.kind }), () => setSelect({ ...select, kinds: select.kinds.filter((k) => k !== info.kind) })]);
    }
    if (project?.site) actions.push([t("Its page on the site"), `${project.site}resources/${info.id.replace(/[^A-Za-z0-9._-]/g, "_")}.html`]);
  } else if (edge) {
    const info = state.insight.edges[Number(edge[1])];
    if (!info) return hideMenu();
    head = [t("an edge"), info.layers.join(", ")];
    const edges = edgesOf(parts);
    const setEdges = (e) => apply(withClause("edges", edgesText(e)));
    for (const layer of info.layers.filter((l) => edges.main.layers.includes(l))) {
      if (edges.main.layers.length > 1) actions.push([t("Only the layer {layer}", { layer }), () => setEdges({ ...edges, main: { ...edges.main, layers: [layer] } })]);
      actions.push([t("Without the layer {layer}", { layer }), () => setEdges({ ...edges, main: { ...edges.main, layers: edges.main.layers.filter((l) => l !== layer) } })]);
    }
  } else {
    return hideMenu();
  }
  const menu = $("menu");
  menu.replaceChildren(h("div", { className: "head" }, h("b", { textContent: head[0] }), h("small", { textContent: head[1] })),
    ...actions.map(([label, action]) => (typeof action === "string"
      ? h("a", { href: action, target: "_blank", textContent: label })
      : h("button", { textContent: label, onclick: action }))));
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

// Colors the code with the module's highlighter, the range of the last change marked.
function paintCode() {
  const editor = $("code");
  const target = $("highlight");
  const text = editor.value;
  const pieces = [];
  const piece = (start, end, className) => {
    const cuts = [start, end, ...(changed ?? []).filter((b) => b > start && b < end)].sort((a, b) => a - b);
    for (let i = 0; i + 1 < cuts.length; i++) {
      const [a, b] = [cuts[i], cuts[i + 1]];
      if (a >= b) continue;
      let node = className ? h("span", { className, textContent: text.slice(a, b) }) : document.createTextNode(text.slice(a, b));
      if (changed && a >= changed[0] && b <= changed[1]) node = h("mark", {}, node);
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
  pieces.push("\n");
  target.replaceChildren(...pieces);
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

// ——— Starting points: a recipe, a view of the project, a link ———

function recipes() {
  const list = $("recipe-list");
  const resources = state.facts.resources;
  list.replaceChildren(...state.facts.recipes.map((recipe) => {
    const [title, text] = RECIPES[recipe.id] ?? [recipe.id, ""];
    const offered = recipe.asks ? resources.filter((r) => recipe.asks === "any" || r.kind === recipe.asks) : [];
    if (recipe.asks && !offered.length) return null;
    const input = recipe.asks ? h("input", { type: "text", placeholder: t("Pick a resource…") }) : null;
    input?.setAttribute("list", `recipe-${recipe.id}`);
    const options = recipe.asks ? h("datalist", { id: `recipe-${recipe.id}` }, offered.slice(0, 2000).map((r) => h("option", { value: r.id, label: r.label ?? r.kind }))) : null;
    const create = h("button", { className: "primary", textContent: t("Create"), onclick: () => {
      const resource = recipe.asks ? offered.find((r) => r.id === input.value.trim()) : null;
      if (recipe.asks && !resource) return input.focus();
      $("recipes").close();
      fromRecipe(recipe, resource);
    } });
    return h("div", { className: "recipe" }, h("h3", { textContent: t(title, { x: recipe.asks ? "…" : "" }) }), h("p", { textContent: t(text) }), input, options, create);
  }).filter(Boolean));
  $("recipes-footer").replaceChildren(t("Or start from nothing: an empty view."), " ",
    h("button", { textContent: t("An empty view"), onclick: () => { $("recipes").close(); fromBody("new-view", "", "select app\nedges  [flow]\nrender svg"); } }));
  $("recipes").showModal();
}

function fromRecipe(recipe, resource) {
  const body = recipe.body.replaceAll("{x}", resource?.id ?? "").replaceAll("{kind}", resource?.kind ?? "");
  const short = resource ? resource.id.split(".").pop() : "";
  const title = t(RECIPES[recipe.id]?.[0] ?? recipe.id, { x: resource ? resource.label ?? short : "" });
  fromBody(`${recipe.id}${short ? `-${short}` : ""}`, title, body);
}

function fromBody(base, title, body) {
  let name = base.replace(/[^A-Za-z0-9_-]/g, "-");
  for (let i = 2; state.views.includes(name); i++) name = `${base}-${i}`;
  const lines = [...(title ? [`title: ${quoted(title)}`] : []), ...body.split("\n")];
  state.origin = null;
  state.format = null;
  changed = null;
  setText(`view ${name} {\n${lines.map((l) => `  ${l}`).join("\n")}\n}`);
}

function setText(text) {
  state.text = text;
  $("code").value = text;
  paintCode();
  panel();
  run();
}

// A view of the project, to edit: its block, and where it is written.
function edit(name) {
  for (const file of state.files.filter((f) => f.path.endsWith(".cav"))) {
    const found = cadastre.parts(file.text).find((v) => v.name === name);
    if (found) {
      state.origin = { path: file.path, name };
      setText(file.text.slice(found.range[0], found.range[1]));
      return true;
    }
  }
  return false;
}

// ——— Link, copy, save ———

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
    prompt(t(label), text);
  }
}

// `cadastre serve --edit`: the view written in its file, the one it edits replaced.
async function save() {
  const parts = view();
  if (!parts) return;
  let path = state.origin?.path;
  if (!path) {
    const files = state.files.filter((f) => f.path.endsWith(".cav")).map((f) => f.path);
    path = prompt(`${t("The file to write the view in:")}\n${files.join("\n")}`, files[0] ?? "views.cav");
    if (!path) return;
  }
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
    status(t("Saved in {file}", { file: path }), "ok");
    $("save").textContent = t("Save in {file}", { file: path });
    remember();
  } catch (error) {
    status(t("Cannot save: {error}", { error: error.message }), "ko");
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

// The facts of the project and its views, once: what the panel offers.
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
      state.origin = name ? state.files.filter((f) => f.path.endsWith(".cav")).map((f) => ({ path: f.path, found: cadastre.parts(f.text).some((v) => v.name === name) }))
        .filter((f) => f.found).map((f) => ({ path: f.path, name }))[0] ?? null : null;
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
  for (const [id, text] of [["start", "Start from a recipe…"], ["copy", "Copy the code"], ["share", "Copy the link"], ["save", "Save"], ["recipes-title", "Start from a recipe"]]) {
    $(id).textContent = t(text);
  }
  $("example-label").firstElementChild.textContent = t("Project");
  $("code").setAttribute("aria-label", t("Code of the view"));
}

async function open(files) {
  state.files = files;
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
  timer = setTimeout(() => { panel(); run(); }, 300);
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
$("start").addEventListener("click", recipes);
$("recipes-close").addEventListener("click", () => $("recipes").close());
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
    const home = h("a", { href: `${project.site}index.html`, textContent: project.name });
    $("title").replaceChildren(home);
    $("site").hidden = false;
  } else {
    $("title").textContent = project.name;
  }
  $("save").hidden = !project.edit;
  await open(project.files);
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
