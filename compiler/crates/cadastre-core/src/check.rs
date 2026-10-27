//! Registry validation against the schema, reference resolution and consistency checks.
//!
//! Principle (spec § 3, Partial knowledge): an imprecise model is accepted
//! (incomplete element), a wrong model is rejected (error).

use std::collections::{BTreeSet, HashMap};

use crate::diagnostic::Diagnostic;
use crate::net;
use crate::registry::{Attribute, LinkSource, Reference, Registry, ResourceId, Value, ValueKind};
use crate::resolve::{Resolution, Resolver};
use crate::schema::{self, AttrDef, Card, Placement, Ty};
use crate::source::Span;
use crate::suggest;

#[derive(Clone, Copy, Debug, Default)]
pub struct Options {
    /// Reject attributes outside the schema (incomplete elements are handled by the host).
    pub strict: bool,
    /// The language of what is written for the readers: the site, the words of the diagrams.
    pub language: crate::i18n::Language,
}

/// Holder of an attribute.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Holder {
    Resource(ResourceId),
    Link(usize),
}

/// Validation result: resolved references, used afterwards by views.
#[derive(Debug, Default)]
pub struct Model {
    /// (holder, key) → referenced resources.
    pub references: HashMap<(Holder, String), Vec<ResourceId>>,
    /// Resolved ends of each link (same index as `Registry::links`).
    pub link_ends: Vec<Option<(ResourceId, ResourceId)>>,
    /// The steps of each process, resolved, in their order.
    pub processes: HashMap<ResourceId, Vec<crate::process::Event>>,
    /// Resources reported as incomplete.
    pub incomplete: std::collections::HashSet<ResourceId>,
    /// What each incomplete resource misses: the messages of its diagnostics.
    pub incomplete_reasons: HashMap<ResourceId, Vec<String>>,
    /// Resource → the resources whose `on` designates it (what a server, a cluster… hosts).
    pub hosts: HashMap<ResourceId, Vec<ResourceId>>,
    /// Links whose `through` is deduced from an `egress` or an `ingress`, with where it comes
    /// from (`egress of site paris-dc1`).
    pub deduced_through: HashMap<usize, Vec<String>>,
    /// Links whose `through` is deduced from the cabling: its steps, each with its variants
    /// (`col-sw10-01` or `col-sw10-02`), in order.
    pub through_steps: HashMap<usize, Vec<Vec<ResourceId>>>,
    /// The failures of each resource (`failures`), computed when a filter first asks for them.
    pub failures: std::sync::OnceLock<HashMap<ResourceId, Vec<ResourceId>>>,
    /// The same through hosting only (`failures(hosting)`): without the links.
    pub hosting_failures: std::sync::OnceLock<HashMap<ResourceId, Vec<ResourceId>>>,
    /// The pairs of failures of each resource (`failures(pairs)`), computed when first asked.
    pub failure_pairs: std::sync::OnceLock<HashMap<ResourceId, Vec<(ResourceId, ResourceId)>>>,
    /// Names written in the registry and the plans, with the resources they designate:
    /// the resolved one, or the candidates of an ambiguous or wrongly typed name.
    pub names: Vec<(Span, Vec<ResourceId>)>,
}

impl Model {
    /// Marks a resource incomplete, with the reason the diagnostic gives.
    fn incomplete(&mut self, id: ResourceId, diagnostic: &Diagnostic) {
        self.incomplete.insert(id);
        self.incomplete_reasons.entry(id).or_default().push(diagnostic.message.clone());
    }

    pub fn reference(&self, holder: Holder, key: &str) -> &[ResourceId] {
        self.references.get(&(holder, key.to_owned())).map_or(&[], Vec::as_slice)
    }

    /// The resources whose failure alone makes a resource unavailable (impact analysis).
    pub fn failures_of(&self, registry: &Registry, id: ResourceId) -> &[ResourceId] {
        self.failures.get_or_init(|| crate::impact::failures(registry, self, true)).get(&id).map_or(&[], Vec::as_slice)
    }

    /// The same through hosting only: containment, racks, `on`, cluster and pool members,
    /// connector ends; not the links nor their `through`.
    /// The pairs of resources whose failure together, and not of either alone, makes this one
    /// unavailable.
    pub fn failure_pairs_of(&self, registry: &Registry, id: ResourceId) -> &[(ResourceId, ResourceId)] {
        self.failure_pairs
            .get_or_init(|| crate::impact::failure_pairs(registry, self))
            .get(&id)
            .map_or(&[], Vec::as_slice)
    }

    pub fn hosting_failures_of(&self, registry: &Registry, id: ResourceId) -> &[ResourceId] {
        self.hosting_failures
            .get_or_init(|| crate::impact::failures(registry, self, false))
            .get(&id)
            .map_or(&[], Vec::as_slice)
    }

    /// The system a resource belongs to: the nearest enclosing system (itself included),
    /// otherwise its `system:` reference (infrastructure).
    pub fn system_of(&self, registry: &Registry, id: ResourceId) -> Option<ResourceId> {
        std::iter::once(id)
            .chain(registry.ancestors(id))
            .find(|&r| registry.resource(r).kind == "system")
            .or_else(|| self.reference(Holder::Resource(id), "system").first().copied())
    }

    /// The systems of the resources hosted by a resource (`on`).
    pub fn hosted_systems(&self, registry: &Registry, id: ResourceId) -> BTreeSet<ResourceId> {
        self.hosts.get(&id).into_iter().flatten().filter_map(|&r| self.system_of(registry, r)).collect()
    }

    /// Effective value of an inherited key (`owner`, `operator`): its own, otherwise the nearest parent's.
    pub fn inherited(&self, registry: &Registry, id: ResourceId, key: &str) -> Option<ResourceId> {
        std::iter::once(id)
            .chain(registry.ancestors(id))
            .find_map(|r| self.reference(Holder::Resource(r), key).first().copied())
    }
}

pub fn check(registry: &Registry, options: Options, diagnostics: &mut Vec<Diagnostic>) -> Model {
    let mut checker =
        Checker { registry, options, resolver: Resolver::new(registry), model: Model::default(), diagnostics };
    checker.resources();
    checker.links();
    checker.processes();
    checker.ownership();
    checker.coherence();
    checker.selections();
    checker.hosting();
    checker.egress();
    checker.cabling();
    checker.through_paths();
    checker.model
}

struct Checker<'a> {
    registry: &'a Registry,
    options: Options,
    resolver: Resolver<'a>,
    model: Model,
    diagnostics: &'a mut Vec<Diagnostic>,
}

