//! `cadastre anonymize`: a copy of a project where everything that identifies is replaced and
//! everything that shapes the diagrams is kept, to share a project (a bug, an unreadable view)
//! without exposing an information system.
//!
//! Kept: the kinds, the nesting, the links with their ports, protocols and layers, the
//! references, the views and the checks. Replaced, consistently across the files: the names of
//! the resources (by their kind and a number), of the views, checks and layers of the project,
//! the items of `for_each` lists; every string by a text of the same length (the width of a box
//! counts); the addresses and networks, renumbered; the host names; the comments, dropped. The
//! schema of the project (`kind`, `link`, `layer`, `group` blocks) is kept: it holds no data.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use tree_sitter::Node;

use crate::project::compile;
use crate::source::{FileRole, SourceFile};

/// The anonymized copies of the files, in the same order; CSV files are kept as they are, and
/// reported in `warnings`, as anything the tool left for the author to check.
pub struct Anonymized {
    pub files: Vec<SourceFile>,
    pub warnings: Vec<String>,
}

pub fn anonymize(files: &[SourceFile]) -> Anonymized {
    let compilation = compile(files);
    let registry = &compilation.registry;
    let mut renames = Renames::default();
    // Every resource short name, by its kind: `app-1`, `server-12`.
    let mut ids = registry.ids().collect::<Vec<_>>();
    ids.sort_by_key(|&id| (registry.resource(id).kind.clone(), registry.qualified_name(id)));
    let mut counters: HashMap<String, usize> = HashMap::new();
    for id in ids {
        let resource = registry.resource(id);
        if renames.names.contains_key(&resource.name) {
            continue;
        }
        let kind = resource.kind.clone();
        let n = counters.entry(kind.clone()).or_default();
        *n += 1;
        renames.names.insert(resource.name.clone(), format!("{kind}-{n}"));
    }
    // The words that are not names: kinds, keys, closed values, plan words.
    renames.keep.extend(crate::schema::kind_names().map(str::to_owned));
    renames.keep.extend(registry.schema.kind_names().into_iter().map(str::to_owned));
    renames.keep.extend(registry.schema.group_names().map(str::to_owned));
    renames.keep.extend(["flow", "auth", "trigger"].map(str::to_owned));
    // The keys whose value is a word of a closed list: never renamed, whatever the word.
    let schema = &registry.schema;
    let kinds: Vec<&'static crate::schema::KindDef> =
        schema.kind_names().into_iter().filter_map(|k| schema.kind_def(k)).collect();
    let closed = kinds
        .iter()
        .flat_map(|k| k.attrs.iter())
        .chain(crate::schema::RESOURCE_KEYS)
        .chain(crate::schema::LINK_KEYS)
        .chain(schema.link_keys(&[]))
        .filter(|a| matches!(a.ty, crate::schema::Ty::Symbol(_)))
        .map(|a| a.key.to_owned());
    renames.closed_keys.extend(closed);
    let typed = |wanted: fn(&crate::schema::Ty) -> bool| {
        kinds
            .iter()
            .flat_map(|k| k.attrs.iter())
            .chain(crate::schema::RESOURCE_KEYS)
            .chain(crate::schema::LINK_KEYS)
            .chain(schema.link_keys(&[]))
            .filter(|a| wanted(&a.ty))
            .map(|a| a.key.to_owned())
            .collect::<Vec<_>>()
    };
    renames.kept_strings.extend(typed(|ty| {
        matches!(
            ty,
            crate::schema::Ty::Spdx
                | crate::schema::Ty::Pattern(_)
                | crate::schema::Ty::Bandwidth
                | crate::schema::Ty::Semver
                | crate::schema::Ty::Version
        )
    }));
    renames.kept_strings.insert("schedule".to_owned());
    renames.url_keys.extend(typed(|ty| matches!(ty, crate::schema::Ty::Url)));
    renames.closed_keys.extend(
        [
            "hosting",
            "confidence",
            "scope",
            "reach",
            "exchange",
            "type",
            "severity",
            "direction",
            "orientation",
            "layout",
            "routing",
            "legend",
            "icons",
            "satellites",
            "title",
            "edge_labels_layout",
            "merge_label",
            "rows",
            "line",
            "shape",
            "color",
            "fill",
            "edge_labels",
            "node_labels",
            "columns",
            "loops",
            "merge",
            "expect",
        ]
        .map(str::to_owned),
    );
    let mut warnings = Vec::new();
    let mut out = Vec::new();
    for file in files {
        match FileRole::from_path(&file.path) {
            Some(_) => out.push(SourceFile { path: file.path.clone(), text: renames.rewrite(&file.text) }),
            None => {
                if file.path.ends_with(".csv") {
                    warnings.push(format!("{}: a CSV file, kept as it is: check it", file.path));
                }
                out.push(file.clone());
            }
        }
    }
    Anonymized { files: out, warnings }
}

