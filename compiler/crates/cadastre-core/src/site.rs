//! The site of a project (`cadastre site`): pages for its readers, who never see a line of the
//! registry. A page per view and per resource, indexes, a search, and the JSON model under
//! `api/`. Static files: the host writes them, serves them, deploys them.
//!
//! The diagrams are given by the host, laid out by Graphviz ([`dots`] gives it the DOT of each
//! view, its nodes linking to their pages); without them, a page holds the DOT and draws it in
//! the browser with Graphviz for the browser, when the host says where to load it from.

use std::collections::{BTreeMap, HashMap};

use serde_json::Value;

use crate::api;
use crate::project::Compilation;
use crate::render::icons::icon;
use crate::source::SourceFile;

/// What the host gives to build the site.
pub struct Options<'a> {
    /// The name of the project, the title of the site.
    pub name: &'a str,
    /// The SVG of each view, by name, laid out by Graphviz and with its icons.
    pub svgs: &'a HashMap<String, String>,
    /// Where a page loads Graphviz for the browser to draw a view that has no SVG (a path
    /// relative to the root of the site, or a URL); `None`: such a view shows its DOT.
    pub viz: Option<&'a str>,
    /// Whether the playground is beside the site, in `playground/`: the pages link to it.
    pub playground: bool,
    /// The language of the pages (their words; the contents are those of the project).
    pub language: crate::i18n::Language,
}

/// A file of the site: its path from the root of the site, its content.
pub struct File {
    pub path: String,
    pub content: String,
}

/// The DOT of each view, its nodes and frames linking to their pages: what the host lays out.
pub fn dots(compilation: &Compilation) -> Vec<(String, String)> {
    if compilation.diagnostics.iter().any(|d| d.severity == crate::diagnostic::Severity::Error) {
        return Vec::new();
    }
    compilation
        .plans
        .views
        .iter()
        .map(|view| {
            let mut diagnostics = Vec::new();
            let graph = crate::view::evaluate(view, &compilation.registry, &compilation.model, &mut diagnostics);
            let scene =
                crate::render::Scene::new(&graph, &compilation.plans, &compilation.registry, &compilation.model)
                    .with_language(compilation.language)
                    .with_page_links("../resources/");
            (view.name.clone(), crate::render::dot(&scene))
        })
        .collect()
}

/// Builds the site.
pub fn build(compilation: &Compilation, files: &[SourceFile], options: &Options) -> Vec<File> {
    let site = Site { compilation, files, options, project: api::project(compilation, files, options.name) };
    site.build()
}

struct Site<'a> {
    compilation: &'a Compilation,
    files: &'a [SourceFile],
    options: &'a Options<'a>,
    project: Value,
}

/// The file name of a resource page or record: its qualified name, safe as a file name.
pub fn file_name(id: &str) -> String {
    id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' }).collect()
}

