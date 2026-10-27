//! Compiling a project: all the `.ca`, `.cav` and `.cac` files of a directory.

use std::collections::HashMap;

use crate::check::{self, Model, Options};
use crate::diagnostic::Diagnostic;
use crate::lower::Lowerer;
use crate::plan::{self, Plans};
use crate::registry::{Registry, ResourceId};
use crate::source::{FileId, FileRole, SourceFile, Span};
use crate::syntax;

/// Result of compiling a project.
#[derive(Debug, Default)]
pub struct Compilation {
    pub registry: Registry,
    pub model: Model,
    pub plans: Plans,
    pub diagnostics: Vec<Diagnostic>,
    /// The modules the project depends on, with their files.
    pub modules: Vec<Module>,
    /// The language of what is written for the readers (`Options::language`).
    pub language: crate::i18n::Language,
}

/// A module of the project (a dependency of its `cadastre.toml`): the files of another project,
/// read only. Its schema, registry, styles and checks join the project; its views are not built;
/// only its errors are reported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Module {
    pub name: String,
    /// The indices of its files among the files compiled.
    pub files: std::ops::Range<usize>,
}

impl Compilation {
    /// The module a file belongs to; `None` for a file of the project itself.
    pub fn module_of(&self, file: FileId) -> Option<&str> {
        let index = file.0 as usize;
        self.modules.iter().find(|m| m.files.contains(&index)).map(|m| m.name.as_str())
    }

    /// The resources designated by the name at a byte offset of a file: the target of a
    /// reference (several when it is ambiguous), or the resource a declaration names.
    pub fn definitions(&self, file: FileId, offset: usize) -> Vec<ResourceId> {
        let contains = |span: &Span| span.file == file && span.start <= offset && offset <= span.end;
        if let Some(id) = self.registry.ids().find(|&id| contains(&self.registry.resource(id).name_span)) {
            return vec![id];
        }
        self.model
            .names
            .iter()
            .filter(|(span, _)| contains(span))
            .min_by_key(|(span, _)| span.end - span.start)
            .map(|(_, targets)| targets.clone())
            .unwrap_or_default()
    }

    /// Where a resource is named: its declaration first (with `declaration`), then, in text
    /// order, every name of the registry and the plans that designates it alone (an ambiguous
    /// name designates none of its candidates).
    pub fn references(&self, id: ResourceId, declaration: bool) -> Vec<Span> {
        let mut spans: Vec<Span> =
            self.model.names.iter().filter(|(_, targets)| targets.as_slice() == [id]).map(|(span, _)| *span).collect();
        spans.sort_by_key(|span| (span.file, span.start));
        spans.dedup_by_key(|span| (span.file, span.start, span.end));
        if declaration {
            spans.insert(0, self.registry.resource(id).name_span);
        }
        spans
    }
}

/// Compiles a project from already-read files.
///
/// File order does not matter (spec § 1); files without a `.ca`, `.cav` or `.cac`
/// extension are ignored.
pub fn compile(files: &[SourceFile]) -> Compilation {
    compile_with(files, Options::default())
}

pub fn compile_with(files: &[SourceFile], options: Options) -> Compilation {
    compile_modules(files, &[], options)
}