#[derive(Default)]
struct Renames {
    /// Resource short name → neutral name.
    names: HashMap<String, String>,
    /// Identifiers never renamed.
    keep: BTreeSet<String>,
    /// Keys whose values are words of the language, never renamed.
    closed_keys: BTreeSet<String>,
    /// Keys whose strings are kept: a license, a version, a pattern, a schedule.
    kept_strings: BTreeSet<String>,
    /// Keys whose strings are URLs: made generic.
    url_keys: BTreeSet<String>,
    /// Other identifiers renamed on sight (views, checks, layers, items of `for_each`, hosts).
    others: HashMap<String, String>,
    /// Networks renumbered: the first three octets → new ones.
    networks: BTreeMap<String, String>,
    ipv6: HashMap<String, String>,
    strings: usize,
}

impl Renames {
    fn rewrite(&mut self, text: &str) -> String {
        let tree = crate::syntax::parse(text);
        let mut edits: Vec<(usize, usize, String)> = Vec::new();
        self.walk(tree.root_node(), text, &mut edits, None);
        edits.sort_by_key(|e| e.0);
        let mut out = String::with_capacity(text.len());
        let mut at = 0;
        for (start, end, replacement) in edits {
            if start < at {
                continue;
            }
            out.push_str(&text[at..start]);
            out.push_str(&replacement);
            at = end;
        }
        out.push_str(&text[at..]);
        // Blank lines left by the comments, folded.
        let mut folded = String::with_capacity(out.len());
        let mut blank = 0;
        for line in out.lines() {
            if line.trim().is_empty() {
                blank += 1;
                if blank > 1 {
                    continue;
                }
            } else {
                blank = 0;
            }
            folded.push_str(line);
            folded.push('\n');
        }
        folded
    }

    fn walk(&mut self, node: Node, text: &str, edits: &mut Vec<(usize, usize, String)>, field: Option<&str>) {
        let kind = node.kind();
        // The schema of the project holds no data: kept whole.
        if matches!(kind, "kind_declaration" | "link_declaration" | "group_declaration") {
            return;
        }
        let raw = &text[node.byte_range()];
        let replacement = match kind {
            "comment" => Some(String::new()),
            "string" => {
                let key = node
                    .parent()
                    .filter(|p| p.kind() == "attribute")
                    .and_then(|p| p.child_by_field_name("key"))
                    .map(|k| text[k.byte_range()].to_owned());
                match key {
                    Some(key) if self.kept_strings.contains(&key) => None,
                    Some(key) if self.url_keys.contains(&key) => {
                        let n = self.others.len() + 1;
                        let host =
                            self.others.entry(raw.to_owned()).or_insert_with(|| format!("host-{n}.example")).clone();
                        Some(format!("\"https://{host}/\""))
                    }
                    _ => Some(self.string(raw)),
                }
            }
            "ipv4" | "cidr4" => Some(self.ipv4(raw)),
            "ipv6" | "cidr6" => Some(self.ipv6(raw)),
            "identifier" if field != Some("key") && field != Some("kind") => self.identifier(raw, node, text),
            "qualified_name" => Some(self.qualified(raw, node)),
            _ => None,
        };
        if let Some(replacement) = replacement {
            if replacement != raw {
                edits.push((node.start_byte(), node.end_byte(), replacement));
            }
            return;
        }
        let mut cursor = node.walk();
        let children: Vec<(Node, Option<String>)> = {
            let mut list = Vec::new();
            for (i, child) in node.children(&mut cursor).enumerate() {
                list.push((child, node.field_name_for_child(i as u32).map(str::to_owned)));
            }
            list
        };
        for (child, field) in children {
            self.walk(child, text, edits, field.as_deref());
        }
    }

