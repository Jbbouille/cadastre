//! Built-in v1 schema (spec § 4): resource kinds, parents, attributes, groups; and the schema
//! of a project, which adds keys to them and kinds of its own (`kind` declarations).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Type of an attribute value.
#[derive(Clone, Copy, Debug)]
pub enum Ty {
    /// Identifier from a closed list.
    Symbol(&'static [&'static str]),
    String,
    /// String, or list of strings (`question`).
    Strings,
    Number,
    Semver,
    /// `semver`, `number` or `string`.
    Version,
    Ip,
    Cidr,
    /// DNS host name, written bare (`labels.example.com`) or as a string.
    Host,
    /// URL, as a string (`"https://labels.example.com"`, `"ssh://admin@fw01"`), without a secret.
    Url,
    /// A bandwidth, `100Mbps` or `"1.5 Gbit/s"`, compared in bits per second.
    Bandwidth,
    /// An SPDX license expression, as a string: `"Apache-2.0"`, `"MIT OR Apache-2.0"`.
    Spdx,
    /// Reference to a resource of one of these kinds or groups (`"server|cluster"`).
    Ref(&'static str),
    /// A string matching a regular expression as a whole (`pattern("APP-[0-9]{3}")`).
    Pattern(&'static Pattern),
}

/// A regular expression of the schema of a project, as written and compiled.
#[derive(Debug)]
pub struct Pattern {
    pub source: &'static str,
    pub regex: regex_lite::Regex,
}

/// Number of allowed values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Card {
    One,
    OneOrMore,
    Many,
    Exactly(usize),
}

#[derive(Clone, Copy, Debug)]
pub struct AttrDef {
    pub key: &'static str,
    pub ty: Ty,
    pub card: Card,
    pub required: bool,
}

const fn attr(key: &'static str, ty: Ty) -> AttrDef {
    AttrDef { key, ty, card: Card::One, required: false }
}

const fn required(key: &'static str, ty: Ty) -> AttrDef {
    AttrDef { key, ty, card: Card::One, required: true }
}

const fn many(key: &'static str, ty: Ty) -> AttrDef {
    AttrDef { key, ty, card: Card::Many, required: false }
}

const fn with_card(def: AttrDef, card: Card) -> AttrDef {
    AttrDef { card, ..def }
}

#[derive(Debug)]
pub struct KindDef {
    pub name: &'static str,
    /// Precise parents; `none` is the root.
    pub parents: &'static [&'static str],
    pub attrs: &'static [AttrDef],
    /// `{2}` attribute that defines the ends of a connector.
    pub connector: Option<&'static str>,
    /// `owner` required (after inheritance), unless `scope: external`.
    pub owner_required: bool,
    /// A kind within another resource only, a detail of it (an interface of a device, a component
    /// of an app, a channel of a broker, a participant of a process): a resource like the others
    /// (named, resolved, renamed), listed with its parent rather than among the kinds of the map.
    pub within: bool,
}