impl Checker<'_> {
    fn resources(&mut self) {
        for id in self.registry.ids() {
            let resource = self.registry.resource(id);
            let Some(def) = self.registry.schema.kind_def(&resource.kind) else {
                let mut d = Diagnostic::error(format!("unknown resource kind: {}", resource.kind), resource.kind_span);
                if let Some(s) = suggest::closest(&resource.kind, self.registry.schema.kind_names()) {
                    d = d.with_help(format!("did you mean `{s}`?"));
                }
                self.diagnostics.push(d);
                continue;
            };
            let external = is_external(&resource.attributes);

            // `virtual`: a machine running on a host. `on` only for a virtual machine, `rack`
            // only for a physical one. A load balancer or a proxy on a host without `virtual`
            // is only warned about: it was written that way before `virtual`.
            let base = self.registry.schema.base(&resource.kind);
            let machine = crate::schema::MACHINES.contains(&base);
            match (resource.virtual_span, machine) {
                (Some(span), false) => self.diagnostics.push(
                    Diagnostic::error(
                        format!("only a machine can be virtual: {} {}", resource.kind, resource.name),
                        span,
                    )
                    .with_help(format!("virtual: {}", crate::schema::MACHINES.join(", "))),
                ),
                (Some(_), true) => {
                    if let Some(rack) = resource.attribute("rack") {
                        self.diagnostics.push(
                            Diagnostic::error(
                                format!("a virtual machine is in no rack: {} {}", resource.kind, resource.name),
                                rack.key_span,
                            )
                            .with_help("its host is given by `on:`, the rack is that of the host"),
                        );
                    }
                }
                (None, true) => {
                    if let Some(on) = resource.attribute("on") {
                        let message =
                            format!("only a virtual machine runs on a host: {} {}", resource.kind, resource.name);
                        let help = format!("declare it `virtual {} {}`", resource.kind, resource.name);
                        self.diagnostics.push(if matches!(base, "load_balancer" | "proxy") {
                            Diagnostic::warning(message, on.key_span).with_help(help)
                        } else {
                            Diagnostic::error(message, on.key_span).with_help(help)
                        });
                    }
                }
                (None, false) => {}
            }

            // Placement.
            let parent_kind = resource.parent.map(|p| self.registry.resource(p).kind.as_str());
            match self.registry.schema.placement(def, parent_kind) {
                Placement::Exact => {}
                Placement::Approximate if !external => {
                    let place = parent_kind.map_or("at the root".to_owned(), |k| format!("in a {k}"));
                    let diagnostic = Diagnostic::incomplete(
                        format!(
                            "approximate placement: {} {} {place} (expected parent: {})",
                            def.name,
                            resource.name,
                            parents_text(def.parents)
                        ),
                        resource.name_span,
                    );
                    self.model.incomplete(id, &diagnostic);
                    self.diagnostics.push(diagnostic);
                }
                Placement::Approximate => {}
                Placement::Impossible => {
                    self.diagnostics.push(
                        Diagnostic::error(
                            format!(
                                "{} {} cannot be placed in {} {}",
                                def.name,
                                resource.name,
                                parent_kind.unwrap_or("?"),
                                resource.parent.map_or("", |p| self.registry.resource(p).name.as_str())
                            ),
                            resource.name_span,
                        )
                        .with_help(format!(
                            "parent of {}: {}",
                            def.name,
                            parents_text(def.parents)
                        )),
                    );
                }
            }

            // Attributes.
            self.duplicate_keys(&resource.attributes);
            // `parent:` is read by the placement, before the checks (`place.rs`).
            for attribute in resource.attributes.iter().filter(|a| a.key != "parent") {
                match self.registry.schema.resource_attr(def, &attribute.key) {
                    Some(attr_def) => {
                        let resolved = self.value(attr_def, &attribute.value, Some(id));
                        if !resolved.is_empty() {
                            self.model.references.insert((Holder::Resource(id), attribute.key.clone()), resolved);
                        }
                    }
                    None => {
                        let known = def.attrs.iter().chain(schema::RESOURCE_KEYS).map(|a| a.key);
                        self.free_key(attribute, known);
                    }
                }
            }

            // A schedule: a valid cron expression; free text is to be converted (incomplete).
            if let Some(schedule) = resource.attribute("schedule") {
                match crate::schedule::parse(&crate::query::value_text(&schedule.value)) {
                    Err(message) => self.diagnostics.push(
                        Diagnostic::error(format!("invalid schedule: {message}"), schedule.value.span)
                            .with_help("a cron expression: minute hour day-of-month month day-of-week, \"0 3 * * *\" every day at 03:00"),
                    ),
                    Ok(crate::schedule::Schedule::Text(text)) if !external => {
                        let diagnostic = Diagnostic::incomplete(
                                format!("{} {}: schedule in free text, \"{text}\"", resource.kind, self.registry.qualified_name(id)),
                                schedule.value.span,
                            )
                            .with_help("write it as a cron expression, checked and shown in plain words: \"0 3 * * *\"");
                        self.model.incomplete(id, &diagnostic);
                        self.diagnostics.push(diagnostic);
                    }
                    Ok(_) => {}
                }
            }

            // A virtual IP is known by its address or its host names.
            if resource.kind == "vip"
                && !external
                && resource.attribute("ip").is_none()
                && resource.attribute("host").is_none()
            {
                let diagnostic = Diagnostic::incomplete(
                    format!("vip {}: neither ip nor host", self.registry.qualified_name(id)),
                    resource.name_span,
                );
                self.model.incomplete(id, &diagnostic);
                self.diagnostics.push(diagnostic);
            }

            // Missing required attributes.
            if !external {
                for attr_def in def.attrs.iter().filter(|a| a.required) {
                    if resource.attribute(attr_def.key).is_none() {
                        let diagnostic = Diagnostic::incomplete(
                            format!(
                                "{} {}: missing attribute {}",
                                def.name,
                                self.registry.qualified_name(id),
                                attr_def.key
                            ),
                            resource.name_span,
                        );
                        self.model.incomplete(id, &diagnostic);
                        self.diagnostics.push(diagnostic);
                    }
                }
            }
        }
    }

    fn links(&mut self) {
        let flow_endpoint = self.registry.schema.expand("flow_endpoint");
        // Identity of a link: its ends, its port, its channels (a pull on a topic and a push on
        // another one, to the same broker, are two links).
        let mut identities: HashMap<(ResourceId, ResourceId, String, Vec<ResourceId>), usize> = HashMap::new();

        for (index, link) in self.registry.links.iter().enumerate() {
            let (source, context) = match &link.source {
                LinkSource::Enclosing(id) => {
                    let kind = self.registry.resource(*id).kind.as_str();
                    if self.registry.schema.kind_def(kind).is_some() && !flow_endpoint.contains(&kind) {
                        self.diagnostics
                            .push(Diagnostic::error(format!("a flow cannot start from a {kind}"), link.span));
                    }
                    (Some(*id), Some(*id))
                }
                LinkSource::Named(reference) => {
                    (self.reference(reference, None, &flow_endpoint, "flow_endpoint"), None)
                }
            };
            let target = self.reference(&link.target, context, &flow_endpoint, "flow_endpoint");

            self.duplicate_keys(&link.attributes);
            let layers: Vec<&str> = link.layers.iter().map(|(name, _)| name.as_str()).collect();
            for attribute in &link.attributes {
                match self.registry.schema.link_attr(&attribute.key, &layers) {
                    Some(attr_def) => {
                        let resolved = self.value(attr_def, &attribute.value, context);
                        if !resolved.is_empty() {
                            self.model.references.insert((Holder::Link(index), attribute.key.clone()), resolved);
                        }
                    }
                    None => {
                        let declared = self.registry.schema.link_keys(&layers).map(|a| a.key);
                        self.free_key(attribute, schema::LINK_KEYS.iter().map(|a| a.key).chain(declared))
                    }
                }
            }

            let ends = source.zip(target);
            self.model.link_ends.push(ends);
            let Some((source, target)) = ends else { continue };

            // A key the schema of the project requires: the link is incomplete without it.
            for def in self.registry.schema.link_keys(&layers).filter(|d| d.required) {
                if link.attribute(def.key).is_none() {
                    let (from, to) = (self.registry.qualified_name(source), self.registry.qualified_name(target));
                    self.diagnostics.push(Diagnostic::incomplete(
                        format!("link {from} -> {to}: missing attribute {}", def.key),
                        link.span,
                    ));
                }
            }

            // A channel belongs to the source or the target of the link.
            for &channel in self.model.reference(Holder::Link(index), "channel") {
                let host = self.registry.resource(channel).parent;
                if host != Some(source) && host != Some(target) {
                    self.diagnostics.push(Diagnostic::error(
                        format!(
                            "channel {} belongs neither to {} nor to {}",
                            self.registry.qualified_name(channel),
                            self.registry.resource(source).name,
                            self.registry.resource(target).name
                        ),
                        link.attribute("channel").map_or(link.span, |a| a.value.span),
                    ));
                }
            }

            // Identity: (source, target, normalized port, channels).
            let port = link.port.as_ref().map(|p| p.normalized()).unwrap_or_default();
            let mut channels = self.model.reference(Holder::Link(index), "channel").to_vec();
            channels.sort();
            if let Some(&first) = identities.get(&(source, target, port.clone(), channels.clone())) {
                self.diagnostics.push(
                    Diagnostic::error(
                        format!(
                            "link declared twice: {} -> {}{}",
                            self.registry.qualified_name(source),
                            self.registry.qualified_name(target),
                            if port.is_empty() { String::new() } else { format!(" : {port}") }
                        ),
                        link.span,
                    )
                    .with_span(self.registry.links[first].span)
                    .with_help("declare the link once, with all its layers"),
                );
            } else {
                identities.insert((source, target, port, channels), index);
            }
        }

        // Link without a port, redundant with a link with a port.
        for (&(source, target, ref port, ref channels), &index) in &identities {
            let with_port = |(s, t, p, c): &(ResourceId, ResourceId, String, Vec<ResourceId>)| {
                *s == source && *t == target && c == channels && !p.is_empty()
            };
            if port.is_empty() && identities.keys().any(with_port) {
                self.diagnostics.push(Diagnostic::warning(
                    format!(
                        "redundant link without port: {} -> {} is also declared with a port",
                        self.registry.resource(source).name,
                        self.registry.resource(target).name
                    ),
                    self.registry.links[index].span,
                ));
            }
        }
    }

    /// `owner` required after inheritance (spec § 3, Teams and responsibilities).
    /// `egress` and `ingress`: a link towards the internet without a written `through` goes
    /// through the `egress` of its source, a link from the internet through the `ingress` of its
    /// target; the nearest one counts, by containment, then by `on`.
    fn egress(&mut self) {
        for (index, link) in self.registry.links.iter().enumerate() {
            if link.attribute("through").is_some() {
                continue;
            }
            let Some((source, target)) = self.model.link_ends[index] else { continue };
            let (external_source, external_target) = (self.external(source), self.external(target));
            let mut path = Vec::new();
            let mut origins = Vec::new();
            for (end, key) in [(source, "egress"), (target, "ingress")] {
                let applies = if key == "egress" {
                    external_target && !external_source
                } else {
                    external_source && !external_target
                };
                if let Some((holder, items)) = applies.then(|| self.nearest(end, key, 0)).flatten() {
                    path.extend(items);
                    let resource = self.registry.resource(holder);
                    origins.push(format!("{key} of {} {}", resource.kind, self.registry.qualified_name(holder)));
                }
            }
            if !path.is_empty() {
                self.model.references.insert((Holder::Link(index), "through".to_owned()), path);
                self.model.deduced_through.insert(index, origins);
            }
        }
    }

    /// `through` deduced from the cabling (`peer`), the subnets and their gateways: for a flow
    /// without a written `through`, the devices between its ends; before the `egress` of a flow
    /// to the internet, the way to it; after the `ingress` of a flow from the internet, the way
    /// from it. Only a complete path is kept, silently otherwise. A written `through` naming a
    /// switch, a router or a firewall off the path of the cabling is warned about.
    fn cabling(&mut self) {
        let network = crate::path::Network::new(self.registry, &self.model);
        let mut deduced: Vec<(usize, Vec<Vec<ResourceId>>, String)> = Vec::new();
        let mut warnings = Vec::new();
        for (index, link) in self.registry.links.iter().enumerate() {
            let Some((source, target)) = self.model.link_ends[index] else { continue };
            if let Some(attribute) = link.attribute("through") {
                let Ok(path) = network.between(source, target) else { continue };
                let on_path = path.devices();
                for &item in self.model.reference(Holder::Link(index), "through") {
                    let base = self.registry.schema.base(&self.registry.resource(item).kind);
                    if matches!(base, "switch" | "router" | "firewall")
                        && !on_path.contains(&item)
                        && !path.steps.is_empty()
                    {
                        warnings.push(
                            Diagnostic::warning(
                                format!(
                                    "{} is not on the path of the cabling between {} and {}",
                                    self.registry.qualified_name(item),
                                    self.registry.qualified_name(source),
                                    self.registry.qualified_name(target)
                                ),
                                attribute.value.span,
                            )
                            .with_help(format!(
                                "the cabling goes through {}",
                                path.devices()
                                    .iter()
                                    .map(|&d| self.registry.qualified_name(d))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            )),
                        );
                    }
                }
                continue;
            }
            match self.model.deduced_through.get(&index) {
                // To or from the internet: the way to the egress, or from the ingress.
                Some(origins) => {
                    let items = self.model.reference(Holder::Link(index), "through").to_vec();
                    let (Some(&first), Some(&last)) = (items.first(), items.last()) else { continue };
                    let egress = origins.iter().any(|o| o.starts_with("egress"));
                    let ingress = origins.iter().any(|o| o.starts_with("ingress"));
                    let mut steps: Vec<Vec<ResourceId>> = Vec::new();
                    // The way up to the device of the egress, without that device (its first item).
                    if egress && let Ok(way) = network.towards(source, first) {
                        steps.extend(way.steps.into_iter().filter(|s| !s.contains(&first)));
                    }
                    steps.extend(items.iter().map(|&i| vec![i]));
                    if ingress && let Ok(way) = network.towards(target, last) {
                        steps.extend(way.steps.into_iter().rev().filter(|s| !s.contains(&last)));
                    }
                    if steps.len() > items.len() {
                        deduced.push((index, steps, "cabling".to_owned()));
                    }
                }
                None => {
                    if let Ok(path) = network.between(source, target)
                        && !path.steps.is_empty()
                    {
                        deduced.push((index, path.steps, "cabling".to_owned()));
                    }
                }
            }
        }
        self.diagnostics.extend(warnings);
        for (index, steps, origin) in deduced {
            let mut flat: Vec<ResourceId> = Vec::new();
            for &d in steps.iter().flatten() {
                if !flat.contains(&d) {
                    flat.push(d);
                }
            }
            self.model.references.insert((Holder::Link(index), "through".to_owned()), flat);
            self.model.deduced_through.entry(index).or_default().push(origin);
            self.model.through_steps.insert(index, steps);
        }
    }

    /// A device of a written `through` lies in the site of one end of the flow; a tunnel or a
    /// connector has an end there. Only what is known is compared: an end or a device without a
    /// site is not checked.
    fn through_paths(&mut self) {
        for (index, link) in self.registry.links.iter().enumerate() {
            let Some(attribute) = link.attribute("through") else { continue };
            let Some((source, target)) = self.model.link_ends[index] else { continue };
            let ends: BTreeSet<ResourceId> = self.sites(source, 0).into_iter().chain(self.sites(target, 0)).collect();
            if ends.is_empty() {
                continue;
            }
            for &item in self.model.reference(Holder::Link(index), "through") {
                let resource = self.registry.resource(item);
                let sites: BTreeSet<ResourceId> =
                    match self.registry.schema.kind_def(&resource.kind).and_then(|d| d.connector) {
                        Some(key) => self
                            .model
                            .reference(Holder::Resource(item), key)
                            .iter()
                            .flat_map(|&e| self.sites(e, 0))
                            .collect(),
                        None => self.sites(item, 0),
                    };
                if !sites.is_empty() && sites.is_disjoint(&ends) {
                    let names = |set: &BTreeSet<ResourceId>| {
                        set.iter().map(|&s| self.registry.resource(s).name.as_str()).collect::<Vec<_>>().join(", ")
                    };
                    self.diagnostics.push(
                        Diagnostic::warning(
                            format!(
                                "{} {} is in {}, where neither end of the flow is ({})",
                                resource.kind,
                                resource.name,
                                names(&sites),
                                names(&ends)
                            ),
                            attribute.value.span,
                        )
                        .with_help("a traversed device is on the way: in the site of the source or of the target"),
                    );
                }
            }
        }
    }

    /// The sites a resource is in: its own, otherwise those of what it runs on (`on`).
    fn sites(&self, id: ResourceId, depth: usize) -> BTreeSet<ResourceId> {
        let own =
            std::iter::once(id).chain(self.registry.ancestors(id)).find(|&a| self.registry.resource(a).kind == "site");
        if let Some(site) = own {
            return BTreeSet::from([site]);
        }
        if depth > 4 {
            return BTreeSet::new();
        }
        self.model.reference(Holder::Resource(id), "on").iter().flat_map(|&host| self.sites(host, depth + 1)).collect()
    }

    /// Whether a resource is reached over the internet: its nearest `reach`, otherwise a SaaS or
    /// the internet. `scope: external` (outside the perimeter) does not say it: a directory run by
    /// another team is external, and internal to the network.
    fn external(&self, id: ResourceId) -> bool {
        crate::path::over_internet(self.registry, &self.model, id)
    }

    fn nearest(&self, id: ResourceId, key: &str, _depth: usize) -> Option<(ResourceId, Vec<ResourceId>)> {
        crate::path::nearest(self.registry, &self.model, id, key)
    }

    /// Two subnets of a site whose CIDRs overlap: an address would be in both. Sites may reuse
    /// the same private ranges: only the subnets of a site are compared.
    fn overlapping_subnets(&mut self) {
        let mut by_site: HashMap<Option<ResourceId>, Vec<(ResourceId, net::Cidr, Span)>> = HashMap::new();
        for id in self.registry.ids() {
            let resource = self.registry.resource(id);
            if resource.kind != "subnet" {
                continue;
            }
            let Some(attribute) = resource.attribute("cidr") else { continue };
            let Some(cidr) = net::parse_cidr(&attribute.value.raw) else { continue };
            let site = self.registry.ancestors(id).find(|&a| self.registry.resource(a).kind == "site");
            by_site.entry(site).or_default().push((id, cidr, attribute.value.span));
        }
        let mut sites: Vec<_> = by_site.into_values().collect();
        sites.sort_by_key(|subnets| subnets[0].0);
        for subnets in sites {
            for (i, (a, cidr_a, span_a)) in subnets.iter().enumerate() {
                for (b, cidr_b, span_b) in &subnets[i + 1..] {
                    if cidr_a.overlaps(cidr_b) {
                        let name = |id: ResourceId| self.registry.qualified_name(id);
                        let raw = |id: ResourceId| {
                            self.registry.resource(id).attribute("cidr").map_or(String::new(), |c| c.value.raw.clone())
                        };
                        self.diagnostics.push(
                            Diagnostic::error(
                                format!("subnets {} ({}) and {} ({}) overlap", name(*a), raw(*a), name(*b), raw(*b)),
                                *span_b,
                            )
                            .with_span(*span_a),
                        );
                    }
                }
            }
        }
    }

    fn ownership(&mut self) {
        for id in self.registry.ids() {
            let resource = self.registry.resource(id);
            let Some(def) = self.registry.schema.kind_def(&resource.kind) else { continue };
            if def.owner_required
                && !is_external(&resource.attributes)
                && self.model.inherited(self.registry, id, "owner").is_none()
            {
                let diagnostic = Diagnostic::incomplete(
                    format!(
                        "{} {}: no owner, neither written nor inherited",
                        def.name,
                        self.registry.qualified_name(id)
                    ),
                    resource.name_span,
                );
                self.model.incomplete(id, &diagnostic);
                self.diagnostics.push(diagnostic);
            }
        }
    }

    fn coherence(&mut self) {
        self.virtual_ips();
        self.overlapping_subnets();
        let mut ips: HashMap<(ResourceId, net::Ip), Span> = HashMap::new();
        let mut peers: HashMap<ResourceId, BTreeSet<ResourceId>> = HashMap::new();

        for id in self.registry.ids() {
            let resource = self.registry.resource(id);
            let Some(def) = self.registry.schema.kind_def(&resource.kind) else { continue };

            // Distinct connector ends.
            if let Some(attr) = def.connector {
                let ends = self.model.reference(Holder::Resource(id), attr);
                if ends.len() == 2 && ends[0] == ends[1] {
                    self.diagnostics.push(Diagnostic::error(
                        format!("both ends of {} are the same", resource.name),
                        resource.attribute(attr).map_or(resource.name_span, |a| a.value.span),
                    ));
                }
            }

            // IP within the CIDR of its subnet, and unique in that subnet.
            if let Some(ip_attr) = resource.attribute("ip") {
                let subnet = match resource.kind.as_str() {
                    "server" | "device" => resource.parent.filter(|&p| self.registry.resource(p).kind == "subnet"),
                    "interface" | "vip" => self.model.reference(Holder::Resource(id), "subnet").first().copied(),
                    _ => None,
                };
                let ip = net::parse_ip(&ip_attr.value.raw);
                let cidr = subnet
                    .and_then(|s| self.registry.resource(s).attribute("cidr"))
                    .and_then(|c| net::parse_cidr(&c.value.raw));
                if let (Some(subnet), Some(ip)) = (subnet, ip) {
                    if cidr.is_some_and(|c| !c.contains(ip)) {
                        self.diagnostics.push(Diagnostic::error(
                            format!(
                                "{} is outside the CIDR of subnet {}",
                                ip_attr.value.raw,
                                self.registry.resource(subnet).name
                            ),
                            ip_attr.value.span,
                        ));
                    }
                    match ips.get(&(subnet, ip)) {
                        Some(&first) => self.diagnostics.push(
                            Diagnostic::error(
                                format!(
                                    "{} is used twice in subnet {}",
                                    ip_attr.value.raw,
                                    self.registry.resource(subnet).name
                                ),
                                ip_attr.value.span,
                            )
                            .with_span(first),
                        ),
                        None => {
                            ips.insert((subnet, ip), ip_attr.value.span);
                        }
                    }
                }
            }

            // Rack in the same site as the resource.
            if let Some(&rack) = self.model.reference(Holder::Resource(id), "rack").first() {
                let own_site = self.site_of(id);
                let rack_site = self.site_of(rack);
                if let (Some(a), Some(b)) = (own_site, rack_site)
                    && a != b
                {
                    self.diagnostics.push(Diagnostic::error(
                        format!(
                            "{} is in site {} but its rack {} is in {}",
                            resource.name,
                            self.registry.resource(a).name,
                            self.registry.resource(rack).name,
                            self.registry.resource(b).name
                        ),
                        resource.attribute("rack").map_or(resource.name_span, |r| r.value.span),
                    ));
                }
            }

            // Cables: a plug point has a single peer.
            if let Some(&peer) = self.model.reference(Holder::Resource(id), "peer").first() {
                peers.entry(id).or_default().insert(peer);
                peers.entry(peer).or_default().insert(id);
            }
        }

        let mut contradictions: Vec<_> = peers.into_iter().filter(|(_, p)| p.len() > 1).collect();
        contradictions.sort();
        for (end, partners) in contradictions {
            let names: Vec<String> = partners.iter().map(|&p| self.registry.qualified_name(p)).collect();
            self.diagnostics.push(Diagnostic::error(
                format!("{} is cabled to several peers: {}", self.registry.qualified_name(end), names.join(", ")),
                self.registry.resource(end).name_span,
            ));
        }
    }

    fn site_of(&self, id: ResourceId) -> Option<ResourceId> {
        std::iter::once(id).chain(self.registry.ancestors(id)).find(|&r| self.registry.resource(r).kind == "site")
    }

    /// Checks the value of a known attribute; returns the resolved references.
    fn value(&mut self, def: &AttrDef, value: &Value, context: Option<ResourceId>) -> Vec<ResourceId> {
        let items = value.items();
        let is_list = matches!(value.kind, ValueKind::List(_));

        if matches!(def.ty, Ty::Strings) {
            for item in &items {
                if !matches!(item.kind, ValueKind::String(_)) {
                    self.diagnostics.push(Diagnostic::error(
                        format!("{} expects a string or a list of strings", def.key),
                        item.span,
                    ));
                }
            }
            return Vec::new();
        }

        let card_ok = match def.card {
            Card::One => !is_list,
            Card::OneOrMore => !items.is_empty(),
            Card::Many => true,
            Card::Exactly(n) => items.len() == n,
        };
        if !card_ok {
            let expected = match def.card {
                Card::One => "a single value".to_owned(),
                Card::OneOrMore => "at least one value".to_owned(),
                Card::Many => unreachable!(),
                Card::Exactly(n) => format!("exactly {n} values"),
            };
            self.diagnostics
                .push(Diagnostic::error(format!("{} expects {expected}, {} given", def.key, items.len()), value.span));
        }

        let mut resolved = Vec::new();
        for item in items {
            let ok = match def.ty {
                Ty::Symbol(values) => {
                    if matches!(item.kind, ValueKind::Identifier) && values.contains(&item.raw.as_str()) {
                        true
                    } else {
                        let mut d =
                            Diagnostic::error(format!("invalid value for {}: {}", def.key, item.raw), item.span);
                        d = match suggest::closest(&item.raw, values.iter().copied()) {
                            Some(s) => {
                                d.with_help(format!("did you mean `{s}`? possible values: {}", values.join(", ")))
                            }
                            None => d.with_help(format!("possible values: {}", values.join(", "))),
                        };
                        self.diagnostics.push(d);
                        continue;
                    }
                }
                Ty::String => matches!(item.kind, ValueKind::String(_)),
                Ty::Number => matches!(item.kind, ValueKind::Number),
                Ty::Semver => matches!(item.kind, ValueKind::Semver),
                Ty::Version => matches!(item.kind, ValueKind::Semver | ValueKind::Number | ValueKind::String(_)),
                Ty::Ip => matches!(item.kind, ValueKind::Ipv4 | ValueKind::Ipv6),
                Ty::Cidr => matches!(item.kind, ValueKind::Cidr4 | ValueKind::Cidr6),
                Ty::Host => item.as_host().is_some(),
                Ty::Pattern(pattern) => {
                    if !matches!(&item.kind, ValueKind::String(text) if pattern.regex.is_match(text)) {
                        self.diagnostics.push(
                            Diagnostic::error(format!("invalid value for {}: {}", def.key, item.raw), item.span)
                                .with_help(format!("a string matching \"{}\" as a whole", pattern.source)),
                        );
                    }
                    continue;
                }
                Ty::Url => match &item.kind {
                    ValueKind::String(text) => match crate::net::check_url(text) {
                        Ok(()) => true,
                        Err(crate::net::UrlError::Malformed) => false,
                        Err(error) => {
                            let what = match error {
                                crate::net::UrlError::Secret(name) => format!("a secret (`{name}`)"),
                                _ => "a password".to_owned(),
                            };
                            self.diagnostics.push(
                                Diagnostic::error(format!("{} holds {what}: {}", def.key, item.raw), item.span)
                                    .with_help("the registry is versioned and read by everyone: keep secrets in a vault, a user name at most in the URL"),
                            );
                            continue;
                        }
                    },
                    _ => false,
                },
                Ty::Bandwidth => match &item.kind {
                    ValueKind::Quantity | ValueKind::String(_) => {
                        crate::net::bandwidth(&crate::query::value_text(item)).is_some()
                    }
                    _ => false,
                },
                Ty::Spdx => match &item.kind {
                    ValueKind::String(text) => match crate::spdx::check(text) {
                        Ok(()) => true,
                        Err(error) => {
                            let mut d = Diagnostic::error(format!("{}: {error}", def.key), item.span);
                            if let Some(suggestion) = error.suggestion() {
                                d = d.with_help(format!("did you mean `{suggestion}`?"));
                            } else {
                                d = d.with_help("an SPDX expression: `\"MIT\"`, `\"MIT OR Apache-2.0\"`, `\"LicenseRef-<name>\"` for another license");
                            }
                            self.diagnostics.push(d);
                            continue;
                        }
                    },
                    _ => false,
                },
                Ty::Ref(union) => match item.as_reference() {
                    Some(reference) => {
                        let expected = self.registry.schema.expand(union);
                        if let Some(id) = self.reference(&reference, context, &expected, union) {
                            resolved.push(id);
                        }
                        true
                    }
                    None => false,
                },
                Ty::Strings => unreachable!(),
            };
            if !ok {
                self.diagnostics.push(Diagnostic::error(
                    format!("invalid value for {}: {} ({} expected)", def.key, item.raw, ty_text(def.ty)),
                    item.span,
                ));
            }
        }
        resolved
    }

    /// Resolves a reference and reports failures.
    /// The steps of the processes: each a declared link (the other way for a reply), or a
    /// processing within a resource.
    fn processes(&mut self) {
        for process in &self.registry.processes {
            for &participant in &self.registry.resource(process.id).children {
                let resource = self.registry.resource(participant);
                if resource.kind == "participant" && self.members(participant).is_empty() {
                    self.diagnostics.push(
                        Diagnostic::error(
                            format!("participant {} stands for no resource", resource.name),
                            resource.span,
                        )
                        .with_help("its members: ends of flows, or clusters standing for their servers"),
                    );
                }
            }
            let events = self.process_items(process.id, &process.items);
            self.model.processes.insert(process.id, events);
        }
    }

    /// The resources an end of a step stands for: itself, or the members of a participant of the
    /// process, a cluster standing for its servers.
    fn members(&self, id: ResourceId) -> Vec<ResourceId> {
        if self.registry.resource(id).kind != "participant" {
            return vec![id];
        }
        let mut members = Vec::new();
        for &member in self.model.reference(Holder::Resource(id), "members") {
            let servers: Vec<ResourceId> = if self.registry.resource(member).kind == "cluster" {
                self.registry
                    .ids()
                    .filter(|&r| self.model.reference(Holder::Resource(r), "cluster").contains(&member))
                    .collect()
            } else {
                vec![member]
            };
            for server in servers {
                if !members.contains(&server) {
                    members.push(server);
                }
            }
        }
        members
    }

    fn process_items(&mut self, process: ResourceId, items: &[crate::process::Item]) -> Vec<crate::process::Event> {
        use crate::process::{Event, FLAGS, Item, ResolvedStep, STEP_KEYS};
        // The ends of a step: an end of a flow, or a participant of the process.
        let endpoint = self.registry.schema.expand("flow_endpoint|participant");
        let mut events = Vec::new();
        for item in items {
            let step = match item {
                Item::Loop { label, items, .. } => {
                    events.push(Event::LoopStart(label.clone()));
                    events.extend(self.process_items(process, items));
                    events.push(Event::LoopEnd);
                    continue;
                }
                Item::Step(step) => step,
            };
            let link = &step.link;
            let crate::registry::LinkSource::Named(source_name) = &link.source else { continue };
            for (flag, span) in step.flags.iter().filter(|(f, _)| !FLAGS.iter().any(|(k, _)| k == f)) {
                self.diagnostics.push(
                    Diagnostic::error(format!("unknown flag of a step: {flag}"), *span)
                        .with_help(format!("flags: {}", FLAGS.iter().map(|(k, _)| *k).collect::<Vec<_>>().join(", "))),
                );
            }
            let text_of = |key: &str| {
                link.attribute(key).and_then(|a| match &a.value.kind {
                    crate::registry::ValueKind::String(text) => Some(text.clone()),
                    _ => None,
                })
            };
            for attribute in link.attributes.iter().filter(|a| !STEP_KEYS.iter().any(|(k, _)| *k == a.key)) {
                self.diagnostics.push(
                    Diagnostic::error(format!("unknown key of a step: {}", attribute.key), attribute.key_span)
                        .with_help("label, note: the other keys belong to the link of the step"),
                );
            }
            let has = |flag: &str| step.flags.iter().any(|(f, _)| f == flag);
            let (reply, asynchronous) = (has("reply"), has("async"));
            let Some(source) = self.reference(source_name, Some(process), &endpoint, "flow_endpoint") else {
                continue;
            };
            let mut resolved = ResolvedStep {
                source,
                target: None,
                links: Vec::new(),
                reply,
                asynchronous,
                label: text_of("label"),
                note: text_of("note"),
                text: step.text.clone(),
                span: link.span,
            };
            if step.text.is_some() {
                events.push(Event::Step(resolved));
                continue;
            }
            let Some(target) = self.reference(&link.target, Some(process), &endpoint, "flow_endpoint") else {
                continue;
            };
            resolved.target = Some(target);
            // The links of the step: declared from the source to the target, the other way for a
            // reply, for each member of a participant standing for several resources; their port
            // and their layers when written.
            let (from, to) = if reply { (target, source) } else { (source, target) };
            let port = link.port.as_ref().map(|p| p.normalized());
            let (mut missing, mut ambiguous) = (Vec::new(), Vec::new());
            for from in self.members(from) {
                for to in self.members(to) {
                    let candidates: Vec<usize> = (0..self.registry.links.len())
                        .filter(|&i| self.model.link_ends.get(i).copied().flatten() == Some((from, to)))
                        .filter(|&i| {
                            let declared = &self.registry.links[i];
                            port.as_ref()
                                .is_none_or(|p| declared.port.as_ref().map(|d| d.normalized()).as_ref() == Some(p))
                                && link.layers.iter().all(|(l, _)| declared.layers.iter().any(|(d, _)| d == l))
                        })
                        .collect();
                    let written = format!(
                        "{} -> {}{}",
                        self.registry.qualified_name(from),
                        self.registry.qualified_name(to),
                        port.as_ref().map(|p| format!(" : {p}")).unwrap_or_default()
                    );
                    match candidates.as_slice() {
                        [index] => resolved.links.push(*index),
                        [] => missing.push(written),
                        _ => ambiguous.push(written),
                    }
                }
            }
            // A participant standing for several resources: what it stands for, in the help.
            let standing = [from, to]
                .into_iter()
                .filter(|&id| self.registry.resource(id).kind == "participant")
                .map(|id| {
                    let members: Vec<String> =
                        self.members(id).into_iter().map(|m| self.registry.qualified_name(m)).collect();
                    format!("{} stands for {}", self.registry.resource(id).name, members.join(", "))
                })
                .collect::<Vec<_>>()
                .join("; ");
            if !missing.is_empty() {
                let help = if reply {
                    "a reply is the answer to a call: the link declared from the target to the source"
                } else {
                    "a step is a link of the registry: declare it, or mark the answer to a call `{ reply }`"
                };
                let help = if standing.is_empty() { help.to_owned() } else { format!("{standing}; {help}") };
                let message = match missing.as_slice() {
                    [one] => format!("no link {one} for this step"),
                    _ => format!("no links {} for this step", missing.join(", ")),
                };
                self.diagnostics.push(Diagnostic::error(message, link.span).with_help(help));
            }
            if !ambiguous.is_empty() {
                self.diagnostics.push(
                    Diagnostic::error(format!("several links {} for this step", ambiguous.join(", ")), link.span)
                        .with_help("give the port or the layer of the step: `step a -> b : https [flow]`"),
                );
            }
            if !missing.is_empty() || !ambiguous.is_empty() {
                continue;
            }
            events.push(Event::Step(resolved));
        }
        events
    }

    fn reference(
        &mut self,
        reference: &Reference,
        context: Option<ResourceId>,
        expected: &[&str],
        expected_text: &str,
    ) -> Option<ResourceId> {
        let expected_text = expected_text.replace('|', " or ");
        let resolution = self.resolver.resolve(reference, context, expected);
        record_name(&mut self.model.names, reference.span, &resolution);
        match resolution {
            Resolution::Resolved(id) => Some(id),
            Resolution::Dangling => {
                let name = reference.text();
                let mut d = Diagnostic::error(format!("dangling reference: {name}"), reference.span);
                let names: Vec<&str> = self
                    .registry
                    .resources
                    .iter()
                    .filter(|r| expected.contains(&r.kind.as_str()))
                    .map(|r| r.name.as_str())
                    .collect();
                if let Some(s) = suggest::closest(reference.segments.last().map_or("", String::as_str), names) {
                    d = d.with_help(format!("did you mean `{s}`?"));
                }
                self.diagnostics.push(d);
                None
            }
            Resolution::WrongType(found) => {
                let kinds: BTreeSet<&str> = found.iter().map(|&f| self.registry.resource(f).kind.as_str()).collect();
                let kinds: Vec<&str> = kinds.into_iter().collect();
                self.diagnostics.push(Diagnostic::error(
                    format!("{} is a {}, {expected_text} expected", reference.text(), kinds.join(" / ")),
                    reference.span,
                ));
                None
            }
            Resolution::Ambiguous(candidates) => {
                let names: Vec<String> = candidates.iter().map(|&c| self.registry.qualified_name(c)).collect();
                let hint = self.resolver.minimal_suffix(candidates[0], &candidates);
                self.diagnostics.push(
                    Diagnostic::error(
                        format!("ambiguous name: {} ({expected_text} expected)", reference.text()),
                        reference.span,
                    )
                    .with_help(format!(
                        "candidates: {}; use a more qualified name, for instance {hint}",
                        names.join(", ")
                    )),
                );
                None
            }
        }
    }

    /// Key outside the schema: accepted, warning if close to a known key, error in strict mode.
    fn free_key<'k>(&mut self, attribute: &Attribute, known: impl IntoIterator<Item = &'k str>) {
        let suggestion = suggest::closest(&attribute.key, known);
        if self.options.strict {
            let mut d = Diagnostic::error(format!("unknown attribute: {}", attribute.key), attribute.key_span);
            if let Some(s) = suggestion {
                d = d.with_help(format!("did you mean `{s}`?"));
            }
            self.diagnostics.push(d);
        } else if let Some(s) = suggestion {
            self.diagnostics.push(
                Diagnostic::warning(format!("unknown attribute: {}", attribute.key), attribute.key_span)
                    .with_help(format!("did you mean `{s}`?")),
            );
        }
    }

    fn duplicate_keys(&mut self, attributes: &[Attribute]) {
        let mut seen: HashMap<&str, Span> = HashMap::new();
        for a in attributes {
            if let Some(nested) = a.value.nested_list() {
                self.diagnostics
                    .push(Diagnostic::error(format!("{}: a list cannot contain a list", a.key), nested.span));
            }
            if let Some(&first) = seen.get(a.key.as_str()) {
                self.diagnostics
                    .push(Diagnostic::error(format!("attribute {} written twice", a.key), a.key_span).with_span(first));
            } else {
                seen.insert(&a.key, a.key_span);
            }
        }
    }
}