    /// An identifier: a resource name, a plan name (view, check, layer, zone), an item of a
    /// `for_each` list, a free value; a word of the language, and the value of a closed key, are
    /// kept.
    fn identifier(&mut self, raw: &str, node: Node, text: &str) -> Option<String> {
        if raw.contains('$') {
            return None;
        }
        let parent = node.parent()?;
        let parent_kind = parent.kind();
        let grand = parent.parent().map(|g| g.kind()).unwrap_or_default();
        let field = (0..parent.child_count())
            .find(|&i| parent.child(i).is_some_and(|c| c.id() == node.id()))
            .and_then(|i| parent.field_name_for_child(i));
        // The key this identifier is a value of, when it is one.
        let attribute = match (parent_kind, grand) {
            ("attribute", _) if field == Some("value") => Some(parent),
            ("list", "attribute") => parent.parent(),
            _ => None,
        };
        let key = attribute.and_then(|a| a.child_by_field_name("key")).map(|k| &text[k.byte_range()]);
        if key.is_some_and(|k| self.closed_keys.contains(k)) {
            return None;
        }
        // In a plan (a view, a check, a style): only the names of resources and the words renamed
        // elsewhere (a layer, a zone) change; the words of the plans stay.
        let in_plan = std::iter::successors(Some(parent), |n| n.parent()).any(|n| {
            matches!(n.kind(), "view" | "check" | "style_block" | "render_clause" | "edges_clause" | "zone_clause")
        });
        if let Some(name) = self.names.get(raw) {
            return Some(name.clone());
        }
        if in_plan
            && !matches!(
                (parent_kind, field),
                ("view" | "check", Some("name")) | ("zone_clause", Some("name")) | ("layers", _)
            )
        {
            return self.others.get(raw).cloned();
        }
        if self.keep.contains(raw) {
            return None;
        }
        // A name that is not a resource: the declaration of a view, a check, a zone; a layer; an
        // item of a `for_each`; a link end that resolves to nothing; a free value.
        let renamable = match (parent_kind, field, grand) {
            ("view" | "check", Some("name"), _) => parent_kind,
            ("zone_clause", Some("name"), _) => "zone",
            ("layers", _, _) => "layer",
            ("list", _, "for_each") | ("list", _, "list") => "item",
            ("link", Some("source" | "target"), _) | ("inline_link", Some("target"), _) => "name",
            ("resource", Some("name"), _) => "name",
            _ if key.is_some() => "value",
            _ => return None,
        };
        let n = self.others.len() + 1;
        let fresh = self.others.entry(raw.to_owned()).or_insert_with(|| format!("{renamable}-{n}")).clone();
        Some(fresh)
    }

    fn qualified(&mut self, raw: &str, node: Node) -> String {
        let segments: Vec<&str> = raw.split('.').collect();
        // In a plan, a dotted name is a path (`target.ancestor`) or a reference: its segments that
        // are names change, nothing else; a host name is only written in the registry.
        let in_plan = std::iter::successors(node.parent(), |n| n.parent()).any(|n| {
            matches!(n.kind(), "view" | "check" | "style_block" | "render_clause" | "edges_clause" | "zone_clause")
        });
        if in_plan || segments.iter().any(|s| self.names.contains_key(*s)) {
            return segments
                .iter()
                .map(|s| self.names.get(*s).cloned().unwrap_or_else(|| (*s).to_owned()))
                .collect::<Vec<_>>()
                .join(".");
        }
        // A host name.
        let n = self.others.len() + 1;
        self.others.entry(raw.to_owned()).or_insert_with(|| format!("host-{n}.example")).clone()
    }