const fn kind(name: &'static str, parents: &'static [&'static str], attrs: &'static [AttrDef]) -> KindDef {
    KindDef { name, parents, attrs, connector: None, owner_required: false, within: false }
}

const fn within(def: KindDef) -> KindDef {
    KindDef { within: true, ..def }
}

const fn owned(def: KindDef) -> KindDef {
    KindDef { owner_required: true, ..def }
}

const fn connector(def: KindDef, attr: &'static str) -> KindDef {
    KindDef { connector: Some(attr), ..def }
}

const HOSTING: &[&str] = &["on-prem", "colocation", "cloud", "office"];
const CHANNEL_TYPE: &[&str] = &["topic", "queue", "directory", "bucket"];

const RACK: AttrDef = attr("rack", Ty::Ref("rack"));
/// The host of a virtual machine (`virtual server web01 { on: esx01 }`): a server or a cluster.
const ON_HOST: AttrDef = attr("on", Ty::Ref("server|cluster"));

/// The kinds that may be `virtual`: machines, physical or running on a host.
pub const MACHINES: &[&str] =
    &["server", "router", "switch", "firewall", "forward_proxy", "device", "load_balancer", "proxy"];
const SYSTEM: AttrDef = attr("system", Ty::Ref("system"));
/// The address of a service, for its users.
const URL: AttrDef = many("url", Ty::Url);
/// The access for its administration: a web console, SSH, RDP.
const CONSOLE: AttrDef = many("console", Ty::Url);
/// The license of a software: an SPDX expression.
const LICENSE: AttrDef = attr("license", Ty::Spdx);
/// The devices traversed by the flows towards the internet, and from it (`through` deduced).
const EGRESS: AttrDef = many("egress", Ty::Ref("transit"));
const INGRESS: AttrDef = many("ingress", Ty::Ref("transit"));
const NETWORK_DEVICE_PARENTS: &[&str] = &["site", "vpc", "network"];
const SOFTWARE_DEVICE_PARENTS: &[&str] = &["site", "vpc", "network", "none", "system"];

pub static KINDS: &[KindDef] = &[
    // Infrastructure
    kind("site", &["none"], &[required("hosting", Ty::Symbol(HOSTING)), EGRESS, INGRESS]),
    kind("rack", &["site"], &[]),
    kind("vpc", &["site"], &[EGRESS, INGRESS]),
    kind("network", &["site"], &[EGRESS, INGRESS]),
    // `gateway`: what routes it, an interface or its device; otherwise deduced from the routers
    // and firewalls with an interface in it. `vlan`: its VLAN, matched with those of the ports.
    kind(
        "subnet",
        &["vpc", "network"],
        &[
            required("cidr", Ty::Cidr),
            attr("gateway", Ty::Ref("interface|router|firewall")),
            attr("vlan", Ty::Number),
            EGRESS,
            INGRESS,
        ],
    ),
    owned(kind(
        "server",
        &["subnet"],
        &[
            attr("ip", Ty::Ip),
            RACK,
            ON_HOST,
            attr("cluster", Ty::Ref("cluster")),
            many("pool", Ty::Ref("pool")),
            SYSTEM,
            CONSOLE,
        ],
    )),
    // A group of servers an app or a database runs on, each server declaring it (`pool:`): a farm,
    // the servers of the stores.
    kind("pool", &["none", "site", "system"], &[SYSTEM]),
    owned(kind("cluster", &["site", "vpc", "network", "subnet"], &[SYSTEM, CONSOLE])),
    kind("fleet", &["site", "network", "subnet"], &[]),
    within(kind(
        "interface",
        &["server", "firewall", "router", "switch", "vpn_gateway", "device"],
        &[
            attr("subnet", Ty::Ref("subnet")),
            attr("ip", Ty::Ip),
            attr("peer", Ty::Ref("interface|server")),
            // The VLANs a port carries: one, untagged (access), or several, tagged (trunk).
            attr("vlan", Ty::Number),
            many("vlans", Ty::Number),
            // The port-channel or vPC the port belongs to: its cables make one link.
            attr("aggregate", Ty::String),
        ],
    )),
    kind("router", NETWORK_DEVICE_PARENTS, &[RACK, SYSTEM, ON_HOST, CONSOLE]),
    kind("switch", NETWORK_DEVICE_PARENTS, &[RACK, SYSTEM, ON_HOST, CONSOLE]),
    kind("firewall", NETWORK_DEVICE_PARENTS, &[RACK, SYSTEM, ON_HOST, CONSOLE]),
    kind("forward_proxy", NETWORK_DEVICE_PARENTS, &[RACK, SYSTEM, ON_HOST, CONSOLE]),
    // Also in a subnet, like a server: a Wi-Fi access point in its VLAN.
    kind("device", &["site", "vpc", "network", "subnet"], &[attr("ip", Ty::Ip), RACK, ON_HOST, SYSTEM, CONSOLE]),
    kind("load_balancer", SOFTWARE_DEVICE_PARENTS, &[RACK, SYSTEM, ON_HOST, CONSOLE]),
    kind("proxy", SOFTWARE_DEVICE_PARENTS, &[RACK, SYSTEM, ON_HOST, CONSOLE]),
    kind("vpn_gateway", &["site", "vpc", "firewall", "router"], &[CONSOLE]),
    // Virtual IP: an address and / or host names served by a load balancer, a proxy or a cluster.
    kind(
        "vip",
        &["load_balancer", "proxy", "cluster"],
        &[many("host", Ty::Host), attr("ip", Ty::Ip), attr("subnet", Ty::Ref("subnet")), URL],
    ),
    connector(
        kind("vpn_tunnel", &["none"], &[with_card(required("ends", Ty::Ref("vpn_gateway")), Card::Exactly(2))]),
        "ends",
    ),
    connector(kind("vpc_peering", &["none"], &[with_card(required("ends", Ty::Ref("vpc")), Card::Exactly(2))]), "ends"),
    connector(
        kind(
            "wan_link",
            &["none"],
            &[with_card(required("ends", Ty::Ref("router|firewall|vpn_gateway")), Card::Exactly(2))],
        ),
        "ends",
    ),
    kind("actor", &["none", "site"], &[]),
    // Organisation
    kind("team", &["none", "team"], &[]),
    // Fonctionnel
    owned(kind("system", &["none", "system"], &[EGRESS, INGRESS])),
    owned(kind(
        "app",
        &["none", "system"],
        &[
            with_card(required("on", Ty::Ref("server|cluster|pool|fleet|subnet|vpc")), Card::OneOrMore),
            attr("version", Ty::Semver),
            // The time zone of the jobs a scheduler runs.
            attr("timezone", Ty::String),
            URL,
            CONSOLE,
            LICENSE,
            EGRESS,
            INGRESS,
        ],
    )),
    owned(kind(
        "job",
        &["none", "system"],
        &[
            attr("on", Ty::Ref("server|cluster|pool")),
            attr("schedule", Ty::String),
            attr("scheduler", Ty::Ref("app")),
            attr("timezone", Ty::String),
            LICENSE,
            EGRESS,
            INGRESS,
        ],
    )),
    owned(kind(
        "database",
        &["none", "system"],
        &[
            with_card(attr("on", Ty::Ref("server|cluster|pool|fleet|subnet|vpc|database")), Card::OneOrMore),
            attr("version", Ty::Version),
            many("stores", Ty::Ref("data")),
            LICENSE,
            CONSOLE,
            EGRESS,
            INGRESS,
        ],
    )),
    // A business process: its steps are links (`process` blocks, spec § 3); the participants of
    // its sequence diagram placed first, in this order.
    kind("process", &["none", "system"], &[many("participants", Ty::Ref("flow_endpoint|participant"))]),
    // A participant of a process standing for several resources (`participant esx [esx1]`): a
    // cluster stands for its servers.
    within(kind("participant", &["process"], &[many("members", Ty::Ref("flow_endpoint|cluster"))])),
    owned(kind("saas", &["none", "system"], &[many("stores", Ty::Ref("data")), URL, CONSOLE, LICENSE])),
    within(kind("component", &["app"], &[attr("version", Ty::Version), URL, LICENSE])),
    within(kind(
        "channel",
        &["saas", "app", "server"],
        &[required("type", Ty::Symbol(CHANNEL_TYPE)), many("carries", Ty::Ref("data"))],
    )),
    kind("data", &["none"], &[]),
    // The internet as an end, for the flows whose destination or origin is not modeled.
    kind("internet", &["none"], &[]),
];

/// Standard keys carried by any resource.
pub static RESOURCE_KEYS: &[AttrDef] = &[
    attr("label", Ty::String),
    attr("description", Ty::String),
    attr("confidence", Ty::Symbol(CONFIDENCE)),
    attr("question", Ty::Strings),
    attr("scope", Ty::Symbol(SCOPE)),
    attr("owner", Ty::Ref("team")),
    attr("operator", Ty::Ref("team")),
    // How the resource is reached: over the internet, or from the internal network. Inherited
    // along containment, then `on`; a SaaS and the internet are on the internet by default.
    attr("reach", Ty::Symbol(REACH)),
    // The text of the resource as a participant of a sequence diagram, in place of its label
    // and the `node_labels` of the view; `\n` breaks the line.
    attr("sequence_label", Ty::String),
];

pub const REACH: &[&str] = &["internet", "internal"];

/// Standard keys carried by links.
pub static LINK_KEYS: &[AttrDef] = &[
    attr("description", Ty::String),
    attr("confidence", Ty::Symbol(CONFIDENCE)),
    attr("question", Ty::Strings),
    many("carries", Ty::Ref("data")),
    attr("exchange", Ty::Symbol(EXCHANGE)),
    many("channel", Ty::Ref("channel")),
    many("through", Ty::Ref("transit")),
    // The processes a link serves, without an order (the steps of a `process` give one).
    many("processes", Ty::Ref("process")),
];

pub const CONFIDENCE: &[&str] = &["confirmed", "assumed", "doubtful"];
pub const SCOPE: &[&str] = &["internal", "external"];
pub const EXCHANGE: &[&str] = &["push", "pull", "both"];

/// Groups: named union aliases.
pub static GROUPS: &[(&str, &[&str])] = &[
    (
        "network_device",
        &["router", "switch", "firewall", "vpn_gateway", "forward_proxy", "load_balancer", "proxy", "device"],
    ),
    ("connector", &["vpn_tunnel", "vpc_peering", "wan_link"]),
    (
        "transit",
        &[
            "router",
            "switch",
            "firewall",
            "vpn_gateway",
            "forward_proxy",
            "device",
            "vpn_tunnel",
            "vpc_peering",
            "wan_link",
        ],
    ),
    (
        "flow_endpoint",
        &[
            "app",
            "job",
            "database",
            "saas",
            "server",
            "fleet",
            "load_balancer",
            "proxy",
            "vip",
            "device",
            "actor",
            "internet",
        ],
    ),
];

pub fn kind_def(name: &str) -> Option<&'static KindDef> {
    KINDS.iter().find(|k| k.name == name)
}