/// Compiles a project whose files include those of its modules.
pub fn compile_modules(files: &[SourceFile], modules: &[Module], options: Options) -> Compilation {
    let mut compilation =
        Compilation { modules: modules.to_vec(), language: options.language, ..Compilation::default() };

    for (index, file) in files.iter().enumerate() {
        let Some(role) = FileRole::from_path(&file.path) else {
            continue;
        };
        let tree = syntax::parse(&file.text);
        let (resources, links) = (compilation.registry.resources.len(), compilation.registry.links.len());
        Lowerer { registry: &mut compilation.registry, diagnostics: &mut compilation.diagnostics, files }.lower_file(
            FileId(index as u32),
            role,
            &file.text,
            tree.root_node(),
        );
        let unbound = crate::lower::unbound_variables(&compilation.registry, resources, links);
        compilation.diagnostics.extend(unbound);
        if role.is_plans() {
            plan::lower_plans(
                FileId(index as u32),
                role,
                &file.text,
                tree.root_node(),
                &mut compilation.plans,
                &mut compilation.diagnostics,
            );
        }
    }

    // The schema of the project, before anything reads the kinds and the keys.
    compilation.registry.schema = crate::declare::schema(
        &compilation.registry.declarations,
        &compilation.registry.link_declarations,
        &compilation.registry.group_declarations,
        &mut compilation.diagnostics,
    );
    // `parent:` places resources before anything reads where they are.
    let placed = crate::place::place(&mut compilation.registry, &mut compilation.diagnostics);
    let duplicates = duplicate_names(&compilation);
    compilation.diagnostics.extend(duplicates);
    compilation.model = check::check(&compilation.registry, options, &mut compilation.diagnostics);
    compilation.model.names.extend(placed);
    crate::view::validate(
        &compilation.plans,
        &compilation.registry,
        &mut compilation.model.names,
        &mut compilation.diagnostics,
    );
    // Checks run on a valid model: on an invalid one, their failures would be noise.
    if !compilation.diagnostics.iter().any(|d| d.severity == crate::diagnostic::Severity::Error) {
        let failures = crate::rules::run(&compilation);
        compilation.diagnostics.extend(failures);
    }
    // A module is read only: its views are not built, and of what it says about itself only its
    // errors are reported.
    if !compilation.modules.is_empty() {
        let modules = compilation.modules.clone();
        let foreign = |span: &Span| modules.iter().any(|m| m.files.contains(&(span.file.0 as usize)));
        let views = std::mem::take(&mut compilation.plans.views);
        compilation.plans.views = views.into_iter().filter(|v| !foreign(&v.span)).collect();
        let diagnostics = std::mem::take(&mut compilation.diagnostics);
        compilation.diagnostics = diagnostics
            .into_iter()
            .filter(|d| d.severity == crate::diagnostic::Severity::Error || !d.spans.iter().all(&foreign))
            .collect();
    }
    compilation
}

/// A rendering of a view. For `svg`, `content` is DOT: the host turns it into SVG
/// with Graphviz (`dot -Tsvg` natively, a WebAssembly build of Graphviz in the browser).
#[derive(Clone, Debug)]
pub struct Output {
    pub view: String,
    pub format: String,
    pub content: String,
}

/// Evaluates and renders the views of a compiled project (all of them, or `only` one).
pub fn build(compilation: &Compilation, only: Option<&str>) -> (Vec<Output>, Vec<Diagnostic>) {
    build_with(compilation, only, None)
}

