//! Name resolution (spec § 5, Names).
//!
//! - any unambiguous suffix of the qualified path is accepted;
//! - lexical scope by system: nearest enclosing system, then its parents, then the whole project;
//! - at each level, candidates are filtered by expected kind before concluding to an ambiguity.

use std::collections::HashMap;

use crate::registry::{Reference, Registry, ResourceId};

pub struct Resolver<'r> {
    registry: &'r Registry,
    /// Last name segment → resources with that name.
    by_name: HashMap<&'r str, Vec<ResourceId>>,
    /// Host name (lowercase) → virtual IPs serving it.
    by_host: HashMap<String, Vec<ResourceId>>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Resolution {
    Resolved(ResourceId),
    /// Several candidates of the expected kind at the same scope level.
    Ambiguous(Vec<ResourceId>),
    /// The name exists, but with another kind than expected.
    WrongType(Vec<ResourceId>),
    Dangling,
}

impl<'r> Resolver<'r> {
    pub fn new(registry: &'r Registry) -> Self {
        let mut by_name: HashMap<&str, Vec<ResourceId>> = HashMap::new();
        let mut by_host: HashMap<String, Vec<ResourceId>> = HashMap::new();
        for id in registry.ids() {
            let resource = registry.resource(id);
            by_name.entry(resource.name.as_str()).or_default().push(id);
            if resource.kind == "vip"
                && let Some(hosts) = resource.attribute("host")
            {
                for host in hosts.value.items().into_iter().filter_map(|v| v.as_host()) {
                    by_host.entry(host).or_default().push(id);
                }
            }
        }
        Self { registry, by_name, by_host }
    }

    /// Resolves `reference`, written in `context` (the resource carrying the attribute or the link,
    /// `None` at the root level), expecting one of the `expected` kinds (empty: any kind).
    pub fn resolve(&self, reference: &Reference, context: Option<ResourceId>, expected: &[&str]) -> Resolution {
        let Some(last) = reference.segments.last() else {
            return Resolution::Dangling;
        };
        let matches: Vec<ResourceId> = self
            .by_name
            .get(last.as_str())
            .into_iter()
            .flatten()
            .copied()
            .filter(|&id| self.has_suffix(id, &reference.segments))
            .collect();
        if matches.is_empty() {
            return self.resolve_host(reference, expected);
        }
        let typed = |ids: &[ResourceId]| -> Vec<ResourceId> {
            ids.iter()
                .copied()
                .filter(|&id| expected.is_empty() || expected.contains(&self.registry.resource(id).kind.as_str()))
                .collect()
        };

        for scope in self.scopes(context) {
            let in_scope: Vec<ResourceId> =
                matches.iter().copied().filter(|&id| self.is_descendant(id, scope)).collect();
            let candidates = typed(&in_scope);
            match candidates.len() {
                0 => continue,
                1 => return Resolution::Resolved(candidates[0]),
                _ => return Resolution::Ambiguous(candidates),
            }
        }

        let candidates = typed(&matches);
        match candidates.len() {
            0 => Resolution::WrongType(matches),
            1 => Resolution::Resolved(candidates[0]),
            _ => Resolution::Ambiguous(candidates),
        }
    }

    /// A name designating no resource may be the host name of a virtual IP: `-> labels.example.com`.
    fn resolve_host(&self, reference: &Reference, expected: &[&str]) -> Resolution {
        let Some(hosts) = crate::net::host_name(&reference.text()).and_then(|host| self.by_host.get(&host)) else {
            return Resolution::Dangling;
        };
        if !expected.is_empty() && !expected.contains(&"vip") {
            return Resolution::WrongType(hosts.clone());
        }
        match hosts.as_slice() {
            [id] => Resolution::Resolved(*id),
            _ => Resolution::Ambiguous(hosts.clone()),
        }
    }

