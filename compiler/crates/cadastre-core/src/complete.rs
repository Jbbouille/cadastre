//! Completion: what may be written at a position of a `.ca`, `.cav` or `.cac` file.
//!
//! The text before the position is read with a light lexer rather than the syntax tree: the
//! line being typed is almost always incomplete, hence invalid. The lexer tracks the blocks
//! opened before the position (a resource, a link, a view, a style rule…) and the tokens of the
//! statement being written; resource names come from the last compilation of the project.

use std::collections::BTreeSet;

use crate::project::Compilation;
use crate::registry::ResourceId;
use crate::resolve::Resolver;
use crate::schema::{self, AttrDef, Ty};
use crate::source::FileRole;
use crate::vocabulary::{Styled, Values};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemKind {
    /// A resource kind (`server`) or a group of kinds (`flow_endpoint`).
    Kind,
    /// An attribute key, a render option, a style property.
    Key,
    /// A value from a closed list (`on-prem`), a format, a protocol, a layer.
    Value,
    /// A resource name, or the host name of a virtual IP.
    Resource,
    Keyword,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub label: String,
    pub kind: ItemKind,
    /// What the item is: `server dc.lan.dmz.web01`, `ref(server | cluster)`.
    pub detail: String,
    /// Text to insert, when it differs from the label (`os: ` for the key `os`).
    pub insert: Option<String>,
}

/// The completions at a byte offset of a file of the project.
pub fn complete(compilation: &Compilation, role: FileRole, text: &str, offset: usize) -> Vec<Item> {
    let before = &text[..floor_boundary(text, offset)];
    let context = Context::read(before, role);
    let mut items = Completer { compilation, items: Vec::new() }.at(&context);
    // The word being typed filters the items, by the start of the label or of one of its
    // segments (`pr` for `on-prem`, `web` for `dmz.web01`); editors filter too.
    if !context.word.is_empty() {
        let word = context.word.to_ascii_lowercase();
        items.retain(|item| {
            let label = item.label.to_ascii_lowercase();
            label.starts_with(&word) || label.split(['.', '-', '_']).any(|segment| segment.starts_with(&word))
        });
    }
    let mut seen = BTreeSet::new();
    items.retain(|item| seen.insert(item.label.clone()));
    items
}

fn floor_boundary(text: &str, mut offset: usize) -> usize {
    offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

// ---------------------------------------------------------------------------
// Context
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    Word(String),
    Arrow,
    Punct(char),
    Text,
}

/// A block opened before the position.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Block {
    Root,
    Resource(String),
    Link,
    View,
    Check,
    Render,
    Style,
    /// The properties of a style rule: of nodes, or of edges.
    StyleRule {
        edges: bool,
    },
    Other,
}

struct Context {
    role: FileRole,
    block: Block,
    /// Tokens of the statement being written, before the word being typed.
    statement: Vec<Token>,
    /// The word being typed, possibly empty.
    word: String,
}

impl Context {
    fn read(before: &str, role: FileRole) -> Self {
        let (tokens, word) = lex(before);
        let mut blocks = vec![Block::Root];
        let mut statement: Vec<Token> = Vec::new();
        let mut nesting = 0usize;
        for token in tokens {
            match &token {
                Token::Punct('{') => {
                    let parent = blocks.last().cloned().unwrap_or(Block::Root);
                    blocks.push(open(&parent, &statement, role));
                    statement.clear();
                    nesting = 0;
                }
                Token::Punct('}') => {
                    if blocks.len() > 1 {
                        blocks.pop();
                    }
                    statement.clear();
                    nesting = 0;
                }
                Token::Punct('[' | '(') => {
                    nesting += 1;
                    statement.push(token);
                }
                Token::Punct(']' | ')') => {
                    nesting = nesting.saturating_sub(1);
                    statement.push(token);
                }
                // A comma or a line break ends a statement, outside brackets.
                Token::Punct(',' | '\n') if nesting == 0 => statement.clear(),
                Token::Punct('\n') => {}
                _ => statement.push(token),
            }
        }
        Self { role, block: blocks.pop().unwrap_or(Block::Root), statement, word }
    }

