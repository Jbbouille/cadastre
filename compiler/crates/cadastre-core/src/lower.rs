//! Syntax tree → registry.
//!
//! Reports syntax errors and the file-extension rule (spec § 1):
//! no resource nor link in a `.cav`, no view nor style in a `.ca`.
//!
//! A `for_each` is expanded here: its body is lowered once per item of its list, each `$variable`
//! replaced, in names, values and strings, by the item.

use tree_sitter::Node;

use crate::diagnostic::Diagnostic;
use crate::registry::{
    Attribute, Link, LinkSource, PortSpec, Reference, Registry, Resource, ResourceId, Value, ValueKind,
};
use crate::source::{FileId, FileRole, Span};

pub struct Lowerer<'a> {
    pub registry: &'a mut Registry,
    pub diagnostics: &'a mut Vec<Diagnostic>,
    /// Every file of the project, for the CSV files of `for_each`.
    pub files: &'a [crate::source::SourceFile],
}

impl Lowerer<'_> {
    pub fn lower_file(&mut self, file: FileId, role: FileRole, source: &str, root: Node) {
        let ctx = Ctx { file, source, variables: Vec::new() };
        self.syntax_errors(&ctx, root);

        let mut cursor = root.walk();
        for node in root.named_children(&mut cursor) {
            match (node.kind(), role) {
                ("resource", FileRole::Registry) => {
                    self.lower_resource(&ctx, node, None);
                }
                ("link", FileRole::Registry) => {
                    let link = self.lower_link(&ctx, node, None);
                    self.registry.links.push(link);
                }
                ("for_each", FileRole::Registry) => self.lower_for_each(&ctx, node, None),
                ("process", FileRole::Registry) => self.lower_process(&ctx, node, None),
                ("process", FileRole::Plans | FileRole::Checks) => {
                    self.diagnostics.push(
                        Diagnostic::error("a process cannot be declared in a plan file (.cav)", ctx.span(&node))
                            .with_help("declare it in a .ca file, beside the links of its steps"),
                    );
                }
                ("kind_declaration", FileRole::Registry) => {
                    let declaration = self.lower_kind_declaration(&ctx, node);
                    self.registry.declarations.push(declaration);
                }
                ("link_declaration", FileRole::Registry) => {
                    let layer = node.child_by_field_name("layer").map(|l| (ctx.raw(&l).to_owned(), ctx.span(&l)));
                    let keys = self.lower_key_declarations(&ctx, node);
                    self.registry.link_declarations.push(crate::declare::LinkDecl {
                        layer,
                        span: ctx.span(&node),
                        keys,
                    });
                }
                ("group_declaration", FileRole::Registry) => {
                    let Some(name) = node.child_by_field_name("name") else { continue };
                    let mut cursor = node.walk();
                    let members = node
                        .children_by_field_name("member", &mut cursor)
                        .map(|m| (ctx.raw(&m).to_owned(), ctx.span(&m)))
                        .collect();
                    self.registry.group_declarations.push(crate::declare::GroupDecl {
                        name: ctx.raw(&name).to_owned(),
                        name_span: ctx.span(&name),
                        members,
                    });
                }
                ("kind_declaration" | "link_declaration" | "group_declaration", FileRole::Plans | FileRole::Checks) => {
                    self.diagnostics.push(
                        Diagnostic::error("a kind cannot be declared in a plan file (.cav, .cac)", ctx.span(&node))
                            .with_help("declare the schema in a .ca file, such as schema.ca"),
                    );
                }
                ("resource" | "link" | "for_each", FileRole::Plans | FileRole::Checks) => {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "a resource or a link cannot be declared in a plan file (.cav)",
                            ctx.span(&node),
                        )
                        .with_help("declare facts in a .ca file"),
                    );
                }
                ("view" | "style_block", FileRole::Registry) => {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "a view or a style cannot be declared in a registry file (.ca)",
                            ctx.span(&node),
                        )
                        .with_help("declare plans in a .cav file"),
                    );
                }
                ("view" | "style_block", FileRole::Checks) => {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "a view or a style cannot be declared in a checks file (.cac)",
                            ctx.span(&node),
                        )
                        .with_help("a .cac file holds checks only: declare views and styles in a .cav file"),
                    );
                }
                ("check", FileRole::Registry | FileRole::Plans) => {
                    self.diagnostics.push(
                        Diagnostic::error("a check belongs in a checks file (.cac)", ctx.span(&node)).with_help(
                            "the checks are the tests of the project: move it to a .cac file, such as checks.cac",
                        ),
                    );
                }
                // Plans are handled by the views module.
                _ => {}
            }
        }
    }

    /// A `kind` declaration: its keys and their types, as written.
    fn lower_kind_declaration(&mut self, ctx: &Ctx, node: Node) -> crate::declare::KindDecl {
        let name = node.child_by_field_name("name");
        crate::declare::KindDecl {
            name: name.map(|n| ctx.raw(&n).to_owned()).unwrap_or_default(),
            name_span: name.map_or_else(|| ctx.span(&node), |n| ctx.span(&n)),
            keys: self.lower_key_declarations(ctx, node),
        }
    }

    /// The keys of a `kind`, `link` or `layer` declaration, as written.
    fn lower_key_declarations(&mut self, ctx: &Ctx, node: Node) -> Vec<crate::declare::KeyDecl> {
        use crate::declare::{KeyDecl, TypeDecl, TypeForm};
        let mut keys = Vec::new();
        let mut cursor = node.walk();
        for key in node.named_children(&mut cursor).filter(|n| n.kind() == "key_declaration") {
            let (Some(key_node), Some(ty)) = (key.child_by_field_name("key"), key.child_by_field_name("type")) else {
                continue;
            };
            let identifiers = |n: Node| -> Vec<String> {
                let mut c = n.walk();
                n.named_children(&mut c).filter(|i| i.kind() == "identifier").map(|i| ctx.raw(&i).to_owned()).collect()
            };
            let form = if let Some(kinds) = ty.child_by_field_name("kinds") {
                TypeForm::Ref(identifiers(kinds))
            } else if let Some(pattern) = ty.child_by_field_name("pattern") {
                // As written between the quotes: `\d` stays a class of the regular expression.
                let raw = ctx.raw(&pattern);
                let inner = raw.strip_prefix('"').and_then(|s| s.strip_suffix('"')).unwrap_or(raw);
                TypeForm::Pattern(inner.replace("\\\"", "\""))
            } else if let Some(values) = ty.child_by_field_name("values") {
                TypeForm::Choices(identifiers(values))
            } else {
                TypeForm::Named(ty.child_by_field_name("name").map(|n| ctx.raw(&n).to_owned()).unwrap_or_default())
            };
            let card = ty.child_by_field_name("cardinality").map(|c| {
                let raw = ctx.raw(&c);
                match raw {
                    "+" => crate::schema::Card::OneOrMore,
                    "*" => crate::schema::Card::Many,
                    _ => crate::schema::Card::Exactly(raw.trim_matches(['{', '}', ' ']).parse().unwrap_or(1)),
                }
            });
            keys.push(KeyDecl {
                key: ctx.raw(&key_node).to_owned(),
                key_span: ctx.span(&key_node),
                ty: TypeDecl { form, card, span: ctx.span(&ty) },
                required: key.child_by_field_name("required").is_some(),
            });
        }
        keys
    }

    fn syntax_errors(&mut self, ctx: &Ctx, root: Node) {
        if !root.has_error() {
            return;
        }
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            if node.is_error() {
                let excerpt: String = ctx.raw(&node).chars().take(40).collect();
                self.diagnostics
                    .push(Diagnostic::error(format!("invalid syntax: `{}`", excerpt.trim()), ctx.span(&node)));
                continue;
            }
            if node.is_missing() {
                self.diagnostics
                    .push(Diagnostic::error(format!("invalid syntax: `{}` expected", node.kind()), ctx.span(&node)));
                continue;
            }
            if node.has_error() {
                let mut cursor = node.walk();
                stack.extend(node.children(&mut cursor));
            }
        }
    }

    fn lower_resource(&mut self, ctx: &Ctx, node: Node, parent: Option<ResourceId>) {
        let (Some(kind), Some(name)) = (node.child_by_field_name("kind"), node.child_by_field_name("name")) else {
            return;
        };
        let id = ResourceId(self.registry.resources.len() as u32);
        self.registry.resources.push(Resource {
            kind: ctx.text(&kind).to_owned(),
            kind_span: ctx.span(&kind),
            name: ctx.text(&name).to_owned(),
            name_span: ctx.span(&name),
            parent,
            children: Vec::new(),
            attributes: Vec::new(),
            span: ctx.span(&node),
            virtual_span: node.child_by_field_name("virtual").map(|v| ctx.span(&v)),
        });
        if let Some(p) = parent {
            self.registry.resources[p.0 as usize].children.push(id);
        }

        let Some(body) = node.child_by_field_name("body") else {
            return;
        };
        let mut cursor = body.walk();
        for element in body.named_children(&mut cursor) {
            match element.kind() {
                "attribute" => {
                    if let Some(attribute) = lower_attribute(ctx, element) {
                        self.registry.resources[id.0 as usize].attributes.push(attribute);
                    }
                }
                "resource" => self.lower_resource(ctx, element, Some(id)),
                "process" => self.lower_process(ctx, element, Some(id)),
                "inline_link" => {
                    let link = self.lower_link(ctx, element, Some(id));
                    self.registry.links.push(link);
                }
                "for_each" => self.lower_for_each(ctx, element, Some(id)),
                "selection" => {
                    let select = crate::plan::lower_select(ctx.file, ctx.source, element);
                    self.registry.selections.push((id, select));
                }
                _ => {}
            }
        }
    }

    /// A `process`: a resource of kind `process`, and its steps.
    fn lower_process(&mut self, ctx: &Ctx, node: Node, parent: Option<ResourceId>) {
        let Some(name) = node.child_by_field_name("name") else { return };
        let id = ResourceId(self.registry.resources.len() as u32);
        let keyword = node.child(0).unwrap_or(node);
        let mut cursor = node.walk();
        let attributes = node
            .named_children(&mut cursor)
            .filter(|n| n.kind() == "attribute")
            .filter_map(|n| lower_attribute(ctx, n))
            .collect();
        self.registry.resources.push(Resource {
            kind: "process".to_owned(),
            kind_span: ctx.span(&keyword),
            name: ctx.text(&name).to_owned(),
            name_span: ctx.span(&name),
            parent,
            children: Vec::new(),
            attributes,
            span: ctx.span(&node),
            virtual_span: None,
        });
        if let Some(p) = parent {
            self.registry.resources[p.0 as usize].children.push(id);
        }
        let mut cursor = node.walk();
        for participant in node.named_children(&mut cursor).filter(|n| n.kind() == "participant") {
            self.lower_participant(ctx, participant, id);
        }
        let items = self.lower_process_items(ctx, node);
        self.registry.processes.push(crate::process::Process { id, items });
    }

    /// `participant esx "ESXi hosts" [esx1, esx2]`: a resource of kind `participant` in its
    /// process, its `label` and its `members`.
    fn lower_participant(&mut self, ctx: &Ctx, node: Node, process: ResourceId) {
        let Some(name) = node.child_by_field_name("name") else { return };
        let keyword = node.child(0).unwrap_or(node);
        let mut attributes = Vec::new();
        if let Some(label) = node.child_by_field_name("label").and_then(|l| lower_value(ctx, l).map(|v| (l, v))) {
            attributes.push(Attribute { key: "label".to_owned(), key_span: ctx.span(&label.0), value: label.1 });
        }
        if let Some(members) = node.child_by_field_name("members").and_then(|m| lower_value(ctx, m).map(|v| (m, v))) {
            attributes.push(Attribute { key: "members".to_owned(), key_span: ctx.span(&members.0), value: members.1 });
        }
        let id = ResourceId(self.registry.resources.len() as u32);
        self.registry.resources.push(Resource {
            kind: "participant".to_owned(),
            kind_span: ctx.span(&keyword),
            name: ctx.text(&name).to_owned(),
            name_span: ctx.span(&name),
            parent: Some(process),
            children: Vec::new(),
            attributes,
            span: ctx.span(&node),
            virtual_span: None,
        });
        self.registry.resources[process.0 as usize].children.push(id);
    }

    fn lower_process_items(&mut self, ctx: &Ctx, node: Node) -> Vec<crate::process::Item> {
        use crate::process::{Item, Step};
        let mut items = Vec::new();
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            match child.kind() {
                "step" => {
                    let link = self.lower_link(ctx, child, None);
                    let text = child.child_by_field_name("text").map(|t| unescape(ctx.raw(&t)));
                    let flags = child
                        .child_by_field_name("body")
                        .map(|body| {
                            let mut c = body.walk();
                            body.children_by_field_name("flag", &mut c)
                                .map(|f| (ctx.raw(&f).to_owned(), ctx.span(&f)))
                                .collect()
                        })
                        .unwrap_or_default();
                    items.push(Item::Step(Box::new(Step { link, text, flags })));
                }
                "loop" => {
                    let label = child.child_by_field_name("label").map(|l| unescape(ctx.raw(&l))).unwrap_or_default();
                    let nested = self.lower_process_items(ctx, child);
                    items.push(Item::Loop { label, span: ctx.span(&child), items: nested });
                }
                _ => {}
            }
        }
        items
    }

    /// `for_each`: its body once per item, the variables bound to the item (or to the values of
    /// the item, a list, when there are several variables).
    fn lower_for_each(&mut self, ctx: &Ctx, node: Node, parent: Option<ResourceId>) {
        let mut cursor = node.walk();
        let names: Vec<String> =
            node.children_by_field_name("variable", &mut cursor).map(|n| ctx.raw(&n).to_owned()).collect();
        let Some(items_node) = node.child_by_field_name("items") else { return };
        let items = if items_node.kind() == "csv" {
            match self.csv_items(ctx, items_node, &node, &names) {
                Some(items) => items,
                None => return,
            }
        } else {
            let Some(items) = lower_value(ctx, items_node) else { return };
            let ValueKind::List(items) = items.kind else { return };
            items
        };
        for item in items {
            let values: Vec<Value> = if names.len() == 1 {
                vec![item]
            } else {
                match item.kind {
                    ValueKind::List(values) if values.len() == names.len() => values,
                    _ => {
                        self.diagnostics.push(Diagnostic::error(
                            format!("for_each expects lists of {} values: {}", names.len(), item.raw),
                            item.span,
                        ));
                        continue;
                    }
                }
            };
            let mut variables = ctx.variables.clone();
            variables.extend(names.iter().cloned().zip(values));
            let inner = Ctx { file: ctx.file, source: ctx.source, variables };
            let mut c = node.walk();
            for element in node.named_children(&mut c) {
                match (element.kind(), parent) {
                    ("resource", _) => self.lower_resource(&inner, element, parent),
                    ("link", _) => {
                        let link = self.lower_link(&inner, element, None);
                        self.registry.links.push(link);
                    }
                    ("inline_link", Some(_)) => {
                        let link = self.lower_link(&inner, element, parent);
                        self.registry.links.push(link);
                    }
                    ("inline_link", None) => self.diagnostics.push(
                        Diagnostic::error("a link without its source must be inside a resource", inner.span(&element))
                            .with_help("write `source -> target` at the root level"),
                    ),
                    ("for_each", _) => self.lower_for_each(&inner, element, parent),
                    _ => {}
                }
            }
        }
    }

    /// The rows of the CSV file of a `for_each`, each a list of the values of its variables, taken
    /// from the columns of the same name; `None` when the file cannot be read (reported).
    fn csv_items(&mut self, ctx: &Ctx, csv: Node, for_each: &Node, names: &[String]) -> Option<Vec<Value>> {
        let path_node = csv.child_by_field_name("path")?;
        let written = unescape(ctx.raw(&path_node));
        let here = self.files.get(ctx.file.0 as usize).map_or("", |f| f.path.as_str());
        let path = crate::source::join(here, &written);
        let Some((index, file)) = self.files.iter().enumerate().find(|(_, f)| crate::source::join("", &f.path) == path)
        else {
            self.diagnostics.push(
                Diagnostic::error(format!("CSV file not found: {written}"), ctx.span(&path_node))
                    .with_help("a path relative to this file, of a .csv file of the project"),
            );
            return None;
        };
        let file_id = FileId(index as u32);
        let span = |start: usize, end: usize| crate::source::Span::at(file_id, &file.text, start, end);
        let records = match crate::csv::parse(&file.text) {
            Ok(records) => records,
            Err((at, message)) => {
                self.diagnostics.push(Diagnostic::error(format!("{written}: {message}"), span(at, at)));
                return None;
            }
        };
        let Some((header, rows)) = records.split_first() else { return Some(Vec::new()) };
        // The column of each variable.
        let mut cursor = for_each.walk();
        let variables: Vec<Node> = for_each.children_by_field_name("variable", &mut cursor).collect();
        let mut columns = Vec::new();
        for (name, variable) in names.iter().zip(&variables) {
            match header.iter().position(|h| h.text.trim() == name) {
                Some(column) => columns.push(column),
                None => {
                    let available: Vec<&str> = header.iter().map(|h| h.text.trim()).collect();
                    self.diagnostics.push(
                        Diagnostic::error(format!("{written} has no column {name}"), ctx.span(variable))
                            .with_help(format!("its columns: {}", available.join(", "))),
                    );
                    return None;
                }
            }
        }
        let items = rows
            .iter()
            .map(|row| {
                let values: Vec<Value> = columns
                    .iter()
                    .map(|&column| match row.get(column) {
                        Some(field) => cell_value(file_id, &file.text, field),
                        None => Value::absent(span(row[0].start, row[row.len() - 1].end)),
                    })
                    .collect();
                let whole = span(row[0].start, row[row.len() - 1].end);
                match values.len() {
                    1 => values.into_iter().next().unwrap(),
                    _ => Value { raw: String::new(), kind: ValueKind::List(values), span: whole },
                }
            })
            .collect();
        Some(items)
    }

    fn lower_link(&mut self, ctx: &Ctx, node: Node, enclosing: Option<ResourceId>) -> Link {
        let missing = || Reference { segments: Vec::new(), span: ctx.span(&node) };
        let source = match enclosing {
            Some(id) => LinkSource::Enclosing(id),
            None => LinkSource::Named(
                node.child_by_field_name("source").map(|n| reference(ctx, &n)).unwrap_or_else(missing),
            ),
        };
        let target = node.child_by_field_name("target").map(|n| reference(ctx, &n)).unwrap_or_else(missing);

        let port = node.child_by_field_name("port").map(|p| PortSpec {
            protocol: p.child_by_field_name("protocol").map(|n| ctx.text(&n).to_owned()).unwrap_or_default(),
            port: p.child_by_field_name("port").map(|n| ctx.text(&n).to_owned()),
            span: ctx.span(&p),
        });

        let layers = node
            .child_by_field_name("layers")
            .map(|l| {
                let mut cursor = l.walk();
                l.named_children(&mut cursor)
                    .filter(|n| n.kind() == "identifier")
                    .map(|n| (ctx.text(&n).to_owned(), ctx.span(&n)))
                    .collect()
            })
            .unwrap_or_default();

        let attributes = node
            .child_by_field_name("body")
            .map(|b| {
                let mut cursor = b.walk();
                b.named_children(&mut cursor)
                    .filter(|n| n.kind() == "attribute")
                    .filter_map(|n| lower_attribute(ctx, n))
                    .collect()
            })
            .unwrap_or_default();

        Link { source, target, port, layers, attributes, span: ctx.span(&node) }
    }
}