/// Expands a union of kinds and groups (`"server|transit"`) into concrete kinds.
pub fn expand(union: &str) -> Vec<&'static str> {
    let mut out = Vec::new();
    for part in union.split('|') {
        if let Some((_, members)) = GROUPS.iter().find(|(g, _)| *g == part) {
            out.extend(members.iter().copied());
        } else if let Some(k) = kind_def(part) {
            out.push(k.name);
        }
    }
    out
}

/// Known attribute for this kind: kind-specific, otherwise a standard key.
pub fn resource_attr(kind: &KindDef, key: &str) -> Option<&'static AttrDef> {
    let own = KINDS.iter().find(|k| k.name == kind.name).and_then(|k| k.attrs.iter().find(|a| a.key == key));
    own.or_else(|| RESOURCE_KEYS.iter().find(|a| a.key == key))
}

pub fn link_attr(key: &str) -> Option<&'static AttrDef> {
    LINK_KEYS.iter().find(|a| a.key == key)
}

/// Where a resource of this kind can be placed.
#[derive(Debug, PartialEq, Eq)]
pub enum Placement {
    /// Precise parent.
    Exact,
    /// Possible ancestor of the precise parent, or the root: accepted, but incomplete.
    Approximate,
    Impossible,
}

/// `parent` is `None` for the root.
pub fn placement(kind: &KindDef, parent: Option<&str>) -> Placement {
    let parent = parent.unwrap_or("none");
    if kind.parents.contains(&parent) {
        return Placement::Exact;
    }
    // Possible ancestors: closure of the declared parents of the parent kinds.
    let mut seen: Vec<&str> = kind.parents.to_vec();
    let mut index = 0;
    while index < seen.len() {
        if let Some(def) = kind_def(seen[index]) {
            for p in def.parents {
                if !seen.contains(p) {
                    seen.push(p);
                }
            }
        }
        index += 1;
    }
    if seen.contains(&parent) || parent == "none" { Placement::Approximate } else { Placement::Impossible }
}

