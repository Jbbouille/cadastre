//! Outline of a file: its resources, links, views and style blocks, as nested symbols.
//!
//! Built from the syntax tree of the file alone, so that it is available while the
//! project does not compile.

use tree_sitter::Node;

use crate::syntax;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SymbolKind {
    /// A resource, with its kind (`app`, `server`…).
    Resource(String),
    Link,
    View,
    Style,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Symbol {
    pub name: String,
    /// Short complement: the kind of a resource, the port and layers of a link.
    pub detail: String,
    /// Views and checks are both [`SymbolKind::View`], told apart by `detail`.
    pub kind: SymbolKind,
    /// Byte range of the whole element.
    pub range: (usize, usize),
    /// Byte range of its name.
    pub selection: (usize, usize),
    pub children: Vec<Symbol>,
}

/// The symbols of a `.ca` or `.cav` text, in text order.
pub fn outline(source: &str) -> Vec<Symbol> {
    let tree = syntax::parse(source);
    symbols(tree.root_node(), source)
}

fn symbols(parent: Node, source: &str) -> Vec<Symbol> {
    let mut cursor = parent.walk();
    parent.named_children(&mut cursor).filter_map(|node| symbol(node, source)).collect()
}

fn symbol(node: Node, source: &str) -> Option<Symbol> {
    let text = |n: Node| source[n.byte_range()].to_owned();
    let range = |n: Node| (n.start_byte(), n.end_byte());
    let keyword = |n: Node| n.child(0).map_or(range(n), range);
    match node.kind() {
        // A process: a resource, its steps as children.
        "process" => {
            let name = node.child_by_field_name("name")?;
            Some(Symbol {
                name: text(name),
                detail: "process".into(),
                kind: SymbolKind::Resource("process".into()),
                range: range(node),
                selection: range(name),
                children: symbols(node, source),
            })
        }
        "participant" => {
            let name = node.child_by_field_name("name")?;
            Some(Symbol {
                name: text(name),
                detail: "participant".into(),
                kind: SymbolKind::Resource("participant".into()),
                range: range(node),
                selection: range(name),
                children: Vec::new(),
            })
        }
        "step" => {
            let source_name = node.child_by_field_name("source")?;
            let name = match (node.child_by_field_name("target"), node.child_by_field_name("text")) {
                (Some(target), _) => format!("{} -> {}", text(source_name), text(target)),
                (None, Some(t)) => format!("{} {}", text(source_name), text(t)),
                _ => text(source_name),
            };
            Some(Symbol {
                name,
                detail: "step".into(),
                kind: SymbolKind::Link,
                range: range(node),
                selection: range(source_name),
                children: Vec::new(),
            })
        }
        "loop" => {
            let label = node.child_by_field_name("label")?;
            Some(Symbol {
                name: format!("loop {}", text(label)),
                detail: "loop".into(),
                kind: SymbolKind::Style,
                range: range(node),
                selection: keyword(node),
                children: symbols(node, source),
            })
        }
        "resource" => {
            let kind = node.child_by_field_name("kind")?;
            let name = node.child_by_field_name("name")?;
            Some(Symbol {
                name: text(name),
                detail: text(kind),
                kind: SymbolKind::Resource(text(kind)),
                range: range(node),
                selection: range(name),
                children: node.child_by_field_name("body").map(|body| symbols(body, source)).unwrap_or_default(),
            })
        }
        "link" | "inline_link" => {
            let target = node.child_by_field_name("target")?;
            let source_name = node.child_by_field_name("source").map(|s| format!("{} ", text(s))).unwrap_or_default();
            let port = node.child_by_field_name("port").map(text);
            let layers = node.child_by_field_name("layers").map(text);
            Some(Symbol {
                name: format!("{source_name}-> {}", text(target)),
                detail: port.into_iter().chain(layers).collect::<Vec<_>>().join(" "),
                kind: SymbolKind::Link,
                range: range(node),
                selection: range(target),
                children: Vec::new(),
            })
        }
        "view" | "check" => {
            let name = node.child_by_field_name("name")?;
            Some(Symbol {
                name: text(name),
                detail: node.kind().into(),
                kind: SymbolKind::View,
                range: range(node),
                selection: range(name),
                children: symbols(node, source),
            })
        }
        "style_block" => Some(Symbol {
            name: "style".into(),
            detail: String::new(),
            kind: SymbolKind::Style,
            range: range(node),
            selection: keyword(node),
            children: Vec::new(),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(symbols: &[Symbol]) -> Vec<String> {
        let mut lines = Vec::new();
        fn walk(symbols: &[Symbol], depth: usize, lines: &mut Vec<String>) {
            for s in symbols {
                lines.push(format!("{}{} ({})", "  ".repeat(depth), s.name, s.detail));
                walk(&s.children, depth + 1, lines);
            }
        }
        walk(symbols, 0, &mut lines);
        lines
    }

    #[test]
    fn outlines_a_registry() {
        let source = "site paris { hosting: onprem\n  network lan { subnet dmz { server web01 } } }\n\
                      app billing { on: web01, -> crm : https/443 [flow] }\n\
                      billing -> crm : tcp/22\n";
        assert_eq!(
            tree(&outline(source)),
            [
                "paris (site)",
                "  lan (network)",
                "    dmz (subnet)",
                "      web01 (server)",
                "billing (app)",
                "  -> crm (https/443 [flow])",
                "billing -> crm (tcp/22)",
            ]
        );
        let web01 = &outline(source)[0].children[0].children[0].children[0];
        assert_eq!(&source[web01.selection.0..web01.selection.1], "web01");
    }

    #[test]
    fn outlines_plans() {
        let source = "style { layer flow { color: red } }\nview apps {\n  select app\n  style { nodes { shape: box } }\n}\ncheck owners { select app expect some }\n";
        assert_eq!(tree(&outline(source)), ["style ()", "apps (view)", "  style ()", "owners (check)"]);
    }

    #[test]
    fn keeps_what_parses_around_errors() {
        let source = "app a { on: }\napp b { }\n";
        let names: Vec<String> = outline(source).into_iter().map(|s| s.name).collect();
        assert!(names.contains(&"b".to_owned()), "{names:?}");
    }
}