fn esc(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn strings(value: &Value) -> Vec<String> {
    value.as_array().map(|a| a.iter().map(text).collect()).unwrap_or_default()
}

fn kind_icon(kind: &str) -> String {
    match icon(kind) {
        Some(paths) => format!(
            "<svg class=\"icon\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" \
             stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\">{paths}</svg>"
        ),
        None => String::new(),
    }
}

impl Site<'_> {
    fn build(&self) -> Vec<File> {
        let mut out = Vec::new();
        out.push(File { path: "site.css".into(), content: CSS.to_owned() });
        out.push(File { path: "site.js".into(), content: JS.to_owned() });
        // The API: the records every page is built from.
        let index = api::index(self.compilation, self.files);
        out.push(File { path: "api/project.json".into(), content: pretty(&self.project) });
        out.push(File { path: "api/resources.json".into(), content: pretty(&index) });
        out.push(File { path: "api/links.json".into(), content: pretty(&api::links(self.compilation, self.files)) });
        out.push(File { path: "api/checks.json".into(), content: pretty(&api::checks(self.compilation, self.files)) });
        out.push(File { path: "api/search.json".into(), content: pretty(&api::search(self.compilation)) });
        let mut records = Vec::new();
        for id in self.compilation.registry.ids() {
            let record = api::resource(self.compilation, self.files, id);
            out.push(File {
                path: format!("api/resources/{}.json", file_name(&text(&record["id"]))),
                content: pretty(&record),
            });
            records.push(record);
        }
        let mut views = Vec::new();
        for view in &self.compilation.plans.views {
            if let Ok(record) = api::view(self.compilation, self.files, view) {
                out.push(File { path: format!("api/views/{}.json", file_name(&view.name)), content: pretty(&record) });
                views.push(record);
            }
        }
        // The diagrams as files: the thumbnails of the home page load them lazily.
        for (name, svg) in self.options.svgs {
            out.push(File { path: format!("views/{}.svg", file_name(name)), content: svg.clone() });
        }
        // The own renderings of the views drawn as a sequence or a traversal.
        let own = self.own_renderings();
        for (name, (format, content)) in &own {
            if format == "traversal" {
                out.push(File { path: format!("views/{}.traversal.svg", file_name(name)), content: content.clone() });
            }
        }
        // The views each resource appears in.
        let mut appears: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
        for view in &views {
            let name = text(&view["name"]);
            let title = text(&view["title"]);
            for node in view["nodes"].as_array().into_iter().flatten() {
                appears.entry(text(&node["id"])).or_default().push((name.clone(), title.clone()));
            }
            for frame in view["frames"].as_array().into_iter().flatten() {
                if let Some(id) = frame["resource"].as_str() {
                    appears.entry(id.to_owned()).or_default().push((name.clone(), title.clone()));
                }
            }
        }
        for list in appears.values_mut() {
            list.sort();
            list.dedup();
        }
        // The pages.
        out.push(File { path: "index.html".into(), content: self.home(&views, &records) });
        out.push(File { path: "browse.html".into(), content: self.browse(&records) });
        out.push(File { path: "checks.html".into(), content: self.checks() });
        out.push(File { path: "api.html".into(), content: self.api_page(&views, &records) });
        for view in &views {
            out.push(File {
                path: format!("views/{}.html", file_name(&text(&view["name"]))),
                content: self.view_page(view, own.get(&text(&view["name"]))),
            });
        }
        for record in &records {
            let id = text(&record["id"]);
            let views_of = appears.get(&id).cloned().unwrap_or_default();
            out.push(File {
                path: format!("resources/{}.html", file_name(&id)),
                content: self.resource_page(record, &records, &views_of),
            });
        }
        out
    }

    /// A word of the pages in their language.
    fn t(&self, english: &'static str) -> &'static str {
        self.options.language.text(english)
    }

    /// The views whose `render` is a sequence diagram or a traversal, with that rendering: the
    /// sequence as Mermaid text, the traversal as SVG. The first of the two when a view has both.
    fn own_renderings(&self) -> BTreeMap<String, (String, String)> {
        let mut out = BTreeMap::new();
        if self.compilation.diagnostics.iter().any(|d| d.severity == crate::diagnostic::Severity::Error) {
            return out;
        }
        for view in &self.compilation.plans.views {
            let formats: Vec<String> =
                view.render.iter().flat_map(|r| r.formats.iter().map(|f| f.text.clone())).collect();
            let Some(format) = ["sequence", "traversal"].into_iter().find(|f| formats.iter().any(|w| w == f)) else {
                continue;
            };
            // Drawn here rather than by `build`: the participants link to the pages of the site.
            let c = self.compilation;
            let mut diagnostics = Vec::new();
            let graph = crate::view::evaluate(view, &c.registry, &c.model, &mut diagnostics);
            let scene = crate::render::Scene::new(&graph, &c.plans, &c.registry, &c.model)
                .with_language(c.language)
                .with_page_links("../resources/");
            let content = match format {
                "sequence" => crate::render::sequence::sequence(&scene),
                _ => crate::render::traversal::traversal(&scene),
            };
            out.insert(view.name.clone(), (format.to_owned(), content));
        }
        out
    }

    /// The frame of every page: head, header with the search, the body, the footer.
    fn page(&self, root: &str, title: &str, body: &str) -> String {
        let name = esc(self.options.name);
        let playground = if self.options.playground {
            format!("<a href=\"{root}playground/\">{}</a>", self.t("Playground"))
        } else {
            String::new()
        };
        format!(
            "<!doctype html>\n<html lang=\"{lang}\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
             <title>{title} · {name}</title>\n<link rel=\"stylesheet\" href=\"{root}site.css\">\n</head>\n\
             <body data-root=\"{root}\" data-playground=\"{has_playground}\" data-cannot-draw=\"{cannot_draw}\" data-no-graphviz=\"{no_graphviz}\">\n<header>\n<a class=\"brand\" href=\"{root}index.html\">{name}</a>\n\
             <nav><a href=\"{root}index.html\">{views}</a><a href=\"{root}browse.html\">{resources}</a><a href=\"{root}checks.html\">{checks}</a><a href=\"{root}api.html\">{api}</a>{playground}</nav>\n\
             <div class=\"search\"><input id=\"search\" type=\"search\" placeholder=\"{search}\" autocomplete=\"off\" aria-label=\"{search_label}\"><ul id=\"results\" hidden></ul></div>\n\
             </header>\n<main>\n{body}\n</main>\n<footer>{generated} <a href=\"https://github.com/Jbbouille/cadastre\">Cadastre</a> {version} · <a href=\"{root}api/project.json\">{model}</a></footer>\n\
             <script src=\"{root}site.js\"></script>\n</body>\n</html>\n",
            lang = self.options.language.code(),
            title = esc(title),
            version = env!("CARGO_PKG_VERSION"),
            has_playground = self.options.playground,
            cannot_draw = esc(self.t("Cannot draw the diagram:")),
            no_graphviz = esc(self.t("Graphviz for the browser could not be loaded.")),
            views = self.t("Views"),
            resources = self.t("Resources"),
            checks = self.t("Checks"),
            api = self.t("API"),
            search = esc(self.t("Search a resource…")),
            search_label = esc(self.t("Search a resource")),
            generated = self.t("Generated by"),
            model = self.t("JSON model"),
        )
    }

    fn resource_link(&self, root: &str, id: &str) -> String {
        let registry = &self.compilation.registry;
        let label = registry
            .ids()
            .find(|&r| registry.qualified_name(r) == id)
            .and_then(|r| api::label(registry, r))
            .unwrap_or_else(|| id.rsplit('.').next().unwrap_or(id).to_owned());
        format!("<a href=\"{root}resources/{}.html\" title=\"{}\">{}</a>", file_name(id), esc(id), esc(&label))
    }

    fn home(&self, views: &[Value], records: &[Value]) -> String {
        let project = &self.project;
        let mut body = format!("<h1>{}</h1>\n", esc(self.options.name));
        let kinds = project["kinds"].as_object().cloned().unwrap_or_default();
        // The sites by their hosting, when there are several kinds of them: a hundred stores and
        // two datacenters are not "102 sites" to a reader.
        let mut hostings: BTreeMap<String, u64> = BTreeMap::new();
        for r in records.iter().filter(|r| r["kind"] == "site") {
            *hostings.entry(text(&r["attributes"]["hosting"])).or_default() += 1;
        }
        let by_hosting = if hostings.len() > 1 {
            let mut parts: Vec<(&String, &u64)> = hostings.iter().filter(|(h, _)| !h.is_empty()).collect();
            parts.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
            let parts: Vec<String> = parts.iter().map(|(h, n)| format!("{n} {}", esc(h))).collect();
            format!(" <span class=\"muted\">({})</span>", parts.join(", "))
        } else {
            String::new()
        };
        let counts: Vec<String> = ["site", "system", "app", "database", "server", "saas"]
            .iter()
            .filter_map(|k| {
                kinds.get(*k).and_then(Value::as_u64).map(|n| {
                    let detail = if *k == "site" { by_hosting.as_str() } else { "" };
                    format!("<li><b>{n}</b> {}{detail}</li>", plural(k, n))
                })
            })
            .collect();
        let (resources, links) = (project["resources"].as_u64().unwrap_or(0), project["links"].as_u64().unwrap_or(0));
        body.push_str(&format!(
            "<ul class=\"counts\"><li><b>{resources}</b> {}</li><li><b>{links}</b> {}</li>{}</ul>\n",
            self.options.language.plural("resource", resources),
            self.options.language.plural("link", links),
            counts.join("")
        ));
        // A check of severity `info` that fails is something to know, not a failure.
        let not_passed = |c: &&&Value| c["result"] == "failed" || c["result"] == "cannot be evaluated";
        let checks: Vec<&Value> = project["checks"].as_array().into_iter().flatten().collect();
        let failing = checks.iter().filter(not_passed).filter(|c| c["severity"] != "info").count();
        let to_know = checks.iter().filter(not_passed).filter(|c| c["severity"] == "info").count();
        if !checks.is_empty() {
            let status = if failing == 0 { "ok" } else { "ko" };
            let mut parts =
                vec![format!("{} {}", checks.len(), self.options.language.plural("check", checks.len() as u64))];
            if failing > 0 {
                parts.push(format!("{failing} {}", self.t("failing")));
            }
            if to_know > 0 {
                parts.push(format!("{to_know} {}", self.t("to know")));
            }
            body.push_str(&format!(
                "<p class=\"checks {status}\"><a href=\"checks.html\">{}</a></p>\n",
                parts.join(" · ")
            ));
        }
        let errors =
            project["diagnostics"].as_array().into_iter().flatten().filter(|d| d["severity"] == "error").count();
        if errors > 0 {
            body.push_str(&format!(
                "<p class=\"checks ko\">{}</p>\n",
                self.t("The project has {n} error(s): the views are not drawn until they are fixed (see <a href=\"checks.html\">Checks</a>).")
                    .replace("{n}", &errors.to_string())
            ));
        }
        // The views by file of views, the files in alphabetical order, each in the order written:
        // the file is the theme (`network.cav`, `security.cav`). One file: no heading.
        let mut by_file: BTreeMap<String, Vec<&Value>> = BTreeMap::new();
        for view in views {
            // `path:line:column`: the path alone.
            let declared = text(&view["declared"]);
            let file = declared.rsplitn(3, ':').last().unwrap_or(declared.as_str());
            by_file.entry(file.to_owned()).or_default().push(view);
        }
        body.push_str(&format!("<h2>{}</h2>\n", self.t("Views")));
        for (file, views) in &by_file {
            if by_file.len() > 1 {
                let stem = std::path::Path::new(file).file_stem().and_then(|s| s.to_str()).unwrap_or(file);
                body.push_str(&format!("<h3 class=\"group\">{}</h3>\n", esc(stem)));
            }
            body.push_str("<div class=\"cards\">\n");
            for view in views {
                let name = text(&view["name"]);
                let svg = self.options.svgs.get(&name);
                let thumb = match (svg, self.options.viz) {
                    // The file of the diagram, loaded when the card comes into view: a light page.
                    (Some(_), _) => format!(
                        "<div class=\"thumb\"><img src=\"views/{}.svg\" alt=\"\" loading=\"lazy\"></div>",
                        file_name(&name)
                    ),
                    (None, Some(viz)) => {
                        let dot = dots(self.compilation)
                            .into_iter()
                            .find(|(n, _)| *n == name)
                            .map(|(_, d)| d)
                            .unwrap_or_default();
                        format!(
                            "<div class=\"thumb\" data-viz=\"{}\"><script type=\"text/vnd.graphviz\">{}</script></div>",
                            esc(viz),
                            dot.replace("</script", "<\\/script")
                        )
                    }
                    (None, None) => format!("<div class=\"thumb empty\">{}</div>", self.t("no diagram")),
                };
                body.push_str(&format!(
                    "<a class=\"card\" href=\"views/{}.html\">{thumb}<h3>{}</h3><p>{}</p></a>\n",
                    file_name(&name),
                    esc(&text(&view["title"])),
                    esc(&text(&view["description"]))
                ));
            }
            body.push_str("</div>\n");
        }
        // The systems and the sites: the two entries of a map.
        // Grouped by file when they come from several, as the views are: a file of stores does
        // not hide the datacenters; in each file, the order written.
        for (kind, heading) in [("system", "Systems"), ("site", "Sites")] {
            let items: Vec<&Value> = records.iter().filter(|r| r["kind"] == kind && r["parent"].is_null()).collect();
            if items.is_empty() {
                continue;
            }
            body.push_str(&format!("<h2>{}</h2>\n", self.t(heading)));
            let mut by_file: BTreeMap<String, Vec<&Value>> = BTreeMap::new();
            for r in items {
                let declared = text(&r["declared"]);
                let file = declared.rsplitn(3, ':').last().unwrap_or(declared.as_str());
                by_file.entry(file.to_owned()).or_default().push(r);
            }
            for (file, items) in &by_file {
                if by_file.len() > 1 {
                    let stem = std::path::Path::new(file).file_stem().and_then(|s| s.to_str()).unwrap_or(file);
                    body.push_str(&format!("<h3 class=\"group\">{}</h3>\n", esc(stem)));
                }
                body.push_str("<ul class=\"list\">\n");
                for r in items {
                    let id = text(&r["id"]);
                    body.push_str(&format!(
                        "<li>{}{} <span class=\"muted\">{}</span></li>\n",
                        kind_icon(kind),
                        self.resource_link("", &id),
                        esc(&text(&r["description"]))
                    ));
                }
                body.push_str("</ul>\n");
            }
        }
        self.page("", self.t("Home"), &body)
    }

    fn browse(&self, records: &[Value]) -> String {
        // The kinds within a resource (an interface, a component…) are on the page of what holds
        // them: noise in an index.
        let schema = &self.compilation.registry.schema;
        let within = |kind: &str| schema.kind_def(kind).is_some_and(|d| d.within);
        let mut by_kind: BTreeMap<String, Vec<&Value>> = BTreeMap::new();
        for r in records.iter().filter(|r| !within(&text(&r["kind"]))) {
            by_kind.entry(text(&r["kind"])).or_default().push(r);
        }
        let mut body = format!(
            "<h1>{}</h1>\n<p class=\"muted\">{}</p>\n",
            self.t("Resources"),
            self.t("Every resource of the registry, by kind; what is within another one (an interface, a component) is on its page.")
        );
        body.push_str("<p class=\"toc\">");
        for (kind, items) in &by_kind {
            body.push_str(&format!("<a href=\"#{kind}\">{kind} <span class=\"muted\">{}</span></a> ", items.len()));
        }
        body.push_str("</p>\n");
        for (kind, items) in &by_kind {
            body.push_str(&format!(
                "<h2 id=\"{kind}\">{}{kind}</h2>\n<table>\n<tr><th>{}</th><th>{}</th><th>{}</th><th>{}</th><th>{}</th></tr>\n",
                kind_icon(kind),
                self.t("Name"),
                self.t("Label"),
                self.t("Site"),
                self.t("System"),
                self.t("Owner")
            ));
            for r in items {
                body.push_str(&format!(
                    "<tr><td><a href=\"resources/{}.html\">{}</a>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
                    file_name(&text(&r["id"])),
                    esc(&text(&r["id"])),
                    if r["incomplete"] == true { format!(" <span class=\"tag\">{}</span>", self.t("incomplete")) } else { String::new() },
                    esc(&text(&r["label"])),
                    r["site"].as_str().map(|s| self.resource_link("", s)).unwrap_or_default(),
                    r["system"].as_str().map(|s| self.resource_link("", s)).unwrap_or_default(),
                    r["owner"].as_str().map(|s| self.resource_link("", s)).unwrap_or_default(),
                ));
            }
            body.push_str("</table>\n");
        }
        self.page("", self.t("Resources"), &body)
    }

    fn checks(&self) -> String {
        let checks = &self.project["checks"];
        let mut body = format!(
            "<h1>{}</h1>\n<p class=\"muted\">{}</p>\n",
            self.t("Checks"),
            self.t("The rules of the project, run on its registry: its tests.")
        );
        let diagnostics: Vec<&Value> = self.project["diagnostics"].as_array().into_iter().flatten().collect();
        if checks.as_array().is_none_or(Vec::is_empty) {
            body.push_str(&format!("<p>{}</p>\n", self.t("No check in this project.")));
        }
        for check in checks.as_array().into_iter().flatten() {
            let result = text(&check["result"]);
            let info = check["severity"] == "info";
            let (status, shown) = match result.as_str() {
                "passed" => ("ok", self.t("passed")),
                "failed" if info => ("info", self.t("to know")),
                "failed" => ("ko", self.t("failed")),
                "cannot be evaluated" => ("ko", self.t("cannot be evaluated")),
                "nothing to run" => ("na", self.t("nothing to run")),
                _ => ("na", self.t("not run")),
            };
            body.push_str(&format!(
                "<section class=\"check {status}\">\n<h2>{} <span class=\"result\">{}</span></h2>\n",
                esc(&text(&check["name"])),
                esc(shown)
            ));
            if let Some(message) = check["message"].as_str() {
                body.push_str(&format!("<p>{}</p>\n", esc(message)));
            }
            let severity = match text(&check["severity"]).as_str() {
                "warning" => self.t("warning"),
                "info" => self.t("info"),
                _ => self.t("error"),
            };
            body.push_str(&format!(
                "<p class=\"muted\">{} · {} {severity} · {} {}</p>\n",
                esc(&text(&check["summary"])),
                self.t("severity"),
                self.t("declared at"),
                esc(&text(&check["declared"]))
            ));
            let failing = check["failing"].as_array().cloned().unwrap_or_default();
            if !failing.is_empty() {
                body.push_str("<ul>\n");
                for item in &failing {
                    body.push_str(&format!("<li>{}</li>\n", self.item_links("", &text(&item["item"]))));
                }
                body.push_str("</ul>\n");
            }
            body.push_str("</section>\n");
        }
        if !diagnostics.is_empty() {
            body.push_str(&format!(
                "<h2>{}</h2>\n<table>\n<tr><th>{}</th><th>{}</th><th>{}</th></tr>\n",
                self.t("Diagnostics"),
                self.t("Severity"),
                self.t("Message"),
                self.t("Where")
            ));
            for d in diagnostics {
                body.push_str(&format!(
                    "<tr class=\"{}\"><td>{}</td><td>{}{}</td><td>{}</td></tr>\n",
                    esc(&text(&d["severity"])),
                    match text(&d["severity"]).as_str() {
                        "warning" => self.t("warning"),
                        "info" => self.t("info"),
                        "incomplete" => self.t("incomplete"),
                        _ => self.t("error"),
                    },
                    esc(&text(&d["message"])),
                    d["help"].as_str().map(|h| format!(" <span class=\"muted\">{}</span>", esc(h))).unwrap_or_default(),
                    esc(&text(&d["at"]))
                ));
            }
            body.push_str("</table>\n");
        }
        self.page("", self.t("Checks"), &body)
    }

    /// The way of an edge, its steps separated by commas, the variants of a step by `or`:
    /// `sw-01 or sw-02, fw`.
    fn way(&self, root: &str, steps: &Value) -> String {
        steps
            .as_array()
            .into_iter()
            .flatten()
            .map(|step| {
                strings(step)
                    .iter()
                    .map(|c| self.resource_link(root, c))
                    .collect::<Vec<_>>()
                    .join(&format!(" {} ", self.t("or")))
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The API: the JSON files of the site, what each holds, with a live example of each.
    fn api_page(&self, views: &[Value], records: &[Value]) -> String {
        let example_resource = records.first().map(|r| text(&r["id"]));
        let example_view = views.first().map(|v| text(&v["name"]));
        let mut body = format!(
            "<h1>API</h1>\n<p class=\"lead\">{}</p>\n",
            self.t("The model of the project as JSON files, static, beside these pages: for scripts, dashboards and agents. Read them with <code>fetch</code> from a page, <code>curl</code> from a script, or as files in the deployed folder. The model is read only: writing is editing the sources and going through Git.")
        );
        body.push_str(&format!(
            "<p class=\"muted\">{}</p>\n",
            self.t("Format <code>cadastre-model</code>, version <b>{version}</b>: within a version a field is added, never removed nor renamed. Every resource is designated by its qualified name (<code>{example}</code>).")
                .replace("{version}", &self.project["version"].to_string())
                .replace("{example}", &esc(example_resource.as_deref().unwrap_or("dc1.lan.apps.web01")))
        ));
        body.push_str(&format!(
            "<h2>{}</h2>\n<table>\n<tr><th>{}</th><th>{}</th><th>{}</th></tr>\n",
            self.t("The files"),
            self.t("File"),
            self.t("What it holds"),
            self.t("Example")
        ));
        let mut row = |path: &str, what: &'static str, example: Option<String>| {
            let link = example.map(|e| format!("<a href=\"{e}\">{}</a>", esc(&e))).unwrap_or_default();
            body.push_str(&format!(
                "<tr><td><code>{}</code></td><td>{}</td><td>{link}</td></tr>\n",
                esc(path),
                self.t(what)
            ));
        };
        row(
            "api/project.json",
            "the project: its name, counts by kind, views, checks and their results, modules, files, diagnostics",
            Some("api/project.json".to_owned()),
        );
        row(
            "api/resources.json",
            "every resource as its short record: id, kind, name, label, owner, operator, site, system, incomplete, missing, declared",
            Some("api/resources.json".to_owned()),
        );
        row(
            "api/resources/<id>.json",
            "the full record of a resource: the short record, then virtual, description, parent, path, module, attributes (every key written), references (each reference key resolved to qualified names), children, members, hosts (what runs on it), runs_here (what runs on what it contains), failures (the impact analysis), failure_pairs (the pairs of failures its redundancy rests on), questions, outgoing and incoming links (with, port, layers, carries, through, through_steps, through_deduced, attributes, declared)",
            example_resource.map(|id| format!("api/resources/{}.json", file_name(&id))),
        );
        row(
            "api/links.json",
            "every link, both ends named: source, target, port, layers, carries, through, attributes, declared",
            Some("api/links.json".to_owned()),
        );
        row(
            "api/views/<name>.json",
            "a view evaluated: name, title, description, formats, declared, then nodes (id, kind, label, frame), frames (id, title, resource, in), edges (source, target, relation, ports, layers, carries, through, through_steps, step), diagnostics; absent while the project has errors",
            example_view.map(|name| format!("api/views/{}.json", file_name(&name))),
        );
        row(
            "api/checks.json",
            "the checks and their results: name, severity, message, declared, result (passed, failed, cannot be evaluated, nothing to run, not run), summary, selected, failing (item, declared)",
            Some("api/checks.json".to_owned()),
        );
        row(
            "api/search.json",
            "the index of the search of these pages: id, kind, label, words",
            Some("api/search.json".to_owned()),
        );
        row("views/<name>.svg", "the diagram of a view, when laid out at build time", None);
        body.push_str("</table>\n");
        body.push_str(&format!(
            "<h2>{}</h2>\n<ul>\n<li><code>cadastre export json</code>: {}</li>\n<li><code>cadastre mcp</code>: {}</li>\n<li><code>cadastre serve</code>: {}</li>\n</ul>\n",
            self.t("The same model elsewhere"),
            self.t("the whole model in one file."),
            self.t("the same records for an agent, through the Model Context Protocol."),
            self.t("these paths computed at each request from the files on disk.")
        ));
        self.page("", self.t("API"), &body)
    }

    /// A failing item of a check (`shop.front -> bank`, `shop.front`): its names linked.
    fn item_links(&self, root: &str, item: &str) -> String {
        let registry = &self.compilation.registry;
        item.split(' ')
            .map(|word| {
                if registry.ids().any(|r| registry.qualified_name(r) == word) {
                    format!("<a href=\"{root}resources/{}.html\">{}</a>", file_name(word), esc(word))
                } else {
                    esc(word)
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn view_page(&self, view: &Value, own: Option<&(String, String)>) -> String {
        let root = "../";
        let name = text(&view["name"]);
        let mut body = format!("<h1>{}</h1>\n", esc(&text(&view["title"])));
        if let Some(description) = view["description"].as_str() {
            body.push_str(&format!("<p class=\"lead\">{}</p>\n", esc(description)));
        }
        let mut actions = vec![format!("<a href=\"{root}api/views/{}.json\">JSON</a>", file_name(&name))];
        if self.options.playground {
            actions.push(format!(
                "<a href=\"{root}playground/#view={}\">{}</a>",
                esc(&name),
                self.t("Open in the playground")
            ));
        }
        body.push_str(&format!("<p class=\"actions\">{}</p>\n", actions.join(" · ")));
        // A view drawn as a sequence diagram or a traversal: that drawing first, the graph after.
        match own {
            Some((format, content)) if format == "traversal" => {
                body.push_str(&format!("<figure class=\"diagram\">{}</figure>\n", svg_body(content)));
            }
            Some((_, mermaid)) => {
                // The `link` lines of the participants become links on their boxes once drawn.
                let (links, text) = participant_links(mermaid);
                body.push_str(&format!(
                    "<figure class=\"diagram\" data-links=\"{}\"><pre class=\"mermaid\">{}</pre></figure>\n<script type=\"module\">{}</script>\n",
                    esc(&links.to_string()),
                    esc(&text),
                    MERMAID_SCRIPT.replace("{MERMAID_CDN}", MERMAID_CDN)
                ));
            }
            None => {}
        }
        if own.is_some() {
            body.push_str(&format!("<h2>{}</h2>\n", self.t("As a graph")));
        }
        match self.options.svgs.get(&name) {
            Some(svg) => body.push_str(&format!("<figure class=\"diagram\">{}</figure>\n", svg_body(svg))),
            None => {
                let dot =
                    dots(self.compilation).into_iter().find(|(n, _)| *n == name).map(|(_, d)| d).unwrap_or_default();
                match self.options.viz {
                    Some(viz) => body.push_str(&format!(
                        "<figure class=\"diagram\" data-viz=\"{}\"><script type=\"text/vnd.graphviz\">{}</script><p class=\"muted\">{}</p></figure>\n",
                        esc(viz),
                        dot.replace("</script", "<\\/script"),
                        self.t("Drawing…")
                    )),
                    None => body.push_str(&format!("<pre class=\"dot\">{}</pre>\n", esc(&dot))),
                }
            }
        }
        for warning in view["diagnostics"].as_array().into_iter().flatten() {
            body.push_str(&format!("<p class=\"warning\">{}</p>\n", esc(&text(&warning["message"]))));
        }
        let nodes = view["nodes"].as_array().cloned().unwrap_or_default();
        if !nodes.is_empty() {
            body.push_str(&format!(
                "<h2>{} <span class=\"muted\">{}</span></h2>\n<table>\n<tr><th>{}</th><th>{}</th><th>{}</th></tr>\n",
                self.t("Nodes"),
                nodes.len(),
                self.t("Resource"),
                self.t("Kind"),
                self.t("Frame")
            ));
            for node in &nodes {
                body.push_str(&format!(
                    "<tr><td>{}{}</td><td>{}</td><td>{}</td></tr>\n",
                    kind_icon(&text(&node["kind"])),
                    self.resource_link(root, &text(&node["id"])),
                    esc(&text(&node["kind"])),
                    esc(&frame_title(view, &node["frame"]))
                ));
            }
            body.push_str("</table>\n");
        }
        let edges = view["edges"].as_array().cloned().unwrap_or_default();
        if !edges.is_empty() {
            body.push_str(&format!(
                "<h2>{} <span class=\"muted\">{}</span></h2>\n<table>\n<tr><th>{}</th><th>{}</th><th>{}</th><th>{}</th><th>{}</th><th>{}</th></tr>\n",
                self.t("Edges"),
                edges.len(),
                self.t("From"),
                self.t("To"),
                self.t("Ports"),
                self.t("Layers"),
                self.t("Carries"),
                self.t("Through")
            ));
            for e in &edges {
                body.push_str(&format!(
                    "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
                    self.resource_link(root, &text(&e["source"])),
                    self.resource_link(root, &text(&e["target"])),
                    esc(&strings(&e["ports"]).join(", ")),
                    esc(&strings(&e["layers"]).join(", ")),
                    strings(&e["carries"]).iter().map(|c| self.resource_link(root, c)).collect::<Vec<_>>().join(", "),
                    self.way(root, &e["through_steps"]),
                ));
            }
            body.push_str("</table>\n");
        }
        self.page(root, &text(&view["title"]), &body)
    }

    fn resource_page(&self, record: &Value, records: &[Value], views: &[(String, String)]) -> String {
        let root = "../";
        let id = text(&record["id"]);
        let kind = text(&record["kind"]);
        let label = record["label"].as_str().map(str::to_owned).unwrap_or_else(|| text(&record["name"]));
        let mut body = String::new();
        // The path from the root, as a breadcrumb.
        let path = strings(&record["path"]);
        if !path.is_empty() {
            body.push_str("<p class=\"crumbs\">");
            body.push_str(&path.iter().map(|p| self.resource_link(root, p)).collect::<Vec<_>>().join(" › "));
            body.push_str("</p>\n");
        }
        body.push_str(&format!(
            "<h1>{}{} <span class=\"kind\">{}{}</span></h1>\n<p class=\"muted mono\">{}</p>\n",
            kind_icon(&kind),
            esc(&label),
            if record["virtual"] == true { format!("{} ", self.t("virtual")) } else { String::new() },
            esc(&kind),
            esc(&id)
        ));
        if let Some(description) = record["description"].as_str() {
            body.push_str(&format!("<p class=\"lead\">{}</p>\n", esc(description)));
        }
        if record["incomplete"] == true {
            let missing = strings(&record["missing"]);
            body.push_str(&format!("<p class=\"tag\">{}</p>\n", self.t("incomplete")));
            if !missing.is_empty() {
                body.push_str("<ul class=\"missing\">\n");
                for reason in &missing {
                    body.push_str(&format!("<li>{}</li>\n", esc(reason)));
                }
                body.push_str("</ul>\n");
            }
        }
        // The facts.
        let mut facts: Vec<(String, String)> = Vec::new();
        for (key, title) in
            [("owner", "Owner"), ("operator", "Operator"), ("site", "Site"), ("system", "System"), ("module", "Module")]
        {
            // A site or a system is not its own site or system.
            if let Some(value) = record[key].as_str().filter(|v| *v != id) {
                let shown = if key == "module" { esc(value) } else { self.resource_link(root, value) };
                facts.push((self.t(title).to_owned(), shown));
            }
        }
        if let Some(attributes) = record["attributes"].as_object() {
            for (key, value) in attributes {
                if ["label", "description", "question", "owner", "operator"].contains(&key.as_str()) {
                    continue;
                }
                let shown = match record["references"].get(key) {
                    Some(targets) => {
                        strings(targets).iter().map(|t| self.resource_link(root, t)).collect::<Vec<_>>().join(", ")
                    }
                    None => esc(&strings_or_text(value)),
                };
                facts.push((key.clone(), shown));
            }
        }
        if !facts.is_empty() {
            body.push_str("<dl class=\"facts\">\n");
            for (key, value) in &facts {
                body.push_str(&format!("<dt>{}</dt><dd>{value}</dd>\n", esc(key)));
            }
            body.push_str("</dl>\n");
        }
        let questions = strings(&record["questions"]);
        if !questions.is_empty() {
            body.push_str(&format!("<h2>{}</h2>\n<ul>\n", self.t("Open questions")));
            for q in &questions {
                body.push_str(&format!("<li>{}</li>\n", esc(q)));
            }
            body.push_str("</ul>\n");
        }
        // Lists of resources.
        let list = |body: &mut String, title: &'static str, ids: &[String]| {
            if ids.is_empty() {
                return;
            }
            body.push_str(&format!(
                "<h2>{} <span class=\"muted\">{}</span></h2>\n<ul class=\"list\">\n",
                self.t(title),
                ids.len()
            ));
            for i in ids {
                let kind = records.iter().find(|r| r["id"] == *i).map(|r| text(&r["kind"])).unwrap_or_default();
                body.push_str(&format!(
                    "<li>{}{} <span class=\"muted\">{}</span></li>\n",
                    kind_icon(&kind),
                    self.resource_link(root, i),
                    esc(&kind)
                ));
            }
            body.push_str("</ul>\n");
        };
        list(&mut body, "Contains", &strings(&record["children"]));
        list(&mut body, "Members", &strings(&record["members"]));
        list(&mut body, "Runs on it", &strings(&record["hosts"]));
        list(&mut body, "Runs here", &strings(&record["runs_here"]));
        // What this one owns, for a team.
        if kind == "team" {
            let owned: Vec<String> =
                records.iter().filter(|r| r["owner"] == id && r["id"] != id).map(|r| text(&r["id"])).collect();
            list(&mut body, "Owns", &owned);
        }
        for (key, title) in [("outgoing", "Outgoing links"), ("incoming", "Incoming links")] {
            let links = record[key].as_array().cloned().unwrap_or_default();
            if links.is_empty() {
                continue;
            }
            body.push_str(&format!(
                "<h2>{} <span class=\"muted\">{}</span></h2>\n<table>\n<tr><th>{}</th><th>{}</th><th>{}</th><th>{}</th><th>{}</th></tr>\n",
                self.t(title),
                links.len(),
                if key == "outgoing" { self.t("To") } else { self.t("From") },
                self.t("Port"),
                self.t("Layers"),
                self.t("Carries"),
                self.t("Through")
            ));
            for l in &links {
                body.push_str(&format!(
                    "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}{}</td></tr>\n",
                    self.resource_link(root, &text(&l["with"])),
                    esc(&text(&l["port"])),
                    esc(&strings(&l["layers"]).join(", ")),
                    strings(&l["carries"]).iter().map(|c| self.resource_link(root, c)).collect::<Vec<_>>().join(", "),
                    self.way(root, &l["through_steps"]),
                    if l["through_deduced"] == true {
                        format!(" <span class=\"muted\">({})</span>", self.t("deduced"))
                    } else {
                        String::new()
                    }
                ));
            }
            body.push_str("</table>\n");
        }
        let failures = strings(&record["failures"]);
        if !failures.is_empty() {
            body.push_str(&format!(
                "<h2>{} <span class=\"muted\">{}</span></h2>\n<p class=\"muted\">{}</p>\n<p>",
                self.t("Depends on"),
                failures.len(),
                self.t("Each of these alone, failing, makes it unavailable.")
            ));
            body.push_str(&failures.iter().map(|f| self.resource_link(root, f)).collect::<Vec<_>>().join(", "));
            body.push_str("</p>\n");
        }
        let pairs: Vec<Vec<String>> = record["failure_pairs"].as_array().into_iter().flatten().map(strings).collect();
        if !pairs.is_empty() {
            body.push_str(&format!(
                "<h2>{} <span class=\"muted\">{}</span></h2>\n<p class=\"muted\">{}</p>\n<ul class=\"list\">\n",
                self.t("Redundancy"),
                pairs.len(),
                self.t("Each of these pairs, failing together, makes it unavailable; neither alone does.")
            ));
            for pair in &pairs {
                let links: Vec<String> = pair.iter().map(|f| self.resource_link(root, f)).collect();
                body.push_str(&format!("<li>{}</li>\n", links.join(" + ")));
            }
            body.push_str("</ul>\n");
        }
        if !views.is_empty() {
            body.push_str(&format!("<h2>{}</h2>\n<ul class=\"list\">\n", self.t("Shown in")));
            for (name, title) in views {
                body.push_str(&format!(
                    "<li><a href=\"{root}views/{}.html\">{}</a></li>\n",
                    file_name(name),
                    esc(title)
                ));
            }
            body.push_str("</ul>\n");
        }
        body.push_str(&format!(
            "<p class=\"muted\">{} <code>{}</code> · <a href=\"{root}api/resources/{}.json\">JSON</a></p>\n",
            self.t("Declared at"),
            esc(&text(&record["declared"])),
            file_name(&id)
        ));
        self.page(root, &label, &body)
    }
}

fn strings_or_text(value: &Value) -> String {
    match value {
        Value::Array(items) => items.iter().map(text).collect::<Vec<_>>().join(", "),
        other => text(other),
    }
}

fn frame_title(view: &Value, frame: &Value) -> String {
    let Some(id) = frame.as_str() else { return String::new() };
    view["frames"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|f| f["id"] == id)
        .map(|f| text(&f["title"]))
        .unwrap_or_default()
}

fn plural(kind: &str, n: u64) -> String {
    if n == 1 { kind.to_owned() } else { crate::render::plural(kind) }
}

/// Mermaid for the browser, for the sequence diagrams of the site.
const MERMAID_CDN: &str = "https://cdn.jsdelivr.net/npm/mermaid@11.4.1/dist/mermaid.esm.min.mjs";

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).expect("serializable") + "\n"
}

/// The `<svg …>…</svg>` of a rendering, without the XML prolog.
/// The `link` lines of a sequence diagram taken out: participant to page, and the text left.
fn participant_links(mermaid: &str) -> (Value, String) {
    let mut links = serde_json::Map::new();
    let mut text = String::new();
    for line in mermaid.lines() {
        let link = line.trim_start().strip_prefix("link ").and_then(|rest| {
            let (participant, target) = rest.split_once(": ")?;
            Some((participant.to_owned(), target.split_once(" @ ")?.1.to_owned()))
        });
        match link {
            Some((participant, href)) => {
                links.insert(participant, Value::String(href));
            }
            None => {
                text.push_str(line);
                text.push('\n');
            }
        }
    }
    (Value::Object(links), text)
}

/// Draws the sequence diagrams, then wraps the boxes of their participants in their links.
const MERMAID_SCRIPT: &str = r#"import mermaid from "{MERMAID_CDN}";
mermaid.initialize({ startOnLoad: false, theme: matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "default" });
await mermaid.run({ querySelector: ".mermaid" });
for (const figure of document.querySelectorAll("figure[data-links]")) {
  const links = JSON.parse(figure.dataset.links);
  for (const rect of figure.querySelectorAll("rect.actor[name]")) {
    const href = links[rect.getAttribute("name")];
    const group = rect.parentNode;
    if (!href || group.parentNode.localName === "a") continue;
    const a = document.createElementNS("http://www.w3.org/2000/svg", "a");
    a.setAttribute("href", href);
    a.setAttribute("target", "_blank");
    group.replaceWith(a);
    a.append(group);
  }
}"#;

fn svg_body(svg: &str) -> String {
    svg.find("<svg").map_or_else(|| svg.to_owned(), |at| svg[at..].to_owned())
}

const CSS: &str = r#"
:root { --bg: #ffffff; --fg: #1f2328; --muted: #6b7280; --line: #d0d7de; --accent: #1d4ed8; --ok: #15803d; --ko: #b91c1c; --panel: #f6f8fa; }
@media (prefers-color-scheme: dark) { :root { --bg: #1b1f23; --fg: #d1d5db; --muted: #9ca3af; --line: #30363d; --accent: #60a5fa; --ok: #4ade80; --ko: #f87171; --panel: #22272e; } }
* { box-sizing: border-box; }
body { margin: 0; font: 15px/1.5 -apple-system, "Segoe UI", Helvetica, Arial, sans-serif; color: var(--fg); background: var(--bg); }
a { color: var(--accent); text-decoration: none; } a:hover { text-decoration: underline; }
header { display: flex; flex-wrap: wrap; gap: 1rem 1.5rem; align-items: center; padding: .6rem 1.5rem; border-bottom: 1px solid var(--line); background: var(--panel); position: sticky; top: 0; z-index: 2; }
header .brand { font-weight: 700; color: var(--fg); font-size: 1.1rem; }
header nav a { margin-right: 1rem; color: var(--fg); }
.search { position: relative; margin-left: auto; min-width: 16rem; }
.search input { width: 100%; padding: .4rem .6rem; border: 1px solid var(--line); border-radius: 6px; background: var(--bg); color: var(--fg); }
.search ul { position: absolute; right: 0; left: 0; margin: .2rem 0 0; padding: .3rem 0; list-style: none; background: var(--bg); border: 1px solid var(--line); border-radius: 6px; max-height: 20rem; overflow: auto; box-shadow: 0 6px 20px rgba(0,0,0,.15); }
.search li a { display: block; padding: .3rem .7rem; color: var(--fg); } .search li a:hover, .search li a.active { background: var(--panel); text-decoration: none; }
.search li .kind { color: var(--muted); font-size: .85em; margin-left: .4rem; }
main { max-width: 72rem; margin: 0 auto; padding: 1.5rem; }
footer { max-width: 72rem; margin: 2rem auto; padding: 0 1.5rem; color: var(--muted); font-size: .85rem; }
h1 { font-size: 1.7rem; margin: .2rem 0 .6rem; } h2 { font-size: 1.15rem; margin: 1.8rem 0 .6rem; border-bottom: 1px solid var(--line); padding-bottom: .2rem; }
h1 .kind { font-size: .9rem; font-weight: 400; color: var(--muted); margin-left: .4rem; }
.icon { width: 1em; height: 1em; vertical-align: -.15em; margin-right: .35em; }
.lead { font-size: 1.05rem; } .muted { color: var(--muted); } .mono { font-family: ui-monospace, Consolas, monospace; font-size: .9rem; }
.crumbs { color: var(--muted); font-size: .9rem; margin: 0; }
.counts { list-style: none; padding: 0; display: flex; flex-wrap: wrap; gap: .4rem 1.5rem; }
.cards { display: grid; grid-template-columns: repeat(auto-fill, minmax(17rem, 1fr)); gap: 1rem; }
.card { display: block; border: 1px solid var(--line); border-radius: 8px; padding: .8rem; color: var(--fg); background: var(--panel); }
.card:hover { text-decoration: none; border-color: var(--accent); }
.card h3 { margin: .5rem 0 .2rem; font-size: 1rem; } .card p { margin: 0; color: var(--muted); font-size: .9rem; }
.thumb { height: 9rem; overflow: hidden; background: var(--bg); border-radius: 4px; display: flex; align-items: center; justify-content: center; }
.thumb svg, .thumb img { max-width: 100%; max-height: 100%; width: auto; height: auto; }
h3.group { font-size: 1rem; color: var(--muted); margin: 1.2rem 0 .5rem; }
.missing { margin: .3rem 0 0; padding-left: 1.2rem; font-size: .9rem; }
.mermaid { font-size: .9rem; }
.check.info { border-color: var(--accent); }
@media (max-width: 40rem) { table { display: block; overflow-x: auto; white-space: nowrap; } }
.thumb.empty { color: var(--muted); font-size: .85rem; }
.diagram { margin: 1rem 0; overflow: auto; border: 1px solid var(--line); border-radius: 8px; padding: .5rem; background: var(--bg); }
.diagram svg { max-width: 100%; height: auto; }
.diagram a:hover { text-decoration: none; } .diagram a[*|href]:hover > text, .diagram a[*|href]:hover > g > text { text-decoration: underline; }
.list { list-style: none; padding: 0; } .list li { padding: .15rem 0; }
table { border-collapse: collapse; width: 100%; font-size: .92rem; } th, td { text-align: left; padding: .35rem .5rem; border-bottom: 1px solid var(--line); vertical-align: top; }
th { color: var(--muted); font-weight: 600; }
.facts { display: grid; grid-template-columns: max-content 1fr; gap: .25rem 1rem; } .facts dt { color: var(--muted); } .facts dd { margin: 0; }
.tag { display: inline-block; font-size: .8rem; color: var(--ko); border: 1px solid currentColor; border-radius: 4px; padding: 0 .3rem; }
.checks.ok a { color: var(--ok); } .checks.ko a { color: var(--ko); }
.check { border-left: 4px solid var(--line); padding-left: 1rem; margin: 1rem 0; } .check.ok { border-color: var(--ok); } .check.ko { border-color: var(--ko); }
.check .result { font-size: .85rem; font-weight: 400; color: var(--muted); }
.warning { color: var(--ko); } tr.error td:first-child { color: var(--ko); }
.toc a { margin-right: 1rem; } .actions { font-size: .9rem; }
pre.dot { font-size: .8rem; overflow: auto; }
@media print { header, footer, .actions, .search { display: none; } main { max-width: none; padding: 0; } }
"#;

const JS: &str = r#"
(function () {
  const root = document.body.dataset.root || "";
  const input = document.getElementById("search");
  const results = document.getElementById("results");
  let index = null, active = -1;
  const fileName = (id) => id.replace(/[^A-Za-z0-9._-]/g, "_");
  async function load() {
    if (index) return index;
    const response = await fetch(root + "api/search.json");
    index = response.ok ? await response.json() : [];
    return index;
  }
  function show(items) {
    results.replaceChildren(...items.slice(0, 12).map((r) => {
      const li = document.createElement("li");
      const a = document.createElement("a");
      a.href = root + "resources/" + fileName(r.id) + ".html";
      a.textContent = r.label;
      const kind = document.createElement("span");
      kind.className = "kind";
      kind.textContent = r.kind + (r.label !== r.id ? " · " + r.id : "");
      a.append(kind);
      li.append(a);
      return li;
    }));
    results.hidden = items.length === 0;
    active = -1;
  }
  input.addEventListener("input", async () => {
    const q = input.value.trim().toLowerCase();
    if (!q) return show([]);
    const all = await load();
    const starts = all.filter((r) => r.label.toLowerCase().startsWith(q) || r.id.toLowerCase().startsWith(q));
    const contains = all.filter((r) => !starts.includes(r) && r.text.toLowerCase().includes(q));
    show(starts.concat(contains));
  });
  input.addEventListener("keydown", (e) => {
    const links = [...results.querySelectorAll("a")];
    if (!links.length) return;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      active = (active + (e.key === "ArrowDown" ? 1 : -1) + links.length) % links.length;
      links.forEach((a, i) => a.classList.toggle("active", i === active));
    } else if (e.key === "Enter" && active >= 0) {
      location.href = links[active].href;
    } else if (e.key === "Escape") {
      show([]);
    }
  });
  document.addEventListener("click", (e) => { if (!e.target.closest(".search")) show([]); });
  // The diagrams given as DOT: drawn with Graphviz for the browser, their icons and dark theme
  // added by the module of the playground when the site ships it.
  const pending = [...document.querySelectorAll("[data-viz]")];
  if (pending.length) {
    const src = pending[0].dataset.viz;
    const script = document.createElement("script");
    script.src = src.startsWith("http") ? src : root + src;
    script.onload = async () => {
      const viz = await Viz.instance();
      let draw = (dot) => viz.renderString(dot, { format: "svg" });
      if (document.body.dataset.playground === "true") {
        try {
          const at = (path) => new URL(root + path, document.baseURI).href;
          const { load } = await import(at("playground/cadastre.js"));
          const cadastre = await load(at("playground/cadastre_wasm.wasm"));
          draw = (dot) => cadastre.draw(viz, dot);
        } catch (error) { console.warn("icons unavailable:", error); }
      }
      for (const box of pending) {
        try {
          const dot = box.querySelector("script").textContent;
          const svg = draw(dot);
          const element = new DOMParser().parseFromString(svg, "image/svg+xml").documentElement;
          if (box.classList.contains("thumb")) { element.removeAttribute("width"); element.removeAttribute("height"); }
          box.replaceChildren(document.importNode(element, true));
        } catch (error) {
          box.replaceChildren(Object.assign(document.createElement("p"), { textContent: document.body.dataset.cannotDraw + " " + error }));
        }
      }
    };
    script.onerror = () => { for (const box of pending) box.replaceChildren(Object.assign(document.createElement("p"), { textContent: document.body.dataset.noGraphviz })); };
    document.head.append(script);
  }
})();
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names() {
        assert_eq!(file_name("dc1.lan.web01"), "dc1.lan.web01");
        assert_eq!(file_name("a b/c"), "a_b_c");
    }
}

#[cfg(test)]
mod sequence_tests {
    #[test]
    fn participant_links_are_taken_out_of_the_sequence() {
        let mermaid = "sequenceDiagram
  participant n1 as shop
  link n1: Page @ ../resources/shop.html
  n1->>n1: work
";
        let (links, text) = super::participant_links(mermaid);
        assert_eq!(links, serde_json::json!({ "n1": "../resources/shop.html" }));
        assert_eq!(
            text,
            "sequenceDiagram
  participant n1 as shop
  n1->>n1: work
"
        );
    }
}
