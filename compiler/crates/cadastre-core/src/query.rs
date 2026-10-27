//! Evaluation of paths and `where` predicates (spec § 6, Paths and Filters).

use crate::check::{Holder, Model};
use crate::plan::{Op, Path, Predicate, StepKind};
use crate::registry::{Registry, ResourceId, Value, ValueKind};

/// What a path step produces: a resource, or a plain value.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Resource(ResourceId),
    Value(Text),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Text {
    pub text: String,
    /// `8000-8100`: a port range, matched by inclusion.
    pub is_port: bool,
}

impl Text {
    fn plain(text: impl Into<String>) -> Item {
        Item::Value(Text { text: text.into(), is_port: false })
    }
}

/// An edge as seen by predicates: its ends and the attributes it exposes.
pub trait EdgeView {
    fn source(&self) -> ResourceId;
    fn target(&self) -> ResourceId;
    /// Value of an edge-level key (`proto`, `port`, `kind`, `carries`…), `None` if the key is unknown.
    fn attribute(&self, ctx: &Ctx, key: &str) -> Option<Vec<Item>>;
}

pub enum Subject<'e> {
    Node(ResourceId),
    Edge(&'e dyn EdgeView),
}

pub struct Ctx<'a> {
    pub registry: &'a Registry,
    pub model: &'a Model,
    /// The resources unavailable in the scenario of the view (`failing`): the `down` ones.
    pub down: &'a std::collections::BTreeSet<ResourceId>,
}

/// No scenario: nothing down.
pub static NOTHING_DOWN: std::collections::BTreeSet<ResourceId> = std::collections::BTreeSet::new();

impl<'a> Ctx<'a> {
    /// A context without a scenario of failures.
    pub fn new(registry: &'a Registry, model: &'a Model) -> Self {
        Ctx { registry, model, down: &NOTHING_DOWN }
    }
}