    /// A string of the same length, in placeholder words.
    fn string(&mut self, raw: &str) -> String {
        let inner = &raw[1..raw.len() - 1];
        if inner.is_empty() {
            return raw.to_owned();
        }
        self.strings += 1;
        let words = ["lorem", "ipsum", "dolor", "sit", "amet", "consectetur", "adipiscing", "elit", "sed", "do"];
        let mut out = String::new();
        let mut i = self.strings;
        let target = inner.chars().count();
        while out.chars().count() < target {
            let word = words[i % words.len()];
            i += 1;
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(word);
        }
        let mut shortened: String = out.chars().take(target).collect();
        if shortened.ends_with(' ') {
            shortened.pop();
            shortened.push('x');
        }
        format!("\"{shortened}\"")
    }

    fn ipv4(&mut self, raw: &str) -> String {
        let (address, prefix) = raw.split_once('/').map_or((raw, None), |(a, p)| (a, Some(p)));
        let octets: Vec<&str> = address.split('.').collect();
        if octets.len() != 4 {
            return raw.to_owned();
        }
        let network = octets[..3].join(".");
        let n = self.networks.len() + 1;
        let renumbered = self.networks.entry(network).or_insert_with(|| format!("10.{}.{}", n / 256, n % 256)).clone();
        let mut out = format!("{renumbered}.{}", octets[3]);
        if let Some(prefix) = prefix {
            out.push('/');
            out.push_str(prefix);
        }
        out
    }

    fn ipv6(&mut self, raw: &str) -> String {
        let n = self.ipv6.len() + 1;
        let (address, prefix) = raw.split_once('/').map_or((raw, None), |(a, p)| (a, Some(p)));
        let renumbered = self.ipv6.entry(address.to_owned()).or_insert_with(|| format!("fd00::{n:x}")).clone();
        match prefix {
            Some(prefix) => format!("{renumbered}/{prefix}"),
            None => renumbered,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_strings_and_addresses() {
        let files = [SourceFile {
            path: "a.ca".into(),
            text: "// Acme\nteam acme-ops\nsite acme-paris {\n  hosting: colocation\n  owner: acme-ops\n  label: \"Acme Paris\"\n  \
                   network lan { subnet dmz { cidr: 192.168.10.0/24, server web01 { ip: 192.168.10.5 } } }\n}\n\
                   app shop { on: web01, -> dmz.web01 : https/443 [flow, acme-bus] }\n"
                .into(),
        }];
        let anonymized = anonymize(&files);
        let text = &anonymized.files[0].text;
        assert!(!text.contains("acme") && !text.contains("Acme") && !text.contains("192.168"), "{text}");
        assert!(
            text.contains("team team-1") && text.contains("site site-1") && text.contains("owner: team-1"),
            "{text}"
        );
        assert!(text.contains("hosting: colocation"), "{text}");
        assert!(
            text.contains("label: \"") && text.contains("cidr: 10.0.1.0/24") && text.contains("ip: 10.0.1.5"),
            "{text}"
        );
        assert!(text.contains("-> subnet-1.server-1 : https/443 [flow, layer-"), "{text}");
        // The copy compiles, with the same resources.
        let c = compile(&anonymized.files);
        assert!(
            c.diagnostics.iter().all(|d| d.severity != crate::diagnostic::Severity::Error),
            "{:?}\n{text}",
            c.diagnostics
        );
        assert_eq!(c.registry.resources.len(), 6);
    }
}
