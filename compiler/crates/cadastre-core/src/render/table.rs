//! Table views: the nodes or the edges of a view as rows, chosen keys and paths as columns,
//! written as CSV, TSV, Markdown or an HTML page.

use std::collections::{BTreeMap, BTreeSet};

use crate::plan::{Path, Step, StepKind};
use crate::query::{Item, Subject};
use crate::registry::ResourceId;
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

/// A column of a table: its header, and what its cells hold.
pub struct Column {
    pub header: String,
    pub path: Path,
    /// `count(path)`: the number of distinct items of the path, rather than the items.
    pub count: bool,
}

/// A column written in `columns`: a path (`ip`, `on.label`, `ancestor(site)`,
/// `on?.ancestor(site)`) or the count of its items (`count(on)`), then its header when it is not
/// the text of the column (`on.ancestor(site) as Site`). `None` when it is not one.
pub fn column(text: &str) -> Option<Column> {
    let (value, header) = match text.rsplit_once(" as ") {
        Some((value, header)) if !header.trim().is_empty() => (value.trim(), header.trim()),
        _ => (text.trim(), text.trim()),
    };
    let (inner, count) = match value.strip_prefix("count(").and_then(|r| r.strip_suffix(')')) {
        Some(inner) => (inner, true),
        None => (value, false),
    };
    Some(Column { header: header.to_owned(), path: path(inner)?, count })
}

/// `rows: "each(path)"`: the path whose items each give a row; `None` for `nodes`, `edges`.
pub fn each(text: &str) -> Option<Path> {
    path(text.strip_prefix("each(")?.strip_suffix(')')?)
}

/// A path written in a column or in `rows`.
fn path(text: &str) -> Option<Path> {
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
            // A key with an argument: `referrers(backup_policy)`, `failures(hosting)`.
            None if part
                .strip_suffix(')')
                .and_then(|r| r.split_once('('))
                .is_some_and(|(function, argument)| is_name(function) && is_name(argument)) =>
            {
                StepKind::Name(part.to_owned())
            }
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
    /// The node of each row, when the rows are nodes.
    pub(super) subjects: Vec<Option<ResourceId>>,
    /// The resource a cell names, when it names one and only one (`on` of an app on one server).
    pub(super) resources: Vec<Vec<Option<ResourceId>>>,
}

/// The table of a view in a format.
pub fn table(scene: &Scene, format: Table) -> String {
    let Grid { names, rows, .. } = grid(scene);
    render(scene, format, &names, &rows)
}