impl Ctx<'_> {
    pub fn matches(&self, subject: &Subject, predicate: &Predicate) -> bool {
        match predicate {
            Predicate::Flag(path) => self
                .path(subject, path)
                .iter()
                .any(|i| matches!(i, Item::Resource(_)) || matches!(i, Item::Value(t) if t.text == "true")),
            Predicate::Exists(path) => !self.path(subject, path).is_empty(),
            Predicate::Compare(path, op, value) => {
                let items = self.path(subject, path);
                match op {
                    Op::Eq => items.iter().any(|i| self.equals(i, value)),
                    Op::Ne => items.iter().any(|i| !self.equals(i, value)),
                    _ => items.iter().any(|i| compare(i, value).is_some_and(|o| op_holds(*op, o))),
                }
            }
            Predicate::ComparePaths(left, op, right) => {
                let (left, right) = (self.path(subject, left), self.path(subject, right));
                // Only what is asserted is compared: a side without value makes the comparison false.
                if left.is_empty() || right.is_empty() {
                    return false;
                }
                let common = || left.iter().any(|a| right.contains(a));
                match op {
                    Op::Eq => common(),
                    Op::Ne => !common(),
                    _ => left.iter().any(|a| {
                        right.iter().any(|b| match (a, b) {
                            (Item::Value(a), Item::Value(b)) => {
                                compare_texts(&a.text, &b.text).is_some_and(|o| op_holds(*op, o))
                            }
                            _ => false,
                        })
                    }),
                }
            }
            // `count`: the number of distinct items of the path (validated: no other function).
            Predicate::Aggregate(aggregate) => {
                let count = self.path(subject, &aggregate.path).len() as f64;
                let wanted: Option<f64> = value_text(&aggregate.value).parse().ok();
                wanted.and_then(|w| count.partial_cmp(&w)).is_some_and(|o| op_holds(aggregate.op, o))
            }
            Predicate::Has(path, value) => self.path(subject, path).iter().any(|i| self.equals(i, value)),
            Predicate::In(path, values) => {
                let items = self.path(subject, path);
                items.iter().any(|i| values.iter().any(|v| self.equals(i, v)))
            }
            Predicate::And(a, b) => self.matches(subject, a) && self.matches(subject, b),
            Predicate::Or(a, b) => self.matches(subject, a) || self.matches(subject, b),
            Predicate::Not(a) => !self.matches(subject, a),
        }
    }

    /// Evaluates a path from a node or an edge.
    pub fn path(&self, subject: &Subject, path: &Path) -> Vec<Item> {
        let mut steps = path.steps.iter();
        let mut items = match subject {
            Subject::Node(id) => vec![Item::Resource(*id)],
            Subject::Edge(edge) => {
                let Some(first) = steps.next() else { return Vec::new() };
                let StepKind::Name(name) = &first.kind else { return Vec::new() };
                let head = match name.as_str() {
                    "source" => vec![Item::Resource(edge.source())],
                    "target" => vec![Item::Resource(edge.target())],
                    other => edge.attribute(self, other).unwrap_or_default(),
                };
                if head.is_empty() && first.optional {
                    return Vec::new();
                }
                head
            }
        };
        for step in steps {
            let mut next = Vec::new();
            for item in &items {
                let produced = match item {
                    Item::Resource(id) => self.step(*id, &step.kind),
                    Item::Value(_) => Vec::new(),
                };
                if produced.is_empty() && step.optional {
                    next.push(item.clone());
                } else {
                    next.extend(produced);
                }
            }
            dedup(&mut next);
            items = next;
        }
        items
    }

    /// One step from a resource.
    pub fn step(&self, id: ResourceId, kind: &StepKind) -> Vec<Item> {
        let resource = self.registry.resource(id);
        match kind {
            StepKind::Ancestor(k) => {
                let kinds = self.registry.schema.expand(k);
                std::iter::once(id)
                    .chain(self.registry.ancestors(id))
                    .find(|&a| kinds.contains(&self.registry.resource(a).kind.as_str()))
                    .map(Item::Resource)
                    .into_iter()
                    .collect()
            }
            StepKind::Name(name) => match name.as_str() {
                "parent" => resource.parent.map(Item::Resource).into_iter().collect(),
                "kind" => vec![Text::plain(resource.kind.clone())],
                "name" => vec![Text::plain(self.registry.qualified_name(id))],
                "incomplete" => vec![Text::plain(self.model.incomplete.contains(&id).to_string())],
                "virtual" => vec![Text::plain(resource.is_virtual().to_string())],
                "hosts" => self.model.hosts.get(&id).into_iter().flatten().copied().map(Item::Resource).collect(),
                "failures" => self.model.failures_of(self.registry, id).iter().copied().map(Item::Resource).collect(),
                "failures(pairs)" => {
                    let mut ids: Vec<ResourceId> =
                        self.model.failure_pairs_of(self.registry, id).iter().flat_map(|&(a, b)| [a, b]).collect();
                    ids.sort();
                    ids.dedup();
                    ids.into_iter().map(Item::Resource).collect()
                }
                "failures(hosting)" => {
                    self.model.hosting_failures_of(self.registry, id).iter().copied().map(Item::Resource).collect()
                }
                "shared" => vec![Text::plain((self.model.hosted_systems(self.registry, id).len() > 1).to_string())],
                "down" => vec![Text::plain(self.down.contains(&id).to_string())],
                "owner" | "operator" => {
                    self.model.inherited(self.registry, id, name).map(Item::Resource).into_iter().collect()
                }
                key => {
                    let references = self.model.reference(Holder::Resource(id), key);
                    if !references.is_empty() {
                        return references.iter().copied().map(Item::Resource).collect();
                    }
                    match resource.attribute(key) {
                        Some(a) => value_items(&a.value),
                        None => default_value(key).map(Text::plain).into_iter().collect(),
                    }
                }
            },
        }
    }

    fn equals(&self, item: &Item, value: &Value) -> bool {
        match item {
            Item::Resource(id) => {
                let path = self.registry.path(*id);
                let segments: Vec<&str> = value.raw.split('.').collect();
                path.len() >= segments.len() && path[path.len() - segments.len()..] == segments[..]
            }
            Item::Value(text) => {
                let wanted = value_text(value);
                if text.is_port
                    && let (Some((low, high)), Ok(port)) = (port_range(&text.text), wanted.parse::<u64>())
                {
                    return low <= port && port <= high;
                }
                text.text == wanted
            }
        }
    }
}