struct Ctx<'s> {
    file: FileId,
    source: &'s str,
    /// The variables of the enclosing `for_each` blocks, the innermost last.
    variables: Vec<(String, Value)>,
}

impl Ctx<'_> {
    /// The text of a node as written.
    fn raw(&self, node: &Node) -> &str {
        &self.source[node.byte_range()]
    }

    /// The text of a node, its `$variables` replaced: a string item by its content, any other
    /// item as written.
    fn text(&self, node: &Node) -> String {
        let raw = self.raw(node);
        if self.variables.is_empty() || !raw.contains('$') {
            return raw.to_owned();
        }
        let mut out = String::with_capacity(raw.len());
        let mut rest = raw;
        while let Some(at) = rest.find('$') {
            out.push_str(&rest[..at]);
            let after = &rest[at + 1..];
            // The longest variable whose name follows, not followed by a letter, a digit or `_`.
            let found = self
                .variables
                .iter()
                .rev()
                .filter(|(name, _)| {
                    after.starts_with(name.as_str())
                        && !after[name.len()..].starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_')
                })
                .max_by_key(|(name, _)| name.len());
            match found {
                Some((name, value)) => {
                    out.push_str(&match &value.kind {
                        ValueKind::String(text) => text.clone(),
                        _ => value.raw.clone(),
                    });
                    rest = &after[name.len()..];
                }
                None => {
                    out.push('$');
                    rest = after;
                }
            }
        }
        out.push_str(rest);
        out
    }

    /// The value of a variable written alone: `ip: $ip`.
    fn variable(&self, node: &Node) -> Option<&Value> {
        let name = self.raw(node).strip_prefix('$')?;
        self.variables.iter().rev().find(|(n, _)| n == name).map(|(_, v)| v)
    }

    fn span(&self, node: &Node) -> Span {
        Span::of(self.file, node)
    }
}