impl Checker<'_> {
    /// Inverts `on`: what each resource hosts, in the order of the registry.
    /// The members a pool selects (`select server where …`): each one is in the pool, as if it
    /// declared `pool:`.
    fn selections(&mut self) {
        // What `on` accepts.
        let hosts = self.registry.schema.expand("server|cluster|pool|fleet|subnet|vpc");
        let mut members: Vec<(ResourceId, ResourceId)> = Vec::new();
        for (pool, select) in &self.registry.selections {
            let resource = self.registry.resource(*pool);
            if self.registry.schema.base(&resource.kind) != "pool" {
                self.diagnostics.push(
                    Diagnostic::error(
                        format!("a {} does not select members: only a pool does", resource.kind),
                        select.span,
                    )
                    .with_help("`pool stores { select server where … }`"),
                );
                continue;
            }
            let mut kinds = Vec::new();
            for kind in &select.kinds {
                let expanded = self.registry.schema.expand(&kind.text);
                match expanded.iter().find(|k| !hosts.contains(k)) {
                    _ if expanded.is_empty() => {
                        self.diagnostics.push(Diagnostic::error(format!("unknown kind: {}", kind.text), kind.span))
                    }
                    Some(other) => self.diagnostics.push(
                        Diagnostic::error(format!("a pool holds what `on` designates, not a {other}"), kind.span)
                            .with_help("server, cluster, pool, fleet, subnet or vpc"),
                    ),
                    None => kinds.extend(expanded),
                }
            }
            if let Some(filter) = &select.filter {
                let derived = filter.paths().into_iter().find_map(|p| match p.steps.first().map(|s| &s.kind) {
                    Some(crate::plan::StepKind::Name(n))
                        if matches!(n.as_str(), "hosts" | "shared" | "failures" | "down") =>
                    {
                        Some((n.clone(), p.span))
                    }
                    _ => None,
                });
                if let Some((name, span)) = derived {
                    self.diagnostics.push(Diagnostic::error(
                        format!("`{name}` depends on the pools: a pool cannot select by it"),
                        span,
                    ));
                    continue;
                }
            }
            let ctx = crate::query::Ctx::new(self.registry, &self.model);
            let selected: Vec<ResourceId> = self
                .registry
                .ids()
                .filter(|&id| id != *pool && kinds.contains(&self.registry.resource(id).kind.as_str()))
                .filter(|&id| select.filter.as_ref().is_none_or(|f| ctx.matches(&crate::query::Subject::Node(id), f)))
                .collect();
            if selected.is_empty() {
                self.diagnostics.push(Diagnostic::warning(
                    format!("pool {} selects nothing", self.registry.qualified_name(*pool)),
                    select.span,
                ));
            }
            members.extend(selected.into_iter().map(|m| (m, *pool)));
        }
        for (member, pool) in members {
            let entry = self.model.references.entry((Holder::Resource(member), "pool".to_owned())).or_default();
            if !entry.contains(&pool) {
                entry.push(pool);
            }
        }
    }

    fn hosting(&mut self) {
        let mut hosts: HashMap<ResourceId, Vec<ResourceId>> = HashMap::new();
        for id in self.registry.ids() {
            for &host in self.model.reference(Holder::Resource(id), "on") {
                hosts.entry(host).or_default().push(id);
            }
        }
        self.model.hosts = hosts;

        // A database runs on another one (an instance, a schema of a shared database): the
        // chain of `on` must end on a machine.
        let mut reported = BTreeSet::new();
        for id in self.registry.ids() {
            let mut chain = vec![id];
            let mut current = id;
            while let [host] = self.model.reference(Holder::Resource(current), "on") {
                if *host == id {
                    if chain.iter().all(|r| reported.insert(*r))
                        && let Some(on) = self.registry.resource(id).attribute("on")
                    {
                        let names: Vec<String> =
                            chain.iter().chain([&id]).map(|&r| self.registry.qualified_name(r)).collect();
                        self.diagnostics.push(Diagnostic::error(
                            format!("{} runs on itself: {}", self.registry.qualified_name(id), names.join(" -> ")),
                            on.value.span,
                        ));
                    }
                    break;
                }
                if chain.contains(host) || chain.len() > 16 {
                    break;
                }
                chain.push(*host);
                current = *host;
            }
        }
    }

    /// A host name designates one virtual IP; so does an address, for virtual IPs outside
    /// any subnet (within a subnet, addresses are checked with the other addresses).
    fn virtual_ips(&mut self) {
        let mut hosts: HashMap<String, (ResourceId, Span)> = HashMap::new();
        let mut addresses: HashMap<net::Ip, Span> = HashMap::new();
        for id in self.registry.ids().filter(|&id| self.registry.resource(id).kind == "vip") {
            let resource = self.registry.resource(id);
            for item in resource.attribute("host").map(|a| a.value.items()).unwrap_or_default() {
                let Some(host) = item.as_host() else { continue };
                match hosts.get(&host) {
                    Some(&(first, span)) if first != id => self.diagnostics.push(
                        Diagnostic::error(
                            format!(
                                "host {host} is served by two virtual IPs: {} and {}",
                                self.registry.qualified_name(first),
                                self.registry.qualified_name(id)
                            ),
                            item.span,
                        )
                        .with_span(span),
                    ),
                    Some(_) => {
                        self.diagnostics.push(Diagnostic::warning(format!("host {host} written twice"), item.span))
                    }
                    None => {
                        hosts.insert(host, (id, item.span));
                    }
                }
            }
            if resource.attribute("subnet").is_some() {
                continue;
            }
            let Some(ip_attr) = resource.attribute("ip") else { continue };
            let Some(ip) = net::parse_ip(&ip_attr.value.raw) else { continue };
            match addresses.get(&ip) {
                Some(&first) => self.diagnostics.push(
                    Diagnostic::error(
                        format!("{} is the address of two virtual IPs", ip_attr.value.raw),
                        ip_attr.value.span,
                    )
                    .with_span(first),
                ),
                None => {
                    addresses.insert(ip, ip_attr.value.span);
                }
            }
        }
    }
}

