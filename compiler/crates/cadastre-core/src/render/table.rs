//! Table views: the nodes or the edges of a view as rows, chosen keys and paths as columns,
//! written as CSV, TSV, Markdown or an HTML page.

use crate::plan::{Path, Step, StepKind};
use crate::query::{Item, Subject};
use crate::source::{FileId, Span};

use super::Scene;

/// The columns written when the view does not choose them.
const NODE_COLUMNS: &[&str] = &["name", "kind", "label"];
const EDGE_COLUMNS: &[&str] = &["source", "target", "port", "carries"];

/// A table format.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Table {
    Csv,
    Tsv,
    Markdown,
    /// A whole HTML page, to open in a browser.
    Html,
}

/// The style of an HTML table: readable in light and dark themes, the header kept in sight.
const HTML_STYLE: &str = "\
:root { color-scheme: light dark; --line: #d0d7de; --head: #f6f8fa; }
@media (prefers-color-scheme: dark) { :root { --line: #30363d; --head: #161b22; } }
body { margin: 16px; font: 14px/1.4 system-ui, sans-serif; }
h1 { font-size: 18px; margin: 0 0 12px; }
table { border-collapse: collapse; }
th, td { border: 1px solid var(--line); padding: 4px 8px; text-align: left; vertical-align: top; }
th { background: var(--head); position: sticky; top: 0; }
tbody tr:hover { background: var(--head); }
";

/// A column written in `columns`, parsed as a path: `ip`, `on.label`, `ancestor(site)`,
/// `on?.ancestor(site)`. `None` when it is not one.
pub fn column(text: &str) -> Option<Path> {
    let span = Span { file: FileId(0), start: 0, end: 0, line: 0, column: 0 };
    let mut steps = Vec::new();
    for part in split_steps(text)? {
        let (part, optional) = match part.strip_suffix('?') {
            Some(rest) => (rest, true),
            None => (part, false),
        };
        let kind = match part.strip_prefix("ancestor(").and_then(|r| r.strip_suffix(')')) {
            Some(kind) if is_name(kind) => StepKind::Ancestor(kind.to_owned()),
            Some(_) => return None,
            None if is_name(part) => StepKind::Name(part.to_owned()),
            None => return None,
        };
        steps.push(Step { kind, optional, span });
    }
    (!steps.is_empty()).then_some(Path { steps, span })
}

/// The steps of a path, split on the dots outside parentheses.
fn split_steps(text: &str) -> Option<Vec<&str>> {
    let mut parts = Vec::new();
    let (mut depth, mut start) = (0usize, 0);
    for (i, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.checked_sub(1)?,
            '.' if depth == 0 => {
                parts.push(&text[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    (depth == 0 && parts.iter().all(|p| !p.is_empty())).then_some(parts)
}

fn is_name(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// The rows of a table view: the names of its columns, which hold numbers, and the text of its cells.
pub(super) struct Grid {
    pub(super) names: Vec<String>,
    /// A column of a key typed `number` by the schema, or of `port` or `step`.
    pub(super) numeric: Vec<bool>,
    pub(super) rows: Vec<Vec<String>>,
}

/// The table of a view in a format.
pub fn table(scene: &Scene, format: Table) -> String {
    let Grid { names, rows, .. } = grid(scene);
    render(scene, format, &names, &rows)
}

/// The rows of a view: one per node, or per edge (`rows`), its `columns` or the default ones.
pub(super) fn grid(scene: &Scene) -> Grid {
    let options = &scene.graph.options;
    let edges = match options.rows.as_deref() {
        Some("nodes") => false,
        Some("edges") => true,
        _ => scene.graph.has_edge_clause,
    };
    let names: Vec<String> = if options.columns.is_empty() {
        (if edges { EDGE_COLUMNS } else { NODE_COLUMNS }).iter().map(|c| (*c).to_owned()).collect()
    } else {
        options.columns.clone()
    };
    let paths: Vec<Option<Path>> = names.iter().map(|n| column(n)).collect();
    let ctx = scene.ctx();
    let cell = |subject: &Subject, path: &Option<Path>| -> String {
        let Some(path) = path else { return String::new() };
        let items = ctx.path(subject, path);
        let texts: Vec<String> = items
            .iter()
            .map(|item| match item {
                Item::Resource(id) => scene.registry.resource(*id).name.clone(),
                Item::Value(text) => text.text.clone(),
            })
            .collect();
        texts.join(", ")
    };
    let rows: Vec<Vec<String>> = if edges {
        scene.graph.edges.iter().map(|edge| paths.iter().map(|p| cell(&Subject::Edge(edge), p)).collect()).collect()
    } else {
        scene.graph.nodes.iter().map(|&id| paths.iter().map(|p| cell(&Subject::Node(id), p)).collect()).collect()
    };
    let numeric = paths
        .iter()
        .map(|path| match path.as_ref().and_then(|p| p.steps.last()).map(|s| &s.kind) {
            Some(StepKind::Name(key)) if key == "port" || key == "step" => true,
            Some(StepKind::Name(key)) => {
                let mut types = crate::schema::KINDS
                    .iter()
                    .flat_map(|k| k.attrs.iter())
                    .chain(crate::schema::RESOURCE_KEYS)
                    .chain(crate::schema::LINK_KEYS)
                    .filter(|a| a.key == key)
                    .peekable();
                types.peek().is_some() && types.all(|a| matches!(a.ty, crate::schema::Ty::Number))
            }
            _ => false,
        })
        .collect();
    Grid { names, numeric, rows }
}

fn render(scene: &Scene, format: Table, names: &[String], rows: &[Vec<String>]) -> String {
    match format {
        Table::Csv | Table::Tsv => {
            let separator = if format == Table::Csv { ',' } else { '\t' };
            // A byte order mark, for Excel to read the file as UTF-8.
            let mut out = String::from("\u{feff}");
            for row in std::iter::once(names).chain(rows.iter().map(Vec::as_slice)) {
                let fields: Vec<String> = row.iter().map(|f| delimited(f, separator)).collect();
                out.push_str(&fields.join(&separator.to_string()));
                out.push_str("\r\n");
            }
            out
        }
        Table::Html => {
            let cells = |row: &[String], tag: &str| -> String {
                row.iter().map(|f| format!("<{tag}>{}</{tag}>", html_cell(f))).collect()
            };
            let title = html(&scene.graph.view);
            let mut out = format!(
                "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
                 <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
                 <title>{title}</title>\n<style>\n{HTML_STYLE}</style>\n</head>\n<body>\n<h1>{title}</h1>\n<table>\n\
                 <thead>\n<tr>{}</tr>\n</thead>\n<tbody>\n",
                cells(names, "th")
            );
            for row in rows {
                out.push_str(&format!("<tr>{}</tr>\n", cells(row, "td")));
            }
            out.push_str("</tbody>\n</table>\n</body>\n</html>\n");
            out
        }
        Table::Markdown => {
            let line =
                |row: &[String]| format!("| {} |\n", row.iter().map(|f| markdown(f)).collect::<Vec<_>>().join(" | "));
            let mut out = line(names);
            out.push_str(&format!("|{}\n", "---|".repeat(names.len())));
            for row in rows {
                out.push_str(&line(row));
            }
            out
        }
    }
}

/// A CSV or TSV field: quoted when it holds the separator, a quote or a line break.
fn delimited(field: &str, separator: char) -> String {
    if field.contains([separator, '"', '\n', '\r']) {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_owned()
    }
}

/// The URLs of a cell (`url`, `console`), when it holds only URLs.
pub(super) fn urls(field: &str) -> Option<Vec<&str>> {
    let urls: Vec<&str> = field.split(", ").collect();
    (!field.is_empty() && urls.iter().all(|url| crate::net::check_url(url).is_ok())).then_some(urls)
}

/// A cell in HTML: its URLs as links (`url`, `console`), otherwise its text.
fn html_cell(field: &str) -> String {
    let urls: Vec<&str> = field.split(", ").collect();
    if field.is_empty() || !urls.iter().all(|url| crate::net::check_url(url).is_ok()) {
        return html(field);
    }
    let links: Vec<String> = urls.iter().map(|url| format!("<a href=\"{0}\">{0}</a>", html(url))).collect();
    links.join(", ")
}

/// Text in HTML: its special characters escaped, its line breaks as `<br>`.
fn html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace("\r\n", "<br>")
        .replace('\n', "<br>")
}

/// A Markdown table cell: its pipes escaped, its line breaks as `<br>`.
fn markdown(field: &str) -> String {
    field.replace('|', "\\|").replace("\r\n", "<br>").replace('\n', "<br>")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_are_paths() {
        let path = column("on?.ancestor(site)").unwrap();
        assert_eq!(path.steps.len(), 2);
        assert!(path.steps[0].optional && path.steps[1].kind == StepKind::Ancestor("site".into()));
        assert!(column("ip").is_some() && column("on.label").is_some());
        assert!(column("ancestor(site").is_none() && column("a..b").is_none() && column("x y").is_none());
    }

    #[test]
    fn fields_are_escaped() {
        assert_eq!(delimited("a, b", ','), "\"a, b\"");
        assert_eq!(delimited("a, b", '\t'), "a, b");
        assert_eq!(delimited("say \"hi\"", ','), "\"say \"\"hi\"\"\"");
        assert_eq!(markdown("a|b\nc"), "a\\|b<br>c");
        assert_eq!(html("<b> & \"x\"\ny"), "&lt;b&gt; &amp; &quot;x&quot;<br>y");
    }
}