/// Kind names known to the schema, for suggestions.
pub fn kind_names() -> impl Iterator<Item = &'static str> {
    KINDS.iter().map(|k| k.name)
}

/// A type as the reference and the schema of a project write it: `ref(server | cluster)+`.
pub fn type_text(def: &AttrDef) -> String {
    let ty = match def.ty {
        Ty::Symbol(values) => values.join(" | "),
        Ty::String => "string".to_owned(),
        Ty::Strings => "string, or list of strings".to_owned(),
        Ty::Number => "number".to_owned(),
        Ty::Semver => "semver".to_owned(),
        Ty::Version => "version".to_owned(),
        Ty::Ip => "ip".to_owned(),
        Ty::Cidr => "cidr".to_owned(),
        Ty::Host => "host".to_owned(),
        Ty::Url => "url".to_owned(),
        Ty::Bandwidth => "bandwidth".to_owned(),
        Ty::Spdx => "spdx".to_owned(),
        Ty::Ref(union) => format!("ref({})", union.replace('|', " | ")),
        Ty::Pattern(pattern) => format!("pattern(\"{}\")", pattern.source),
    };
    let card = match def.card {
        Card::One => String::new(),
        Card::OneOrMore => "+".to_owned(),
        Card::Many => "*".to_owned(),
        Card::Exactly(n) => format!("{{{n}}}"),
    };
    format!("{ty}{card}")
}