/// Like [`build`], with the formats of every view replaced by `formats` when given.
pub fn build_with(
    compilation: &Compilation,
    only: Option<&str>,
    formats: Option<&[String]>,
) -> (Vec<Output>, Vec<Diagnostic>) {
    let mut outputs = Vec::new();
    let mut diagnostics = Vec::new();
    for view in &compilation.plans.views {
        if only.is_some_and(|name| name != view.name) {
            continue;
        }
        let graph = crate::view::evaluate(view, &compilation.registry, &compilation.model, &mut diagnostics);
        let scene = crate::render::Scene::new(&graph, &compilation.plans, &compilation.registry, &compilation.model)
            .with_language(compilation.language);
        // The style rules of the view: one that applies to nothing is likely a mistaken filter.
        for rule in view.style.iter().filter(|rule| !scene.applies_somewhere(rule)) {
            let mut d = Diagnostic::warning(
                format!("view {}: this style rule applies to nothing in the view", view.name),
                rule.span,
            );
            if let Some(hint) = rule.filter.as_ref().and_then(|f| crate::view::filter_hint(&compilation.registry, f)) {
                d = d.with_help(hint);
            }
            diagnostics.push(d);
        }
        let stack = view.render.iter().flat_map(|r| r.options.iter()).find(|o| o.key == "stack");
        for conflict in scene.stack_conflicts() {
            diagnostics.push(
                Diagnostic::warning(
                    format!("view {}: {conflict}", view.name),
                    stack.map_or(view.span, |s| s.value.span),
                )
                .with_help("put them on adjacent levels, or group the view so that they are not in the same frame"),
            );
        }
        for format in formats.unwrap_or(&graph.options.formats) {
            let content = match format.as_str() {
                "text" => crate::render::text(&scene),
                "mermaid" => crate::render::mermaid(&scene),
                "sequence" => {
                    if graph.processes.is_empty() {
                        diagnostics.push(
                            Diagnostic::warning(
                                format!("view {}: render sequence draws the steps of a process", view.name),
                                view.span,
                            )
                            .with_help("select one: `edges process(orders)`"),
                        );
                    }
                    crate::render::sequence::sequence(&scene)
                }
                "traversal" => {
                    if graph.processes.is_empty() && graph.paths.is_empty() {
                        diagnostics.push(
                            Diagnostic::warning(
                                format!(
                                    "view {}: render traversal draws the steps of a process, or a path between two resources",
                                    view.name
                                ),
                                view.span,
                            )
                            .with_help("select one: `edges process(orders)`, `edges path(proxy, esx02)`"),
                        );
                    }
                    crate::render::traversal::traversal(&scene)
                }
                "ascii" => crate::render::ascii(&scene),
                "csv" => crate::render::table::table(&scene, crate::render::table::Table::Csv),
                "tsv" => crate::render::table::table(&scene, crate::render::table::Table::Tsv),
                "markdown" => crate::render::table::table(&scene, crate::render::table::Table::Markdown),
                // Laid out by the compiler; hosts with Graphviz lay it out like the SVG ([`drawio`]).
                "drawio" => crate::render::drawio::drawio(&scene, None),
                // Binary: in base64 (see [`xlsx`] for the bytes).
                "xlsx" => crate::render::base64(&crate::render::xlsx::xlsx(&scene)),
                "html" => crate::render::table::table(&scene, crate::render::table::Table::Html),
                // `layout: elk`: laid out by ELK and drawn by the compiler; Graphviz otherwise,
                // and when ELK cannot lay it out.
                "svg" if graph.options.elk => elk_svg(&scene, &view.name, view.span, &mut diagnostics)
                    .unwrap_or_else(|| crate::render::dot(&scene)),
                _ => crate::render::dot(&scene),
            };
            outputs.push(Output { view: view.name.clone(), format: format.clone(), content });
        }
    }
    (outputs, diagnostics)
}