/// The rows of a view: one per node, or per edge (`rows`), or per item of a path from each
/// (`rows: "each(on)"`); its `columns` or the default ones.
pub(super) fn grid(scene: &Scene) -> Grid {
    let options = &scene.graph.options;
    let edges = match options.rows.as_deref() {
        Some("nodes") => false,
        Some("edges") => true,
        _ => scene.graph.has_edge_clause,
    };
    if let Some(group) = options.group.as_deref().and_then(column) {
        return pivot(scene, edges, &group);
    }
    let each = options.rows.as_deref().and_then(each);
    let written: Vec<String> = if options.columns.is_empty() {
        (if edges { EDGE_COLUMNS } else { NODE_COLUMNS }).iter().map(|c| (*c).to_owned()).collect()
    } else {
        options.columns.clone()
    };
    let columns: Vec<Option<Column>> = written.iter().map(|n| column(n)).collect();
    let names: Vec<String> = written
        .iter()
        .zip(&columns)
        .map(|(text, column)| column.as_ref().map_or_else(|| text.clone(), |c| c.header.clone()))
        .collect();
    let ctx = scene.ctx();
    // The items of a column on a row: from the item of the `each` path the column starts with,
    // from the node or the edge of the row otherwise.
    let items = |subject: &Subject, chain: &[Item], path: &Path| -> Vec<Item> {
        let shared = each
            .as_ref()
            .map_or(0, |each| each.steps.iter().zip(&path.steps).take_while(|(a, b)| a.kind == b.kind).count());
        let shared = shared.min(chain.len());
        if shared == 0 {
            return ctx.path(subject, path);
        }
        let rest = &path.steps[shared..];
        match &chain[shared - 1] {
            item if rest.is_empty() => vec![item.clone()],
            Item::Resource(id) => ctx.path(&Subject::Node(*id), &Path { steps: rest.to_vec(), span: path.span }),
            Item::Value(_) => Vec::new(),
        }
    };
    let cell = |subject: &Subject, chain: &[Item], column: &Option<Column>| -> (String, Option<ResourceId>) {
        let Some(column) = column else { return (String::new(), None) };
        // A count is that of the node or the edge, the same on each of its rows.
        if column.count {
            return (ctx.path(subject, &column.path).len().to_string(), None);
        }
        let items = items(subject, chain, &column.path);
        let texts: Vec<String> = items
            .iter()
            .map(|item| match item {
                Item::Resource(id) => scene.registry.resource(*id).name.clone(),
                Item::Value(text) => text.text.clone(),
            })
            .collect();
        let resource = match items.as_slice() {
            [Item::Resource(id)] => Some(*id),
            _ => None,
        };
        (texts.join(", "), resource)
    };
    type Row = (Option<ResourceId>, Vec<(String, Option<ResourceId>)>);
    let row = |subject: &Subject| -> Vec<Row> {
        let chains = match &each {
            Some(each) => chains(&ctx, subject, each),
            None => vec![Vec::new()],
        };
        let node = match subject {
            Subject::Node(id) => Some(*id),
            Subject::Edge(_) => None,
        };
        chains.iter().map(|chain| (node, columns.iter().map(|c| cell(subject, chain, c)).collect())).collect()
    };
    let built: Vec<Row> = if edges {
        scene.graph.edges.iter().flat_map(|edge| row(&Subject::Edge(edge))).collect()
    } else {
        scene.graph.nodes.iter().flat_map(|&id| row(&Subject::Node(id))).collect()
    };
    let subjects = built.iter().map(|(node, _)| *node).collect();
    let resources = built.iter().map(|(_, cells)| cells.iter().map(|(_, r)| *r).collect()).collect();
    let rows: Vec<Vec<String>> =
        built.into_iter().map(|(_, cells)| cells.into_iter().map(|(t, _)| t).collect()).collect();
    let numeric = columns
        .iter()
        .map(|column| match column.as_ref().map(|c| (c.count, c.path.steps.last().map(|s| &s.kind))) {
            Some((true, _)) => true,
            Some((_, Some(StepKind::Name(key)))) if key == "port" || key == "step" => true,
            Some((_, Some(StepKind::Name(key)))) => {
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
    Grid { names, numeric, rows, subjects, resources }
}

/// A pivot table (`group`): a row per value of the column grouped by, the number of nodes (or
/// edges) having it, then a column per value of the columns counted (`count`), and a total row.
/// A node with several values is counted in each, without any in an empty row, last.
fn pivot(scene: &Scene, edges: bool, group: &Column) -> Grid {
    let ctx = scene.ctx();
    let counted: Vec<Column> = scene.graph.options.count.iter().filter_map(|c| column(c)).collect();
    let values = |subject: &Subject, path: &Path| -> BTreeSet<String> {
        ctx.path(subject, path)
            .iter()
            .map(|item| match item {
                Item::Resource(id) => scene.registry.resource(*id).name.clone(),
                Item::Value(text) => text.text.clone(),
            })
            .collect()
    };
    let subjects: Vec<Subject> = if edges {
        scene.graph.edges.iter().map(|edge| Subject::Edge(edge)).collect()
    } else {
        scene.graph.nodes.iter().map(|&id| Subject::Node(id)).collect()
    };
    // By group (the empty one last): its count, and the count of each value of each column.
    type Counts = (usize, Vec<BTreeMap<String, usize>>);
    let mut groups: BTreeMap<(bool, String), Counts> = BTreeMap::new();
    let mut seen: Vec<BTreeSet<String>> = vec![BTreeSet::new(); counted.len()];
    let mut totals: Vec<BTreeMap<String, usize>> = vec![BTreeMap::new(); counted.len()];
    for subject in &subjects {
        let mut keys = values(subject, &group.path);
        if keys.is_empty() {
            keys.insert(String::new());
        }
        let by: Vec<BTreeSet<String>> = counted.iter().map(|c| values(subject, &c.path)).collect();
        for (c, set) in by.iter().enumerate() {
            for value in set {
                seen[c].insert(value.clone());
                *totals[c].entry(value.clone()).or_default() += 1;
            }
        }
        for key in keys {
            let entry =
                groups.entry((key.is_empty(), key)).or_insert_with(|| (0, vec![BTreeMap::new(); counted.len()]));
            entry.0 += 1;
            for (c, set) in by.iter().enumerate() {
                for value in set {
                    *entry.1[c].entry(value.clone()).or_default() += 1;
                }
            }
        }
    }
    let mut names = vec![group.header.clone(), "count".to_owned()];
    for (c, column) in counted.iter().enumerate() {
        for value in &seen[c] {
            names.push(if counted.len() == 1 { value.clone() } else { format!("{}: {value}", column.header) });
        }
    }
    let line = |label: String, count: usize, by: &[BTreeMap<String, usize>]| -> Vec<String> {
        let mut row = vec![label, count.to_string()];
        for (c, values) in seen.iter().enumerate() {
            row.extend(values.iter().map(|v| by[c].get(v).copied().unwrap_or(0).to_string()));
        }
        row
    };
    let mut rows: Vec<Vec<String>> = groups.into_iter().map(|((_, key), (count, by))| line(key, count, &by)).collect();
    rows.push(line(scene.t("Total").to_owned(), subjects.len(), &totals));
    let numeric = (0..names.len()).map(|c| c > 0).collect();
    let resources = rows.iter().map(|row| vec![None; row.len()]).collect();
    Grid { names, numeric, subjects: vec![None; rows.len()], resources, rows }
}

/// The chains of items the `each` path goes through from a node or an edge, one per row: a step
/// giving nothing ends its chain there, so that every node or edge keeps a row.
fn chains(ctx: &crate::query::Ctx, subject: &Subject, each: &Path) -> Vec<Vec<Item>> {
    let mut done = Vec::new();
    let mut chains: Vec<Vec<Item>> = vec![Vec::new()];
    for step in &each.steps {
        let single = Path { steps: vec![step.clone()], span: each.span };
        let mut next = Vec::new();
        for chain in chains {
            let produced = match chain.last() {
                None => ctx.path(subject, &single),
                Some(Item::Resource(id)) => ctx.path(&Subject::Node(*id), &single),
                Some(Item::Value(_)) => Vec::new(),
            };
            if produced.is_empty() {
                done.push(chain);
                continue;
            }
            for item in produced {
                let mut longer = chain.clone();
                longer.push(item);
                next.push(longer);
            }
        }
        chains = next;
    }
    done.extend(chains);
    done
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
        let written = column("on?.ancestor(site)").unwrap();
        let path = &written.path;
        assert_eq!((path.steps.len(), written.header.as_str(), written.count), (2, "on?.ancestor(site)", false));
        assert!(path.steps[0].optional && path.steps[1].kind == StepKind::Ancestor("site".into()));
        assert!(column("ip").is_some() && column("on.label").is_some());
        assert!(column("ancestor(site").is_none() && column("a..b").is_none() && column("x y").is_none());
        let sites = column("count(on.ancestor(site)) as Sites").unwrap();
        assert_eq!((sites.header.as_str(), sites.count, sites.path.steps.len()), ("Sites", true, 2));
        assert!(column("count(on as x").is_none() && each("each(on.on)").is_some() && each("nodes").is_none());
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