/// The schema of a project: the built-in one, its kinds with the keys the project adds, and the
/// kinds of the project, each `like` a built-in one.
#[derive(Clone, Debug, Default)]
pub struct Schema {
    /// Kinds declared by the project: built-in ones with their added keys, or its own.
    declared: Vec<&'static KindDef>,
    /// The built-in kind each kind of the project is like.
    likes: HashMap<&'static str, &'static str>,
    /// Keys of the links declared by the project: of every link, of the links of a layer.
    links: &'static [AttrDef],
    layers: Vec<(&'static str, &'static [AttrDef])>,
    /// Groups of the project, with their kinds.
    groups: Vec<(&'static str, &'static [&'static str])>,
}

impl Schema {
    pub fn new(declared: Vec<&'static KindDef>, likes: HashMap<&'static str, &'static str>) -> Self {
        Self { declared, likes, links: &[], layers: Vec::new(), groups: Vec::new() }
    }

    pub fn with_groups(self, groups: Vec<(&'static str, &'static [&'static str])>) -> Self {
        Self { groups, ..self }
    }

    /// The groups, built in then of the project, with their kinds.
    pub fn groups(&self) -> impl Iterator<Item = (&'static str, &'static [&'static str])> + '_ {
        GROUPS.iter().copied().chain(self.groups.iter().copied())
    }

    /// The names of the groups, built in then of the project.
    pub fn group_names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.groups().map(|(g, _)| g)
    }

    pub fn with_links(self, links: &'static [AttrDef], layers: Vec<(&'static str, &'static [AttrDef])>) -> Self {
        Self { links, layers, ..self }
    }

    /// A key of a link with these layers: built in, declared for every link, or for one of its
    /// layers.
    pub fn link_attr(&self, key: &str, layers: &[&str]) -> Option<&'static AttrDef> {
        link_attr(key).or_else(|| self.link_keys(layers).find(|a| a.key == key))
    }

    /// The keys a project declares for a link with these layers.
    pub fn link_keys<'s>(&'s self, layers: &'s [&'s str]) -> impl Iterator<Item = &'static AttrDef> + 's {
        let of_layers = self.layers.iter().filter(|(l, _)| layers.contains(l)).flat_map(|(_, attrs)| attrs.iter());
        self.links.iter().chain(of_layers)
    }

    pub fn kind_def(&self, name: &str) -> Option<&'static KindDef> {
        self.declared.iter().copied().find(|k| k.name == name).or_else(|| kind_def(name))
    }

    /// The built-in kind a kind is, or is like.
    pub fn base<'k>(&self, kind: &'k str) -> &'k str {
        self.likes.get(kind).copied().unwrap_or(kind)
    }

    /// The kinds of a union of kinds and groups; a kind of the project is in the groups and the
    /// unions its model is in: `select device` selects the printers `like: device`.
    pub fn expand(&self, union: &str) -> Vec<&'static str> {
        let mut out = Vec::new();
        for part in union.split('|') {
            if let Some((_, members)) = self.groups().find(|(g, _)| *g == part) {
                out.extend(members.iter().copied().filter(|m| !out.contains(m)).collect::<Vec<_>>());
            } else if let Some(k) = self.kind_def(part) {
                out.push(k.name);
            }
        }
        let own: Vec<&'static str> = self
            .likes
            .iter()
            .filter(|(kind, like)| out.contains(like) && !out.contains(kind))
            .map(|(k, _)| *k)
            .collect();
        out.extend(own);
        out
    }

    pub fn resource_attr(&self, kind: &KindDef, key: &str) -> Option<&'static AttrDef> {
        let own = self.kind_def(kind.name).and_then(|k| k.attrs.iter().find(|a| a.key == key));
        own.or_else(|| RESOURCE_KEYS.iter().find(|a| a.key == key))
    }

    pub fn placement(&self, kind: &KindDef, parent: Option<&str>) -> Placement {
        let parent = parent.unwrap_or("none");
        if kind.parents.contains(&parent) {
            return Placement::Exact;
        }
        let mut seen: Vec<&str> = kind.parents.to_vec();
        let mut index = 0;
        while index < seen.len() {
            if let Some(def) = self.kind_def(seen[index]) {
                for p in def.parents {
                    if !seen.contains(p) {
                        seen.push(p);
                    }
                }
            }
            index += 1;
        }
        if seen.contains(&parent) || parent == "none" { Placement::Approximate } else { Placement::Impossible }
    }

    /// The keys holding references, with the kinds they designate: of the resources and the
    /// links, built in or declared by the project.
    pub fn reference_keys(&self) -> Vec<(&'static str, &'static str)> {
        let declared = self.declared.iter().flat_map(|k| k.attrs.iter());
        let links = self.links.iter().chain(self.layers.iter().flat_map(|(_, attrs)| attrs.iter()));
        KINDS
            .iter()
            .flat_map(|k| k.attrs.iter())
            .chain(RESOURCE_KEYS)
            .chain(LINK_KEYS)
            .chain(declared)
            .chain(links)
            .filter_map(|a| match a.ty {
                Ty::Ref(union) => Some((a.key, union)),
                _ => None,
            })
            .collect()
    }

    /// The kinds: built-in ones, then those of the project.
    pub fn kind_names(&self) -> Vec<&'static str> {
        let mut names: Vec<&'static str> = kind_names().collect();
        names.extend(self.declared.iter().map(|k| k.name).filter(|n| kind_def(n).is_none()));
        names
    }
}