/// The SVG of a view laid out by ELK (`layout: elk`); `None`, and a warning, when it cannot be:
/// the view is then laid out by Graphviz.
fn elk_svg(
    scene: &crate::render::Scene,
    view: &str,
    span: crate::source::Span,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<String> {
    // What ELK cannot apply: it routes the edges orthogonally, their labels laid out with them.
    let options = &scene.graph.options;
    for (set, option) in
        [(options.routing != crate::view::Routing::Curved, "routing"), (options.free_labels, "edge_labels_layout")]
    {
        if set {
            diagnostics.push(
                Diagnostic::warning(format!("view {view}: {option} does not apply with layout: elk"), span)
                    .with_help("ELK routes the edges orthogonally, their labels on them"),
            );
        }
    }
    #[cfg(feature = "elk")]
    match crate::render::elk::svg(scene) {
        Ok(svg) => return Some(svg),
        Err(error) => diagnostics
            .push(Diagnostic::warning(format!("view {view}: {error}"), span).with_help("laid out by Graphviz instead")),
    }
    #[cfg(not(feature = "elk"))]
    {
        let _ = scene;
        diagnostics.push(
            Diagnostic::warning(
                format!("view {view}: layout: elk needs a compiler built with the `elk` feature"),
                span,
            )
            .with_help("laid out by Graphviz instead"),
        );
    }
    None
}

/// The draw.io file of a view, laid out by Graphviz: `layout` is the `-Tjson` output of the DOT
/// of the view (its `dot` output); without it, or when it does not match the view, the layout of
/// the compiler (that of the ASCII art). `None` for an unknown view.
pub fn drawio(compilation: &Compilation, view: &str, layout: Option<&str>) -> Option<String> {
    let view = compilation.plans.views.iter().find(|v| v.name == view)?;
    let graph = crate::view::evaluate(view, &compilation.registry, &compilation.model, &mut Vec::new());
    let scene = crate::render::Scene::new(&graph, &compilation.plans, &compilation.registry, &compilation.model)
        .with_language(compilation.language);
    Some(crate::render::drawio::drawio(&scene, layout))
}

/// The Excel workbook of a table view; `None` for an unknown view.
pub fn xlsx(compilation: &Compilation, view: &str) -> Option<Vec<u8>> {
    let view = compilation.plans.views.iter().find(|v| v.name == view)?;
    let graph = crate::view::evaluate(view, &compilation.registry, &compilation.model, &mut Vec::new());
    let scene = crate::render::Scene::new(&graph, &compilation.plans, &compilation.registry, &compilation.model)
        .with_language(compilation.language);
    Some(crate::render::xlsx::xlsx(&scene))
}

/// A qualified name can only be declared once (spec § 5, Names).
fn duplicate_names(compilation: &Compilation) -> Vec<Diagnostic> {
    let registry = &compilation.registry;
    let mut diagnostics = Vec::new();
    let mut seen: HashMap<String, crate::registry::ResourceId> = HashMap::new();
    for id in registry.ids() {
        let name = registry.qualified_name(id);
        match seen.get(&name) {
            Some(&first) => {
                // Where each one comes from, when a module is involved.
                let origin = |r: ResourceId| match compilation.module_of(registry.resource(r).name_span.file) {
                    Some(module) => format!("module {module}"),
                    None => "the project".to_owned(),
                };
                let (a, b) = (origin(first), origin(id));
                let message = if a == b && a == "the project" {
                    format!("qualified name declared twice: {name}")
                } else {
                    format!("qualified name declared twice: {name}, in {a} and in {b}")
                };
                diagnostics.push(
                    Diagnostic::error(message, registry.resource(id).name_span)
                        .with_span(registry.resource(first).name_span),
                )
            }
            None => {
                seen.insert(name, id);
            }
        }
    }
    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Severity;

    fn file(path: &str, text: &str) -> SourceFile {
        SourceFile { path: path.into(), text: text.into() }
    }

    fn errors(c: &Compilation) -> Vec<&Diagnostic> {
        c.diagnostics.iter().filter(|d| d.severity == Severity::Error).collect()
    }

    #[test]
    fn builds_registry_and_qualified_names() {
        let c = compile(&[file(
            "infra.ca",
            "site s { vpc prod { subnet dmz { server web01 { os: debian12 } } } }\n\
             data customer\n\
             app billing { on: web01, -> crm : https/443 [flow] { carries: [customer] } }\n\
             app crm { on: web01 }\n\
             billing -> crm : tcp/22",
        )]);
        assert!(errors(&c).is_empty(), "{:?}", errors(&c));
        let names: Vec<String> = c.registry.ids().map(|id| c.registry.qualified_name(id)).collect();
        assert_eq!(names, ["s", "s.prod", "s.prod.dmz", "s.prod.dmz.web01", "customer", "billing", "crm"]);
        assert_eq!(c.registry.links.len(), 2);
        assert_eq!(c.registry.links[0].port.as_ref().unwrap().normalized(), "https/443");
    }

    #[test]
    fn rejects_facts_in_plans_and_views_in_registries() {
        let c = compile(&[file("views.cav", "server tmp01"), file("infra.ca", "view v { select app render svg }")]);
        assert_eq!(errors(&c).len(), 2, "{:?}", errors(&c));
    }

    #[test]
    fn reports_duplicate_qualified_names() {
        let c = compile(&[file("a.ca", "system s { app x }"), file("b.ca", "system s { }\napp y")]);
        let duplicates: Vec<_> = errors(&c).into_iter().filter(|d| d.message.contains("declared twice")).collect();
        assert_eq!(duplicates.len(), 1);
    }

    #[test]
    fn finds_references() {
        let registry = "system sales { app ihm { -> db : jdbc } database db }\n\
                        system stock { database db }\n\
                        server web01\n\
                        app x { on: web01, -> ihm, -> db }\n\
                        app y { on: web01 }\n";
        let plans = "view v { select app where name in [ihm, nowhere] edges follow(nothing, from: ihm) }";
        let c = compile(&[file("a.ca", registry), file("v.cav", plans)]);
        let id = |name: &str| c.registry.ids().find(|&i| c.registry.qualified_name(i) == name).unwrap();
        let places = |name: &str, declaration: bool| -> Vec<(u32, &str)> {
            c.references(id(name), declaration)
                .into_iter()
                .map(|s| (s.file.0, if s.file.0 == 0 { &registry[s.start..s.end] } else { &plans[s.start..s.end] }))
                .collect()
        };
        // The declaration, then the names that designate it, in the registry and the plans.
        assert_eq!(places("web01", true), [(0, "web01"), (0, "web01"), (0, "web01")]);
        assert_eq!(places("web01", false).len(), 2);
        // In the plans: a name of a filter compared to `name`, and the start of a `follow`.
        assert_eq!(places("sales.ihm", false), [(0, "ihm"), (1, "ihm"), (1, "ihm")]);
        // `db` in `sales` resolves by lexical scope; the ambiguous `-> db` of `x` is no reference.
        assert_eq!(places("sales.db", false), [(0, "db")]);
        assert!(places("stock.db", false).is_empty());
    }

    #[test]
    fn finds_definitions() {
        let registry = "system sales { app ihm { -> db : jdbc } database db }
                        system stock { database db }
                        server web01
                        app x { on: web01, -> db }
";
        let plans = "view v { select app edges follow(nothing, from: ihm) }";
        let c = compile(&[file("a.ca", registry), file("v.cav", plans)]);
        let at = |text: &str, needle: &str, nth: usize| text.match_indices(needle).nth(nth).unwrap().0 + 1;
        let found = |file: u32, offset: usize| -> Vec<String> {
            c.definitions(FileId(file), offset).into_iter().map(|id| c.registry.qualified_name(id)).collect()
        };
        // Reference resolved in the lexical scope of its system.
        assert_eq!(found(0, at(registry, "-> db", 0) + 3), ["sales.db"]);
        // A declaration designates itself.
        assert_eq!(found(0, at(registry, "ihm", 0)), ["sales.ihm"]);
        // Attribute reference; ambiguous link target: every candidate.
        assert_eq!(found(0, at(registry, "web01", 1)), ["web01"]);
        assert_eq!(found(0, at(registry, "-> db", 1) + 3), ["sales.db", "stock.db"]);
        // Names of plans; nothing on a keyword or a dangling name.
        assert_eq!(found(1, at(plans, "ihm", 0)), ["sales.ihm"]);
        assert!(found(1, 1).is_empty());
        assert!(found(1, at(plans, "nothing", 0)).is_empty());
    }

    #[test]
    fn reports_syntax_errors() {
        let c = compile(&[file("a.ca", "app x {\n  a -> b\n}\n")]);
        let syntax: Vec<_> = errors(&c).into_iter().filter(|d| d.message.starts_with("invalid syntax")).collect();
        assert_eq!(syntax.len(), 1);
        assert_eq!(syntax[0].spans[0].line, 1);
    }

    #[test]
    fn normalizes_ports() {
        let c = compile(&[file("a.ca", "a -> b : HTTPS/0443\na -> c : tcp/08000-8100")]);
        assert_eq!(c.registry.links[0].port.as_ref().unwrap().normalized(), "https/443");
        assert_eq!(c.registry.links[1].port.as_ref().unwrap().normalized(), "tcp/8000-8100");
    }
}