    fn words(&self) -> Vec<&str> {
        self.statement
            .iter()
            .filter_map(|t| match t {
                Token::Word(w) => Some(w.as_str()),
                _ => None,
            })
            .collect()
    }

    fn first_word(&self) -> Option<&str> {
        match self.statement.first() {
            Some(Token::Word(w)) => Some(w),
            _ => None,
        }
    }

    /// The key of a `key: value` statement, when the position is in its value.
    fn value_of(&self) -> Option<&str> {
        match self.statement.as_slice() {
            [Token::Word(key), Token::Punct(':'), ..] => Some(key),
            _ => None,
        }
    }

    /// Whether the position is inside `[…]` of the statement.
    fn in_brackets(&self) -> bool {
        let opened = self.statement.iter().filter(|t| **t == Token::Punct('[')).count();
        let closed = self.statement.iter().filter(|t| **t == Token::Punct(']')).count();
        opened > closed
    }
}

/// The block a `{` opens, from the statement before it.
fn open(parent: &Block, statement: &[Token], role: FileRole) -> Block {
    let words: Vec<&str> = statement
        .iter()
        .filter_map(|t| match t {
            Token::Word(w) => Some(w.as_str()),
            _ => None,
        })
        .collect();
    let arrow = statement.contains(&Token::Arrow);
    match parent {
        Block::Root | Block::Resource(_) if role == FileRole::Registry => match (arrow, words.as_slice()) {
            (true, _) => Block::Link,
            (false, [kind, _name]) => Block::Resource((*kind).to_owned()),
            _ => Block::Other,
        },
        Block::Root | Block::View => match words.first() {
            Some(&"view") => Block::View,
            Some(&"check") => Block::Check,
            Some(&"style") => Block::Style,
            Some(&"render") => Block::Render,
            _ => Block::Other,
        },
        Block::Style => match words.first() {
            Some(&("layer" | "edges")) => Block::StyleRule { edges: true },
            Some(&("kind" | "nodes")) => Block::StyleRule { edges: false },
            _ => Block::Other,
        },
        _ => Block::Other,
    }
}

/// Tokens of the text, and the word it ends with (being typed).
fn lex(text: &str) -> (Vec<Token>, String) {
    let chars: Vec<char> = text.chars().collect();
    let word_char = |c: char| c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | '?' | '*');
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\n' => {
                tokens.push(Token::Punct('\n'));
                i += 1;
            }
            c if c.is_whitespace() => i += 1,
            '/' if chars.get(i + 1) == Some(&'/') => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '/' if chars.get(i + 1) == Some(&'*') => {
                i += 2;
                while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                    i += 1;
                }
                i += 2;
            }
            '"' => {
                i += 1;
                while i < chars.len() && chars[i] != '"' && chars[i] != '\n' {
                    i += if chars[i] == '\\' { 2 } else { 1 };
                }
                i += 1;
                tokens.push(Token::Text);
            }
            '-' if chars.get(i + 1) == Some(&'>') => {
                tokens.push(Token::Arrow);
                i += 2;
            }
            c if word_char(c) => {
                let start = i;
                while i < chars.len() && word_char(chars[i]) && !(chars[i] == '-' && chars.get(i + 1) == Some(&'>')) {
                    i += 1;
                }
                tokens.push(Token::Word(chars[start..i].iter().collect()));
            }
            c => {
                tokens.push(Token::Punct(c));
                i += 1;
            }
        }
    }
    // The text ends with a word being typed: it is not part of the statement.
    let typing = text.chars().last().is_some_and(word_char);
    let word = match tokens.last() {
        Some(Token::Word(w)) if typing => w.clone(),
        _ => String::new(),
    };
    if !word.is_empty() {
        tokens.pop();
    }
    (tokens, word)
}

// ---------------------------------------------------------------------------
// Items
// ---------------------------------------------------------------------------