/// Records what a written name designates, for editors (go to definition).
pub(crate) fn record_name(names: &mut Vec<(Span, Vec<ResourceId>)>, span: Span, resolution: &Resolution) {
    let targets = match resolution {
        Resolution::Resolved(id) => vec![*id],
        Resolution::Ambiguous(candidates) | Resolution::WrongType(candidates) => candidates.clone(),
        Resolution::Dangling => return,
    };
    names.push((span, targets));
}

fn is_external(attributes: &[Attribute]) -> bool {
    attributes.iter().any(|a| a.key == "scope" && a.value.raw == "external")
}

fn parents_text(parents: &[&str]) -> String {
    parents.iter().map(|p| if *p == "none" { "root" } else { p }).collect::<Vec<_>>().join(", ")
}

fn ty_text(ty: Ty) -> &'static str {
    match ty {
        Ty::Symbol(_) => "an identifier",
        Ty::String | Ty::Strings => "a string",
        Ty::Number => "a number",
        Ty::Semver => "a semver version",
        Ty::Version => "a version",
        Ty::Ip => "an IP address",
        Ty::Cidr => "a CIDR",
        Ty::Host => "a host name",
        Ty::Url => "a URL such as \"https://host/path\"",
        Ty::Bandwidth => "a bandwidth such as 100Mbps or \"1.5 Gbit/s\"",
        Ty::Spdx => "an SPDX license expression such as \"Apache-2.0\"",
        Ty::Ref(_) => "a name",
        Ty::Pattern(_) => "a string matching its pattern",
    }
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::{Diagnostic, Severity};
    use crate::source::SourceFile;

    fn diagnostics(text: &str) -> Vec<Diagnostic> {
        crate::compile(&[SourceFile { path: "a.ca".into(), text: text.into() }]).diagnostics
    }

    fn messages(text: &str, severity: Severity) -> Vec<String> {
        diagnostics(text).into_iter().filter(|d| d.severity == severity).map(|d| d.message).collect()
    }

    fn errors(text: &str) -> Vec<String> {
        messages(text, Severity::Error)
    }

    #[test]
    fn unknown_kind_with_suggestion() {
        let d = diagnostics("serveur web01");
        assert_eq!(d[0].message, "unknown resource kind: serveur");
        assert_eq!(d[0].help.as_deref(), Some("did you mean `server`?"));
    }

    #[test]
    fn invalid_symbol_value() {
        assert_eq!(errors("site s { hosting: cloude, owner: t }\nteam t"), ["invalid value for hosting: cloude"]);
    }

    #[test]
    fn reference_of_the_wrong_kind() {
        let e = errors("team t\nsystem s { owner: t\n app a { on: db }\n database db { } }");
        assert_eq!(e, ["db is a database, server or cluster or pool or fleet or subnet or vpc expected"]);
    }

    #[test]
    fn lexical_scope_resolves_local_names() {
        let text = "team t\n\
                    system a { owner: t, app x { -> db : jdbc }, database db { } }\n\
                    system b { owner: t, app y { -> db : jdbc }, database db { } }";
        assert!(errors(text).is_empty(), "{:?}", errors(text));
        assert_eq!(errors("team t\nsystem a { owner: t, database db { } }\nsystem b { owner: t, database db { } }\napp z { -> db }").len(), 1);
    }

    #[test]
    fn ip_outside_cidr_and_duplicate_ip() {
        let e = errors(
            "team t\nsite s { hosting: cloud, owner: t\n vpc v { subnet dmz { cidr: 10.0.1.0/24\n \
             server a { ip: 10.0.2.1 }\n server b { ip: 10.0.1.5 }\n server c { ip: 10.0.1.5 } } } }",
        );
        assert_eq!(e, ["10.0.2.1 is outside the CIDR of subnet dmz", "10.0.1.5 is used twice in subnet dmz"]);
    }

    #[test]
    fn device_in_a_subnet_with_interfaces() {
        // A Wi-Fi access point in its VLAN, cabled to a switch; its address checked like a server's.
        let e = errors(
            "team t\nsite s { hosting: on-prem, owner: t\n network n {\n \
             subnet wifi { cidr: 10.0.9.0/24\n device ap1 { role: \"Wi-Fi\", ip: 10.0.9.10, interface e0 }\n \
             device ap2 { ip: 10.0.8.1 } }\n switch sw { interface g1 { peer: ap1.e0 } } } }",
        );
        assert_eq!(e, ["10.0.8.1 is outside the CIDR of subnet wifi"]);
    }

    #[test]
    fn duplicate_link_and_redundant_link_without_port() {
        let e = errors("team t\napp a { owner: t, -> b : https/443 }\napp b { owner: t }\na -> b : HTTPS/0443");
        assert_eq!(e, ["link declared twice: a -> b : https/443"]);
        let w = messages("team t\napp a { owner: t, -> b : https/443, -> b }\napp b { owner: t }", Severity::Warning);
        assert_eq!(w, ["redundant link without port: a -> b is also declared with a port"]);
    }

    #[test]
    fn channel_must_belong_to_an_end() {
        let e = errors(
            "saas k { channel tarif { type: topic } }\nsaas other { }\napp a { -> other : https { channel: tarif } }",
        );
        assert_eq!(e, ["channel k.tarif belongs neither to a nor to other"]);
    }

    #[test]
    fn rack_in_another_site() {
        let e = errors(
            "site p { hosting: on-prem, rack r1 }\nsite q { hosting: on-prem, network n { subnet s { cidr: 10.0.0.0/8, server x { rack: r1 } } } }",
        );
        assert_eq!(e, ["x is in site q but its rack r1 is in p"]);
    }

    #[test]
    fn contradictory_cables() {
        let e = errors("switch sw { interface a { peer: web }, interface b { peer: web } }\nserver web");
        assert_eq!(e, ["web is cabled to several peers: sw.a, sw.b"]);
    }

    #[test]
    fn connector_ends() {
        let e = errors("vpn_gateway g\nvpn_tunnel t { ends: [g, g] }\nvpn_tunnel u { ends: [g] }");
        assert_eq!(e, ["ends expects exactly 2 values, 1 given", "both ends of t are the same"]);
    }

    #[test]
    fn free_keys_warn_when_close_and_fail_in_strict_mode() {
        let w = messages("team t\napp a { owner: t, on: x, verison: 1.2.3, tema: y }\nfleet x", Severity::Warning);
        assert_eq!(w, ["unknown attribute: verison"]);
        let strict = crate::compile_with(
            &[SourceFile { path: "a.ca".into(), text: "data d { team: x }".into() }],
            crate::Options { strict: true, ..Default::default() },
        );
        assert_eq!(strict.diagnostics[0].message, "unknown attribute: team");
    }

    #[test]
    fn owner_is_inherited_and_external_is_exempt() {
        let i = messages(
            "team t\nsystem s { owner: t, app a { on: x } }\nserver x\napp e { scope: external }",
            Severity::Incomplete,
        );
        assert_eq!(
            i,
            [
                "approximate placement: server x at the root (expected parent: subnet)",
                "server x: no owner, neither written nor inherited"
            ]
        );
    }
}