fn reference(ctx: &Ctx, node: &Node) -> Reference {
    Reference { segments: ctx.text(node).split('.').map(str::to_owned).collect(), span: ctx.span(node) }
}

/// Every name of a file still holding a `$`: a variable used outside its `for_each`.
pub(crate) fn unbound_variables(registry: &Registry, from: usize, links_from: usize) -> Vec<Diagnostic> {
    let mut found = Vec::new();
    let mut check = |text: &str, span: Span| {
        if let Some(at) = text.find('$') {
            let name: String =
                text[at + 1..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-').collect();
            found.push(
                Diagnostic::error(format!("unknown variable: ${name}"), span)
                    .with_help("a variable is declared by a `for_each` enclosing it"),
            );
        }
    };
    fn names<'v>(value: &'v Value, out: &mut Vec<&'v Value>) {
        match &value.kind {
            ValueKind::List(items) => items.iter().for_each(|i| names(i, out)),
            ValueKind::Identifier | ValueKind::QualifiedName => out.push(value),
            _ => {}
        }
    }
    for resource in &registry.resources[from..] {
        check(&resource.name, resource.name_span);
        for attribute in &resource.attributes {
            let mut values = Vec::new();
            names(&attribute.value, &mut values);
            values.into_iter().for_each(|v| check(&v.raw, v.span));
        }
    }
    for link in &registry.links[links_from..] {
        check(&link.target.segments.join("."), link.target.span);
        if let LinkSource::Named(source) = &link.source {
            check(&source.segments.join("."), source.span);
        }
        for attribute in &link.attributes {
            let mut values = Vec::new();
            names(&attribute.value, &mut values);
            values.into_iter().for_each(|v| check(&v.raw, v.span));
        }
    }
    found
}