    /// The shortest suffix of the path of `id` that distinguishes it from the other candidates.
    pub fn minimal_suffix(&self, id: ResourceId, others: &[ResourceId]) -> String {
        let path = self.registry.path(id);
        for len in 1..=path.len() {
            let suffix = &path[path.len() - len..];
            let unique = others.iter().filter(|&&o| o != id).all(|&o| !ends_with(&self.registry.path(o), suffix));
            if unique {
                return suffix.join(".");
            }
        }
        path.join(".")
    }

    fn has_suffix(&self, id: ResourceId, segments: &[String]) -> bool {
        let path = self.registry.path(id);
        path.len() >= segments.len() && path[path.len() - segments.len()..].iter().zip(segments).all(|(a, b)| *a == b)
    }

    /// Enclosing systems, nearest first. A system is its own scope.
    fn scopes(&self, context: Option<ResourceId>) -> Vec<ResourceId> {
        let Some(context) = context else {
            return Vec::new();
        };
        std::iter::once(context)
            .chain(self.registry.ancestors(context))
            .filter(|&id| self.registry.resource(id).kind == "system")
            .collect()
    }

    fn is_descendant(&self, id: ResourceId, scope: ResourceId) -> bool {
        self.registry.ancestors(id).any(|a| a == scope)
    }
}

fn ends_with(path: &[&str], suffix: &[&str]) -> bool {
    path.len() >= suffix.len() && &path[path.len() - suffix.len()..] == suffix
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceFile;

    fn registry(text: &str) -> Registry {
        crate::project::compile(&[SourceFile { path: "a.ca".into(), text: text.into() }]).registry
    }

    fn reference(text: &str) -> Reference {
        let span = crate::source::Span { file: crate::source::FileId(0), start: 0, end: 0, line: 0, column: 0 };
        Reference { segments: text.split('.').map(str::to_owned).collect(), span }
    }

    fn find(r: &Registry, qualified: &str) -> ResourceId {
        r.ids().find(|&id| r.qualified_name(id) == qualified).unwrap()
    }

    #[test]
    fn suffixes_and_ambiguity() {
        let r = registry("site a { subnet dmz { server web01 } subnet erp { server web01 } }");
        let resolver = Resolver::new(&r);
        let ambiguous = resolver.resolve(&reference("web01"), None, &["server"]);
        let Resolution::Ambiguous(candidates) = ambiguous else { panic!("{ambiguous:?}") };
        assert_eq!(resolver.minimal_suffix(candidates[0], &candidates), "dmz.web01");
        assert_eq!(
            resolver.resolve(&reference("dmz.web01"), None, &["server"]),
            Resolution::Resolved(find(&r, "a.dmz.web01"))
        );
    }

    #[test]
    fn filtering_by_kind() {
        let r = registry("site s { subnet dmz { } }\nfirewall fw { interface dmz { } }");
        let resolver = Resolver::new(&r);
        assert_eq!(resolver.resolve(&reference("dmz"), None, &["subnet"]), Resolution::Resolved(find(&r, "s.dmz")));
        assert!(matches!(resolver.resolve(&reference("dmz"), None, &["server"]), Resolution::WrongType(_)));
        assert_eq!(resolver.resolve(&reference("nope"), None, &[]), Resolution::Dangling);
    }

    #[test]
    fn lexical_scope_by_system() {
        let r = registry(
            "system sales { app ihm { } database db { } }\n\
             system stock { app stock { } database db { } }",
        );
        let resolver = Resolver::new(&r);
        let ihm = find(&r, "sales.ihm");
        let stock = find(&r, "stock.stock");
        assert_eq!(resolver.resolve(&reference("db"), Some(ihm), &[]), Resolution::Resolved(find(&r, "sales.db")));
        assert_eq!(resolver.resolve(&reference("db"), Some(stock), &[]), Resolution::Resolved(find(&r, "stock.db")));
        assert!(matches!(resolver.resolve(&reference("db"), None, &[]), Resolution::Ambiguous(_)));
        assert_eq!(
            resolver.resolve(&reference("stock.db"), Some(ihm), &[]),
            Resolution::Resolved(find(&r, "stock.db"))
        );
    }
}
