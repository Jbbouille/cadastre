//! Syntax highlighting from the grammar's own query (`grammar/queries/highlights.scm`).
//!
//! Shared by the editors (LSP semantic tokens) and the browser playground, so that
//! every host colors a file the same way.

use std::sync::OnceLock;

use tree_sitter::{Query, QueryCursor, StreamingIterator};

use crate::syntax;

/// The highlighting query, as shipped with the grammar.
pub const HIGHLIGHTS_QUERY: &str = include_str!("../../../../grammar/queries/highlights.scm");

/// A highlighted range of a text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Highlight {
    /// Byte range.
    pub start: usize,
    pub end: usize,
    /// Capture name of the query, such as `keyword` or `constant.builtin`.
    pub name: &'static str,
}

fn query() -> &'static Query {
    static QUERY: OnceLock<Query> = OnceLock::new();
    QUERY.get_or_init(|| {
        Query::new(&syntax::language(), HIGHLIGHTS_QUERY).expect("highlights.scm is valid for the Cadastre grammar")
    })
}

/// Highlights a `.ca` or `.cav` text: non-overlapping ranges, in text order.
///
/// When two captures cover the same node, the first pattern of the query wins.
/// Ranges may span several lines (block comments).
pub fn highlight(source: &str) -> Vec<Highlight> {
    let tree = syntax::parse(source);
    let query = query();
    let names = query.capture_names();

    let mut found = Vec::new();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, tree.root_node(), source.as_bytes());
    while let Some(m) = matches.next() {
        for capture in m.captures() {
            let node = capture.node;
            found.push((node.start_byte(), node.end_byte(), m.pattern_index, names[capture.index as usize]));
        }
    }
    found.sort_by_key(|&(start, end, pattern, _)| (start, std::cmp::Reverse(end), pattern));

    let mut highlights: Vec<Highlight> = Vec::with_capacity(found.len());
    for (start, end, _, name) in found {
        if start == end || highlights.last().is_some_and(|last| start < last.end) {
            continue;
        }
        highlights.push(Highlight { start, end, name });
    }
    highlights
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(source: &str) -> Vec<(&str, &'static str)> {
        highlight(source).iter().map(|h| (&source[h.start..h.end], h.name)).collect()
    }

    #[test]
    fn highlights_a_registry() {
        let source = "// sites\nserver web01 { os: debian12, ip: 10.0.1.5 }\nbilling -> crm : https/443 [flow]\n";
        assert_eq!(
            named(source),
            [
                ("// sites", "comment"),
                ("server", "type"),
                ("web01", "variable"),
                ("{", "punctuation.bracket"),
                ("os", "property"),
                (":", "punctuation.delimiter"),
                (",", "punctuation.delimiter"),
                ("ip", "property"),
                (":", "punctuation.delimiter"),
                ("10.0.1.5", "string.special"),
                ("}", "punctuation.bracket"),
                ("->", "operator"),
                (":", "punctuation.delimiter"),
                ("https", "constant.builtin"),
                ("/", "operator"),
                ("443", "number"),
                ("[", "punctuation.bracket"),
                ("flow", "label"),
                ("]", "punctuation.bracket"),
            ]
        );
    }

    #[test]
    fn highlights_a_view() {
        let source = "view apps {\n  select app where criticality >= 2 group-by site\n  render svg\n}\n";
        let highlights = named(source);
        assert!(highlights.contains(&("view", "keyword")));
        assert!(highlights.contains(&("apps", "function")));
        assert!(highlights.contains(&("app", "type")));
        assert!(highlights.contains(&("where", "keyword")));
        assert!(highlights.contains(&(">=", "operator")));
        assert!(highlights.contains(&("group-by", "keyword")));
        assert!(highlights.contains(&("site", "type")));
        assert!(highlights.contains(&("svg", "constant")));
    }

    #[test]
    fn ranges_do_not_overlap() {
        let source =
            "/* a\n   b */ app a.b { carries: [x.y, z] }\nview v { edges lift(flow, through: on?.ancestor(site)) }\n";
        let highlights = highlight(source);
        for pair in highlights.windows(2) {
            assert!(pair[0].end <= pair[1].start, "{pair:?}");
        }
    }
}