/// Lowers a value node (`identifier`, `list`, `string`…) of any file.
pub(crate) fn value_at(file: FileId, source: &str, node: Node) -> Option<Value> {
    lower_value(&Ctx { file, source, variables: Vec::new() }, node)
}

/// Lowers an `attribute` node of any file.
pub(crate) fn attribute_at(file: FileId, source: &str, node: Node) -> Option<Attribute> {
    lower_attribute(&Ctx { file, source, variables: Vec::new() }, node)
}

/// A cell of a CSV file, read as a value of the language when it is one token (`10.1.1.10` an
/// address, `16` a number), a string otherwise; an empty cell is absent.
fn cell_value(file: FileId, csv: &str, field: &crate::csv::Field) -> Value {
    let span = crate::source::Span::at(file, csv, field.start, field.end);
    let text = field.text.trim();
    if text.is_empty() {
        return Value::absent(span);
    }
    let snippet = format!("x y {{ v: {text} }}");
    let tree = crate::syntax::parse(&snippet);
    let value = (!tree.root_node().has_error())
        .then(|| {
            let attribute = tree.root_node().named_child(0)?.child_by_field_name("body")?.named_child(0)?;
            let value = attribute.child_by_field_name("value")?;
            (value.byte_range() == (9..9 + text.len())).then_some(value)
        })
        .flatten()
        .and_then(|node| {
            let ctx = Ctx { file, source: &snippet, variables: Vec::new() };
            lower_value(&ctx, node)
        });
    match value {
        Some(value) => value.with_span(span),
        None => Value { raw: text.to_owned(), kind: ValueKind::String(text.to_owned()), span },
    }
}