/// Definitions of the schema of a project, made `'static` once per distinct definition: the
/// language server compiles a project at each keystroke, the same definitions again.
pub(crate) fn intern<T: Send + Sync + 'static>(key: String, make: impl FnOnce() -> T) -> &'static T {
    type Table = HashMap<(std::any::TypeId, String), &'static (dyn std::any::Any + Send + Sync)>;
    static TABLE: OnceLock<Mutex<Table>> = OnceLock::new();
    let mut table = TABLE.get_or_init(|| Mutex::new(HashMap::new())).lock().unwrap_or_else(|e| e.into_inner());
    let entry = table.entry((std::any::TypeId::of::<T>(), key)).or_insert_with(|| Box::leak(Box::new(make())));
    entry.downcast_ref::<T>().expect("interned under its own type")
}

pub(crate) fn intern_str(text: &str) -> &'static str {
    intern(text.to_owned(), || text.to_owned()).as_str()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placement_exact_approximate_impossible() {
        let server = kind_def("server").unwrap();
        assert_eq!(placement(server, Some("subnet")), Placement::Exact);
        assert_eq!(placement(server, Some("site")), Placement::Approximate);
        assert_eq!(placement(server, Some("vpc")), Placement::Approximate);
        assert_eq!(placement(server, None), Placement::Approximate);
        assert_eq!(placement(server, Some("app")), Placement::Impossible);
        assert_eq!(placement(server, Some("rack")), Placement::Impossible);
        let app = kind_def("app").unwrap();
        assert_eq!(placement(app, Some("subnet")), Placement::Impossible);
    }

    #[test]
    fn expands_groups() {
        let types = expand("transit");
        assert!(types.contains(&"vpn_tunnel") && types.contains(&"firewall"));
        assert_eq!(expand("server|cluster"), ["server", "cluster"]);
    }

    #[test]
    fn every_schema_reference_names_a_known_kind() {
        for k in KINDS {
            for p in k.parents {
                assert!(*p == "none" || kind_def(p).is_some(), "{}: parent {p}", k.name);
            }
            for a in k.attrs.iter().chain(RESOURCE_KEYS).chain(LINK_KEYS) {
                if let Ty::Ref(target) = a.ty {
                    for part in target.split('|') {
                        assert!(!expand(part).is_empty(), "{}.{}: {part}", k.name, a.key);
                    }
                }
            }
        }
    }
}
