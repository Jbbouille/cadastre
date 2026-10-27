//! The registry: resources, attributes and links, as written in `.ca` files.
//!
//! At this stage, references are still names; resolution comes next.

use crate::source::Span;

/// Index of a resource in the registry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ResourceId(pub u32);

#[derive(Clone, Debug, Default)]
pub struct Registry {
    pub resources: Vec<Resource>,
    pub links: Vec<Link>,
    /// The `kind` declarations of the project, and the schema they make with the built-in one.
    pub declarations: Vec<crate::declare::KindDecl>,
    /// The `link` and `layer` declarations: the keys of the links.
    pub link_declarations: Vec<crate::declare::LinkDecl>,
    /// The steps of the processes, as written; each process is also a resource.
    pub processes: Vec<crate::process::Process>,
    /// The `group` declarations: groups of kinds of the project.
    pub group_declarations: Vec<crate::declare::GroupDecl>,
    pub schema: crate::schema::Schema,
    /// The `select` clauses of resources (the members of a pool), with the resource.
    pub selections: Vec<(ResourceId, crate::plan::Select)>,
}

impl Registry {
    pub fn resource(&self, id: ResourceId) -> &Resource {
        &self.resources[id.0 as usize]
    }

    pub fn ids(&self) -> impl Iterator<Item = ResourceId> + '_ {
        (0..self.resources.len() as u32).map(ResourceId)
    }

    /// Full qualified path: `aws-paris.prod.dmz.web01`.
    pub fn path(&self, id: ResourceId) -> Vec<&str> {
        let mut segments = Vec::new();
        let mut current = Some(id);
        while let Some(c) = current {
            let r = self.resource(c);
            segments.push(r.name.as_str());
            current = r.parent;
        }
        segments.reverse();
        segments
    }

    pub fn qualified_name(&self, id: ResourceId) -> String {
        self.path(id).join(".")
    }

    /// Strict ancestors, from the parent up to the root.
    /// Everything a resource contains, at any depth.
    pub fn descendants(&self, id: ResourceId) -> impl Iterator<Item = ResourceId> + '_ {
        let mut queue = self.resource(id).children.clone();
        std::iter::from_fn(move || {
            let next = queue.pop()?;
            queue.extend(self.resource(next).children.iter().copied());
            Some(next)
        })
    }

    pub fn ancestors(&self, id: ResourceId) -> impl Iterator<Item = ResourceId> + '_ {
        std::iter::successors(self.resource(id).parent, |&p| self.resource(p).parent)
    }
}

#[derive(Clone, Debug)]
pub struct Resource {
    pub kind: String,
    pub kind_span: Span,
    pub name: String,
    pub name_span: Span,
    pub parent: Option<ResourceId>,
    pub children: Vec<ResourceId>,
    pub attributes: Vec<Attribute>,
    pub span: Span,
    /// `virtual server web01`: the span of `virtual`, a machine running on a host (`on`).
    pub virtual_span: Option<Span>,
}

impl Resource {
    pub fn attribute(&self, key: &str) -> Option<&Attribute> {
        self.attributes.iter().find(|a| a.key == key)
    }

    /// Declared `virtual`: a machine running on a host.
    pub fn is_virtual(&self) -> bool {
        self.virtual_span.is_some()
    }
}

#[derive(Clone, Debug)]
pub struct Attribute {
    pub key: String,
    pub key_span: Span,
    pub value: Value,
}

/// A literal value. The raw text is always kept (spec § 2, Raw text).
#[derive(Clone, Debug, PartialEq)]
pub struct Value {
    pub kind: ValueKind,
    pub raw: String,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ValueKind {
    Identifier,
    QualifiedName,
    Number,
    /// A number and its unit, without a space: `100Mbps`.
    Quantity,
    Semver,
    Ipv4,
    Cidr4,
    Ipv6,
    Cidr6,
    /// String content, quotes removed and escapes resolved.
    String(String),
    List(Vec<Value>),
}

impl Value {
    /// A value that is not there: an empty cell of a CSV file, read by a `for_each`.
    pub fn absent(span: Span) -> Self {
        Value { raw: String::new(), kind: ValueKind::String(String::new()), span }
    }

    pub fn is_absent(&self) -> bool {
        self.raw.is_empty() && matches!(&self.kind, ValueKind::String(s) if s.is_empty())
    }

    /// The value, and its items, at another place.
    pub fn with_span(self, span: Span) -> Self {
        let kind = match self.kind {
            ValueKind::List(items) => ValueKind::List(items.into_iter().map(|v| v.with_span(span)).collect()),
            kind => kind,
        };
        Value { kind, span, ..self }
    }

    /// A written reference: identifier or qualified name.
    pub fn as_reference(&self) -> Option<Reference> {
        match self.kind {
            ValueKind::Identifier | ValueKind::QualifiedName => {
                Some(Reference { segments: self.raw.split('.').map(str::to_owned).collect(), span: self.span })
            }
            _ => None,
        }
    }

    /// A host name: a bare name (`labels.example.com`) or a string, valid and in lowercase.
    pub fn as_host(&self) -> Option<String> {
        match &self.kind {
            ValueKind::Identifier | ValueKind::QualifiedName => crate::net::host_name(&self.raw),
            ValueKind::String(text) => crate::net::host_name(text),
            _ => None,
        }
    }

    /// A list nested in this list (`[[a, b], c]`), only meaningful for `stack`.
    pub fn nested_list(&self) -> Option<&Value> {
        self.items().into_iter().find(|item| matches!(item.kind, ValueKind::List(_)))
    }

    /// The items of a list, or the value itself.
    pub fn items(&self) -> Vec<&Value> {
        match &self.kind {
            ValueKind::List(items) => items.iter().collect(),
            _ => vec![self],
        }
    }
}

/// A name written in the text, possibly qualified: `db`, `sales.db`.
#[derive(Clone, Debug)]
pub struct Reference {
    pub segments: Vec<String>,
    pub span: Span,
}

impl Reference {
    pub fn text(&self) -> String {
        self.segments.join(".")
    }
}

/// Source of a link: the enclosing resource (`-> target` form inside a block)
/// or a name (full form at the root level).
#[derive(Clone, Debug)]
pub enum LinkSource {
    Enclosing(ResourceId),
    Named(Reference),
}

#[derive(Clone, Debug)]
pub struct Link {
    pub source: LinkSource,
    pub target: Reference,
    pub port: Option<PortSpec>,
    pub layers: Vec<(String, Span)>,
    pub attributes: Vec<Attribute>,
    pub span: Span,
}

impl Link {
    pub fn attribute(&self, key: &str) -> Option<&Attribute> {
        self.attributes.iter().find(|a| a.key == key)
    }
}

#[derive(Clone, Debug)]
pub struct PortSpec {
    pub protocol: String,
    /// `443` or `8000-8100`, as written.
    pub port: Option<String>,
    pub span: Span,
}

impl PortSpec {
    /// Normalized form for link identity: lowercase protocol, ports as integers.
    pub fn normalized(&self) -> String {
        let protocol = self.protocol.to_lowercase();
        match &self.port {
            None => protocol,
            Some(port) => {
                let normalized: Vec<String> = port
                    .split('-')
                    .map(|p| p.parse::<u64>().map_or_else(|_| p.to_owned(), |n| n.to_string()))
                    .collect();
                format!("{protocol}/{}", normalized.join("-"))
            }
        }
    }
}