struct Completer<'c> {
    compilation: &'c Compilation,
    items: Vec<Item>,
}

impl Completer<'_> {
    fn at(mut self, context: &Context) -> Vec<Item> {
        match (&context.block, context.role) {
            (Block::Root, FileRole::Registry) => self.registry_statement(None, context),
            (Block::Resource(kind), _) => self.registry_statement(Some(kind), context),
            (Block::Link, _) => match context.value_of() {
                Some(key) => {
                    if let Some(def) = schema::link_attr(key) {
                        self.values(def);
                    }
                }
                None => self.keys(schema::LINK_KEYS.iter()),
            },
            (Block::Root, FileRole::Plans) => {
                if context.statement.is_empty() {
                    self.keywords(&["view", "style"]);
                }
            }
            (Block::Root, FileRole::Checks) => {
                if context.statement.is_empty() {
                    self.keywords(&["check"]);
                }
            }
            (Block::View, _) => self.view_statement(context),
            (Block::Check, _) => self.check_statement(context),
            (Block::Render, _) => self.render_option(context),
            (Block::Style, _) => self.style_target(context),
            (Block::StyleRule { edges }, _) => self.style_property(*edges, context),
            (Block::Other, _) => {}
        }
        self.items
    }

    /// A statement of the registry, at the root (`kind` is `None`) or in a resource block.
    fn registry_statement(&mut self, kind: Option<&String>, context: &Context) {
        let def = kind.and_then(|k| self.compilation.registry.schema.kind_def(k));
        // `key: value`.
        if let (Some(key), Some(def)) = (context.value_of(), def) {
            // `parent:`: the precise parents of the kind (an approximate one makes it incomplete).
            if key == "parent" {
                let parents: Vec<&str> = def.parents.iter().copied().filter(|&p| p != "none").collect();
                self.resources(&parents);
            } else if let Some(attr) = self.compilation.registry.schema.resource_attr(def, key) {
                self.values(attr);
            }
            return;
        }
        // A link: `-> target : protocol/port [layers] { … }`.
        if let Some(arrow) = context.statement.iter().position(|t| *t == Token::Arrow) {
            let after = &context.statement[arrow + 1..];
            if context.in_brackets() {
                self.layers();
            } else if after.is_empty() {
                self.resources(&self.compilation.registry.schema.expand("flow_endpoint"));
                self.hosts();
            } else if after.contains(&Token::Punct(':')) {
                self.protocols();
            }
            return;
        }
        // After `virtual`: the kinds of machine.
        if context.statement == [Token::Word("virtual".to_owned())] {
            for k in schema::MACHINES {
                self.push(k, ItemKind::Kind, "resource kind", None);
            }
            return;
        }
        // The start of a statement: child kinds, then keys of the enclosing resource; at the root,
        // the kinds placed at the root, then the others; `virtual` where a machine may go.
        if context.statement.is_empty() {
            let parent = kind.map_or("none", String::as_str);
            for k in schema::KINDS {
                if k.parents.contains(&parent) {
                    self.push(k.name, ItemKind::Kind, "resource kind", None);
                }
            }
            if schema::KINDS.iter().any(|k| schema::MACHINES.contains(&k.name) && k.parents.contains(&parent)) {
                self.keywords(&["virtual"]);
            }
            if let Some(def) = def {
                self.keys(def.attrs.iter().chain(schema::RESOURCE_KEYS));
            } else {
                // Not the kinds within a resource: proposed in their resource.
                for k in schema::KINDS.iter().filter(|k| !k.parents.contains(&"none") && !k.within) {
                    self.push(k.name, ItemKind::Kind, "resource kind", None);
                }
            }
        }
    }

    fn view_statement(&mut self, context: &Context) {
        let words = context.words();
        match context.first_word() {
            None => {
                self.keywords(&["select", "zone", "edges", "style", "render"]);
                for option in crate::vocabulary::VIEW_ATTRIBUTES {
                    self.push(option.key, ItemKind::Key, option.doc, Some(format!("{}: ", option.key)));
                }
            }
            Some("select" | "group-by") if words.contains(&"group-by") || context.first_word() == Some("group-by") => {
                self.kinds_and_groups();
                for key in crate::view::reference_keys() {
                    self.push(&key, ItemKind::Key, "reference key", None);
                }
            }
            Some("select") if !words.contains(&"where") => {
                self.kinds_and_groups();
                self.keywords(&["where", "group-by", "with", "without"]);
            }
            Some("select") => self.keywords(&["and", "or", "not", "has", "in", "exists", "group-by"]),
            Some("edges") if context.in_brackets() => self.layers(),
            Some("edges") => {
                self.keywords(&["lift", "follow", "process", "path", "where"]);
                for key in crate::view::reference_keys().iter().map(String::as_str).chain(["parent"]) {
                    self.push(key, ItemKind::Key, "relation", None);
                }
                for k in self.compilation.registry.schema.expand("connector") {
                    self.push(k, ItemKind::Kind, "connector", None);
                }
            }
            Some("render") => {
                for (format, doc) in crate::vocabulary::FORMATS {
                    self.push(format, ItemKind::Value, doc, None);
                }
            }
            _ => {}
        }
    }

    /// A statement of a check: `select` and `edges` as in a view, `expect`, its attributes.
    fn check_statement(&mut self, context: &Context) {
        if let Some(key) = context.value_of() {
            if let Some(option) = crate::vocabulary::find(crate::vocabulary::CHECK_ATTRIBUTES, key)
                && let Values::Closed(values) = option.values
            {
                self.symbols(values);
            }
            return;
        }
        let words = context.words();
        match context.first_word() {
            None => {
                self.keywords(&["select", "edges", "expect"]);
                for option in crate::vocabulary::CHECK_ATTRIBUTES {
                    self.push(option.key, ItemKind::Key, option.doc, Some(format!("{}: ", option.key)));
                }
            }
            Some("expect") if words.len() == 1 => {
                for (expectation, doc) in crate::vocabulary::EXPECTATIONS {
                    let word = expectation.split(' ').next().unwrap_or(expectation);
                    self.push(word, ItemKind::Keyword, doc, None);
                }
            }
            Some("expect") if words.get(1) == Some(&"all") => {
                self.keywords(&["and", "or", "not", "has", "in", "exists"]);
            }
            Some("expect") => {}
            _ => self.view_statement(context),
        }
    }

    fn render_option(&mut self, context: &Context) {
        let Some(key) = context.value_of() else {
            for option in crate::vocabulary::RENDER_OPTIONS {
                self.push(option.key, ItemKind::Key, option.doc, Some(format!("{}: ", option.key)));
            }
            return;
        };
        let Some(option) = crate::vocabulary::find(crate::vocabulary::RENDER_OPTIONS, key) else { return };
        match option.values {
            Values::Closed(values) => self.symbols(values),
            Values::EdgeLabels => {
                for (label, doc) in crate::vocabulary::EDGE_LABELS {
                    self.push(label, ItemKind::Value, doc, None);
                }
                let keys: BTreeSet<&str> = self
                    .compilation
                    .registry
                    .links
                    .iter()
                    .flat_map(|l| l.attributes.iter().map(|a| a.key.as_str()))
                    .collect();
                for key in keys {
                    self.push(key, ItemKind::Key, "link key", None);
                }
            }
            Values::Keys => {
                let keys: BTreeSet<&str> = schema::RESOURCE_KEYS
                    .iter()
                    .map(|a| a.key)
                    .chain(
                        self.compilation
                            .registry
                            .resources
                            .iter()
                            .flat_map(|r| r.attributes.iter().map(|a| a.key.as_str())),
                    )
                    .collect();
                for key in keys {
                    self.push(key, ItemKind::Key, "resource key", None);
                }
            }
            Values::Resources | Values::Levels => self.resources(&[]),
            Values::Wrap => self.symbols(&["values", "40"]),
            Values::Number => self.symbols(&["10"]),
            Values::Numbers => self.symbols(&["1.5", "[1, 2]"]),
            Values::MergeLabel => self.symbols(&["list", "count", "5"]),
            Values::Columns => self.symbols(&["name", "kind", "label", "source", "target", "port", "carries"]),
            _ => {}
        }
    }

    fn style_target(&mut self, context: &Context) {
        match context.first_word() {
            None => self.keywords(&["kind", "layer", "nodes", "edges"]),
            Some("kind") if context.statement.len() == 1 => self.kinds_and_groups(),
            Some("layer") if context.statement.len() == 1 => self.layers(),
            Some(_) => self.keywords(&["where"]),
        }
    }

    fn style_property(&mut self, edges: bool, context: &Context) {
        let styled = if edges { Styled::Edges } else { Styled::Nodes };
        let properties = crate::vocabulary::STYLE_PROPERTIES.iter().filter(|(_, s)| *s == styled || *s == Styled::Both);
        let Some(key) = context.value_of() else {
            for (property, _) in properties {
                self.push(property.key, ItemKind::Key, property.doc, Some(format!("{}: ", property.key)));
            }
            return;
        };
        if let Some((property, _)) = crate::vocabulary::STYLE_PROPERTIES.iter().find(|(p, _)| p.key == key)
            && let Values::Closed(values) = property.values
        {
            self.symbols(values);
        }
    }

    // -- Building blocks ---------------------------------------------------

    fn push(&mut self, label: &str, kind: ItemKind, detail: &str, insert: Option<String>) {
        self.items.push(Item { label: label.to_owned(), kind, detail: detail.to_owned(), insert });
    }

    fn keywords(&mut self, words: &[&str]) {
        for word in words {
            self.push(word, ItemKind::Keyword, "keyword", None);
        }
    }

    fn symbols(&mut self, values: &[&str]) {
        for value in values {
            self.push(value, ItemKind::Value, "value", None);
        }
    }

    fn keys<'a>(&mut self, attrs: impl Iterator<Item = &'a AttrDef>) {
        for attr in attrs {
            let detail = match attr.ty {
                Ty::Ref(union) => format!("ref({})", union.replace('|', " | ")),
                Ty::Symbol(values) => values.join(" | "),
                Ty::String | Ty::Strings => "string".to_owned(),
                Ty::Number => "number".to_owned(),
                Ty::Semver => "semver".to_owned(),
                Ty::Version => "version".to_owned(),
                Ty::Ip => "ip".to_owned(),
                Ty::Cidr => "cidr".to_owned(),
                Ty::Host => "host name".to_owned(),
                Ty::Url => "url".to_owned(),
                Ty::Bandwidth => "bandwidth".to_owned(),
                Ty::Spdx => "SPDX license expression".to_owned(),
                Ty::Pattern(pattern) => format!("pattern(\"{}\")", pattern.source),
            };
            let detail = if attr.required { format!("{detail} (required)") } else { detail };
            self.push(attr.key, ItemKind::Key, &detail, Some(format!("{}: ", attr.key)));
        }
    }

    /// The values of an attribute: its closed list, or the resources it may reference.
    fn values(&mut self, attr: &AttrDef) {
        match attr.ty {
            Ty::Symbol(values) => self.symbols(values),
            Ty::Ref(union) => self.resources(&self.compilation.registry.schema.expand(union)),
            _ => {}
        }
    }

    fn kinds_and_groups(&mut self) {
        let schema = &self.compilation.registry.schema;
        for k in schema.kind_names() {
            let detail = match schema.kind_def(k) {
                Some(def) if def.within => format!("resource kind, within {}", def.parents.join(" | ")),
                _ => "resource kind".to_owned(),
            };
            self.push(k, ItemKind::Kind, &detail, None);
        }
        for (group, members) in self.compilation.registry.schema.groups() {
            self.push(group, ItemKind::Kind, &members.join(" | "), None);
        }
    }

    /// The resources of the expected kinds (all of them when `expected` is empty), each by its
    /// shortest unambiguous name.
    fn resources(&mut self, expected: &[&str]) {
        let registry = &self.compilation.registry;
        let resolver = Resolver::new(registry);
        for id in registry.ids() {
            let resource = registry.resource(id);
            if !expected.is_empty() && !expected.contains(&resource.kind.as_str()) {
                continue;
            }
            let homonyms: Vec<ResourceId> =
                registry.ids().filter(|&o| registry.resource(o).name == resource.name).collect();
            let label = resolver.minimal_suffix(id, &homonyms);
            let detail = format!("{} {}", resource.kind, registry.qualified_name(id));
            self.push(&label, ItemKind::Resource, &detail, None);
        }
    }

    /// The host names of the virtual IPs, valid flow targets.
    fn hosts(&mut self) {
        let registry = &self.compilation.registry;
        for id in registry.ids().filter(|&id| registry.resource(id).kind == "vip") {
            let resource = registry.resource(id);
            for host in resource
                .attribute("host")
                .map(|a| a.value.items())
                .unwrap_or_default()
                .iter()
                .filter_map(|v| v.as_host())
            {
                let detail = format!("vip {}", registry.qualified_name(id));
                self.push(&host, ItemKind::Resource, &detail, None);
            }
        }
    }

    /// Layers used in the project, and the standard ones.
    fn layers(&mut self) {
        let mut layers: BTreeSet<&str> = ["flow", "auth", "trigger"].into_iter().collect();
        layers.extend(self.compilation.registry.links.iter().flat_map(|l| l.layers.iter().map(|(n, _)| n.as_str())));
        for layer in layers {
            self.push(layer, ItemKind::Value, "layer", None);
        }
    }

    /// Protocols used in the project.
    fn protocols(&mut self) {
        let protocols: BTreeSet<&str> = self
            .compilation
            .registry
            .links
            .iter()
            .filter_map(|l| l.port.as_ref().map(|p| p.protocol.as_str()))
            .collect();
        for protocol in protocols {
            self.push(protocol, ItemKind::Value, "protocol", None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile;
    use crate::source::SourceFile;

    const REGISTRY: &str = "site dc {\n  hosting: on-prem\n  network lan {\n    subnet dmz { cidr: 10.0.1.0/24, server web01 { } }\n    subnet erp { cidr: 10.0.2.0/24, server web01 { } }\n  }\n}\ndata customer\napp crm { on: dmz.web01, -> billing : https/443 [flow] }\napp billing { on: erp.web01 }\n";

    /// Completions where `|` is written in `text`, a file of the project above.
    fn at(path: &str, text: &str) -> Vec<Item> {
        let offset = text.find('|').unwrap();
        let text = text.replace('|', "");
        let c = compile(&[SourceFile { path: "a.ca".into(), text: REGISTRY.into() }]);
        let role = FileRole::from_path(path).unwrap();
        complete(&c, role, &text, offset)
    }

    fn labels(items: &[Item]) -> Vec<&str> {
        items.iter().map(|i| i.label.as_str()).collect()
    }

    #[test]
    fn kinds_and_keys_in_a_resource() {
        let items = at("b.ca", "site s {\n  |\n}");
        let names = labels(&items);
        assert!(names.contains(&"vpc") && names.contains(&"network") && names.contains(&"rack"), "{names:?}");
        assert!(!names.contains(&"server"), "{names:?}"); // not a precise child of a site
        let hosting = items.iter().find(|i| i.label == "hosting").unwrap();
        assert_eq!(hosting.insert.as_deref(), Some("hosting: "));
        assert!(hosting.detail.contains("on-prem") && hosting.detail.ends_with("(required)"), "{hosting:?}");
        // At the root: kinds placed at the root first.
        let root = at("b.ca", "|");
        assert_eq!(root.first().map(|i| i.kind), Some(ItemKind::Kind));
        assert!(labels(&root).contains(&"site") && labels(&root).contains(&"app"));
    }

    #[test]
    fn values_of_keys() {
        assert_eq!(labels(&at("b.ca", "site s { hosting: |")), ["on-prem", "colocation", "cloud", "office"]);
        assert_eq!(labels(&at("b.ca", "site s { hosting: c|")), ["colocation", "cloud"]);
        // References: the resources of the expected kinds, by their shortest unambiguous name.
        let on = at("b.ca", "app x {\n  on: |\n}");
        assert!(labels(&on).contains(&"dmz.web01") && labels(&on).contains(&"erp.web01"), "{on:?}");
        assert!(!labels(&on).contains(&"customer"), "{on:?}");
        let web = on.iter().find(|i| i.label == "dmz.web01").unwrap();
        assert_eq!(web.detail, "server dc.lan.dmz.web01");
        // In a list.
        assert_eq!(labels(&at("b.ca", "app x { -> crm [flow] { carries: [|")), ["customer"]);
    }

    #[test]
    fn links() {
        let targets = at("b.ca", "app x {\n  -> |\n}");
        assert!(labels(&targets).contains(&"crm") && labels(&targets).contains(&"dmz.web01"), "{targets:?}");
        assert!(!labels(&targets).contains(&"customer") && !labels(&targets).contains(&"dc"), "{targets:?}");
        assert_eq!(labels(&at("b.ca", "app x { -> crm : |")), ["https"]);
        assert_eq!(labels(&at("b.ca", "app x { -> crm : https/443 [|")), ["auth", "flow", "trigger"]);
        let keys = at("b.ca", "app x { -> crm [flow] { |");
        assert!(labels(&keys).contains(&"carries") && labels(&keys).contains(&"exchange"), "{keys:?}");
        assert_eq!(labels(&at("b.ca", "app x { -> crm [flow] { exchange: |")), ["push", "pull", "both"]);
        // `parent:`: the subnets for a server, with the shortest unambiguous names.
        assert_eq!(labels(&at("b.ca", "server x { parent: |")), ["dmz", "erp"]);
    }

    #[test]
    fn plans() {
        assert_eq!(labels(&at("v.cav", "|")), ["view", "style"]);
        assert_eq!(labels(&at("c.cac", "|")), ["check"]);
        assert_eq!(
            labels(&at("v.cav", "view v {\n  |")),
            ["select", "zone", "edges", "style", "render", "title", "description", "failing"]
        );
        assert_eq!(
            labels(&at("c.cac", "check c {\n  |")),
            ["select", "edges", "expect", "message", "severity", "failing"]
        );
        assert_eq!(labels(&at("c.cac", "check c {\n  expect |")), ["none", "some", "count", "all"]);
        assert_eq!(labels(&at("c.cac", "check c {\n  severity: |")), ["error", "warning", "info"]);
        assert!(labels(&at("c.cac", "check c {\n  select ser|")).contains(&"server"));
        let select = at("v.cav", "view v {\n  select ser|");
        assert!(labels(&select).contains(&"server"), "{select:?}");
        let group_by = at("v.cav", "view v {\n  select app\n    group-by |");
        assert!(labels(&group_by).contains(&"system") && labels(&group_by).contains(&"on"), "{group_by:?}");
        assert!(labels(&at("v.cav", "view v {\n  render |")).contains(&"svg"));
        let options = at("v.cav", "view v {\n  render svg {\n    |");
        assert!(labels(&options).contains(&"direction") && labels(&options).contains(&"cross"), "{options:?}");
        assert_eq!(labels(&at("v.cav", "view v { render svg { direction: |")), ["lr", "rl", "tb", "bt"]);
        assert_eq!(
            labels(&at("v.cav", "style {\n  edges where proto = https { line: |")),
            ["solid", "dashed", "dotted"]
        );
        assert_eq!(labels(&at("v.cav", "style {\n  layer |")), ["auth", "flow", "trigger"]);
    }

    #[test]
    fn comments_and_strings_are_skipped() {
        let items = at("b.ca", "site s { description: \"a { b\" // }\n  hosting: |");
        assert_eq!(labels(&items), ["on-prem", "colocation", "cloud", "office"]);
    }
}