fn lower_attribute(ctx: &Ctx, node: Node) -> Option<Attribute> {
    let key = node.child_by_field_name("key")?;
    let value = node.child_by_field_name("value")?;
    Some(Attribute { key: ctx.text(&key).to_owned(), key_span: ctx.span(&key), value: lower_value(ctx, value)? })
}

fn lower_value(ctx: &Ctx, node: Node) -> Option<Value> {
    // A variable alone takes the value of the item, its kind included: `ip: $ip` is an address.
    if node.kind() == "identifier"
        && let Some(value) = ctx.variable(&node)
    {
        // An empty cell of a CSV file: the key is not written.
        if value.is_absent() {
            return None;
        }
        // A cell of a CSV file keeps its place there: a wrong value is reported on its row.
        if value.span.file != ctx.file {
            return Some(value.clone());
        }
        return Some(Value { span: ctx.span(&node), ..value.clone() });
    }
    let kind = match node.kind() {
        "identifier" => ValueKind::Identifier,
        "qualified_name" => ValueKind::QualifiedName,
        "number" => ValueKind::Number,
        "quantity" => ValueKind::Quantity,
        "semver" => ValueKind::Semver,
        "ipv4" => ValueKind::Ipv4,
        "cidr4" => ValueKind::Cidr4,
        "ipv6" => ValueKind::Ipv6,
        "cidr6" => ValueKind::Cidr6,
        "string" => ValueKind::String(unescape(&ctx.text(&node))),
        "list" => {
            let mut cursor = node.walk();
            ValueKind::List(
                node.named_children(&mut cursor)
                    .filter(|n| n.kind() != "comment")
                    .filter_map(|n| lower_value(ctx, n))
                    .collect(),
            )
        }
        _ => return None,
    };
    Some(Value { kind, raw: ctx.text(&node), span: ctx.span(&node) })
}

/// Removes the surrounding quotes and resolves the escapes (backslash followed by
/// a quote, a backslash, `n` or `t`).
fn unescape(raw: &str) -> String {
    let inner = raw.strip_prefix('"').and_then(|s| s.strip_suffix('"')).unwrap_or(raw);
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unescape_resolves_escapes() {
        assert_eq!(unescape(r#""a \"b\" \\ c""#), r#"a "b" \ c"#);
        assert_eq!(unescape(r#""line\nnext""#), "line\nnext");
    }
}