/// Items of an attribute value: list items expanded, strings without quotes.
pub fn value_items(value: &Value) -> Vec<Item> {
    value.items().into_iter().map(|v| Text::plain(value_text(v))).collect()
}

/// Text of a value: string content, or the raw text.
pub fn value_text(value: &Value) -> String {
    match &value.kind {
        ValueKind::String(s) => s.clone(),
        _ => value.raw.clone(),
    }
}

/// Default values of standard keys (spec § 4).
pub fn default_value(key: &str) -> Option<&'static str> {
    match key {
        "confidence" => Some("confirmed"),
        "scope" => Some("internal"),
        _ => None,
    }
}

fn port_range(text: &str) -> Option<(u64, u64)> {
    match text.split_once('-') {
        Some((a, b)) => Some((a.parse().ok()?, b.parse().ok()?)),
        None => text.parse().ok().map(|p| (p, p)),
    }
}

fn compare(item: &Item, value: &Value) -> Option<std::cmp::Ordering> {
    let Item::Value(text) = item else { return None };
    compare_texts(&text.text, &value_text(value))
}

/// Whether a value has an order: a version, a number or a bandwidth.
pub fn is_ordered(text: &str) -> bool {
    semver(text).is_some() || text.parse::<f64>().is_ok() || crate::net::bandwidth(text).is_some()
}

/// Order of two values: versions, or numbers.
fn compare_texts(a: &str, b: &str) -> Option<std::cmp::Ordering> {
    if let (Some(a), Some(b)) = (semver(a), semver(b)) {
        return Some(a.cmp(&b));
    }
    if let (Some(a), Some(b)) = (crate::net::bandwidth(a), crate::net::bandwidth(b)) {
        return a.partial_cmp(&b);
    }
    let (a, b): (f64, f64) = (a.parse().ok()?, b.parse().ok()?);
    a.partial_cmp(&b)
}

/// `v1.2.3-rc.1` → (1, 2, 3), the pre-release part sorting before the release.
fn semver(text: &str) -> Option<(u64, u64, u64, bool)> {
    let text = text.strip_prefix('v').unwrap_or(text);
    let core = text.split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let (a, b, c) = (parts.next()?.parse().ok()?, parts.next()?.parse().ok()?, parts.next()?.parse().ok()?);
    if parts.next().is_some() {
        return None;
    }
    let release = !text[core.len()..].starts_with('-');
    Some((a, b, c, release))
}

fn op_holds(op: Op, ordering: std::cmp::Ordering) -> bool {
    use std::cmp::Ordering::*;
    match op {
        Op::Lt => ordering == Less,
        Op::Le => ordering != Greater,
        Op::Gt => ordering == Greater,
        Op::Ge => ordering != Less,
        Op::Eq => ordering == Equal,
        Op::Ne => ordering != Equal,
    }
}

fn dedup(items: &mut Vec<Item>) {
    let mut seen = Vec::new();
    items.retain(|i| {
        if seen.contains(i) {
            false
        } else {
            seen.push(i.clone());
            true
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semver_ordering() {
        assert!(semver("2.0.0") > semver("1.9.9"));
        assert!(semver("v2.0.0") == semver("2.0.0"));
        assert!(semver("1.0.0-rc.1") < semver("1.0.0"));
        assert_eq!(semver("16.2"), None);
    }

    #[test]
    fn port_ranges() {
        assert_eq!(port_range("8000-8100"), Some((8000, 8100)));
        assert_eq!(port_range("443"), Some((443, 443)));
    }
}
