//! Network paths: the devices a flow goes through, deduced from the cabling (`peer`), the subnets
//! and their gateways.
//!
//! - **The ends** of a path are machines: a resource runs on its machine (`on`), a virtual machine
//!   is cabled through its host. A machine is in a subnet by its place, or by its address.
//! - **In a subnet** (layer 2), the cables are followed through switches only, the ports carrying
//!   the VLAN of the subnet (`vlan`, `vlans`; a port without them carries any).
//! - **Between subnets** (layer 3), the path goes up to a gateway of the first subnet (its
//!   `gateway`, otherwise a router or a firewall with an interface in it), from router to router
//!   through the subnets they share and the connectors between them, then down to the second.
//! - **Redundant ways** make one path whose steps have variants: `col-sw10-01 or col-sw10-02`.

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

use crate::check::{Holder, Model};
use crate::net::{self, Cidr};
use crate::registry::{Registry, ResourceId};

/// A path: the devices traversed, in order, each step with its variants; the cables taken, as
/// pairs of plugs (an interface, or a server for its implicit interface).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Path {
    pub steps: Vec<Vec<ResourceId>>,
    pub cables: Vec<(ResourceId, ResourceId)>,
    /// The connectors crossed between routers (`wan_link`, `vpn_tunnel`), with their ends.
    pub connectors: Vec<ResourceId>,
    /// Beyond the cabling, to or from the internet: the devices of the `egress` from one to the
    /// next, the last to the end reached over the internet (or the other way for an `ingress`).
    pub internet: Vec<(ResourceId, ResourceId)>,
}

impl Path {
    /// The devices traversed, in order, each once.
    pub fn devices(&self) -> Vec<ResourceId> {
        let mut out = Vec::new();
        for step in &self.steps {
            for &d in step {
                if !out.contains(&d) {
                    out.push(d);
                }
            }
        }
        out
    }

    fn then(mut self, other: Path) -> Path {
        self.steps.extend(other.steps);
        self.cables.extend(other.cables);
        self.connectors.extend(other.connectors);
        self.internet.extend(other.internet);
        self
    }
}

/// Why no path was found.
#[derive(Clone, Debug, PartialEq)]
pub enum NoPath {
    /// The resource runs on no machine.
    NoMachine(ResourceId),
    /// The machine is in no subnet: neither placed in one nor with an address in one.
    NoSubnet(ResourceId),
    /// No router nor firewall routes the subnet.
    NoGateway(ResourceId),
    /// No router path between the gateways of two subnets.
    NoRoute(ResourceId, ResourceId),
    /// The cables stop: the devices reached last, the subnet whose VLAN is followed.
    Stops { at: Vec<ResourceId>, subnet: Option<ResourceId> },
    /// To the internet without an `egress`, or from it without an `ingress`.
    NoEgress { end: ResourceId, key: &'static str },
    /// Both ends are reached over the internet.
    Outside,
}

/// Whether a resource is reached over the internet: its nearest `reach` (its own or an enclosing
/// resource's, then that of what it runs on), otherwise a `saas` or the `internet`.
pub fn over_internet(registry: &Registry, model: &Model, id: ResourceId) -> bool {
    let mut current = id;
    for _ in 0..6 {
        let own = std::iter::once(current)
            .chain(registry.ancestors(current))
            .find_map(|r| registry.resource(r).attribute("reach").map(|a| a.value.raw.clone()));
        if let Some(reach) = own {
            return reach == "internet";
        }
        match model.reference(Holder::Resource(current), "on").first() {
            Some(&host) => current = host,
            None => break,
        }
    }
    matches!(registry.resource(id).kind.as_str(), "saas" | "internet")
}

/// The nearest `egress` or `ingress` of a resource: its own or an enclosing resource's, then
/// those of what it runs on (`on`); with the resource it is written on.
pub fn nearest(registry: &Registry, model: &Model, id: ResourceId, key: &str) -> Option<(ResourceId, Vec<ResourceId>)> {
    let mut current = id;
    for _ in 0..6 {
        let own = std::iter::once(current).chain(registry.ancestors(current)).find_map(|r| {
            let items = model.reference(Holder::Resource(r), key);
            (!items.is_empty()).then(|| (r, items.to_vec()))
        });
        if own.is_some() {
            return own;
        }
        current = *model.reference(Holder::Resource(current), "on").first()?;
    }
    None
}

/// The network of a registry: plugs, cables, subnets.
pub struct Network<'a> {
    registry: &'a Registry,
    model: &'a Model,
    /// Plug -> the plug cabled to it.
    cables: HashMap<ResourceId, ResourceId>,
    /// Device -> its plugs: its interfaces, and itself for a server (its implicit interface).
    plugs: HashMap<ResourceId, Vec<ResourceId>>,
    subnets: Vec<(ResourceId, Cidr)>,
}

/// A cable, as its two plugs.
type Cable = (ResourceId, ResourceId);

/// A way between routers: its intermediate steps, the gateways reached, the connectors crossed.
type Route = (Vec<Vec<ResourceId>>, Vec<ResourceId>, Vec<ResourceId>);

/// The kinds a path goes through in a subnet.
const SWITCHES: &[&str] = &["switch"];
/// The kinds that route between subnets.
const ROUTERS: &[&str] = &["router", "firewall"];

impl<'a> Network<'a> {
    pub fn new(registry: &'a Registry, model: &'a Model) -> Self {
        let mut cables = HashMap::new();
        let mut plugs: HashMap<ResourceId, Vec<ResourceId>> = HashMap::new();
        let mut subnets = Vec::new();
        for id in registry.ids() {
            let resource = registry.resource(id);
            let base = registry.schema.base(&resource.kind);
            if base == "interface"
                && let Some(device) = resource.parent
            {
                plugs.entry(device).or_default().push(id);
            }
            if base == "server" {
                plugs.entry(id).or_default().push(id);
            }
            if let Some(&peer) = model.reference(Holder::Resource(id), "peer").first() {
                cables.insert(id, peer);
                cables.insert(peer, id);
            }
            if base == "subnet"
                && let Some(cidr) = resource.attribute("cidr").and_then(|c| net::parse_cidr(&c.value.raw))
            {
                subnets.push((id, cidr));
            }
        }
        Network { registry, model, cables, plugs, subnets }
    }

    fn base(&self, id: ResourceId) -> &str {
        self.registry.schema.base(&self.registry.resource(id).kind)
    }

    /// The device of a plug: the parent of an interface, a server itself.
    pub fn device(&self, plug: ResourceId) -> ResourceId {
        if self.base(plug) == "interface" { self.registry.resource(plug).parent.unwrap_or(plug) } else { plug }
    }

    /// The cables from devices to others, as pairs of plugs, from the first ones.
    pub fn cables_between(&self, from: &[ResourceId], to: &[ResourceId]) -> Vec<(ResourceId, ResourceId)> {
        let mut out: Vec<(ResourceId, ResourceId)> = from
            .iter()
            .flat_map(|d| self.plugs.get(d).into_iter().flatten())
            .filter_map(|&plug| self.cables.get(&plug).map(|&other| (plug, other)))
            .filter(|&(_, other)| to.contains(&self.device(other)))
            .collect();
        out.sort();
        out
    }

    fn number(&self, id: ResourceId, key: &str) -> Vec<i64> {
        self.registry
            .resource(id)
            .attribute(key)
            .map(|a| a.value.items().iter().filter_map(|v| v.raw.parse().ok()).collect())
            .unwrap_or_default()
    }

    /// Whether a plug carries a VLAN: its `vlan` or one of its `vlans`; any, without them.
    fn carries(&self, plug: ResourceId, vlan: Option<i64>) -> bool {
        let Some(vlan) = vlan else { return true };
        if self.base(plug) != "interface" {
            return true;
        }
        let (access, trunk) = (self.number(plug, "vlan"), self.number(plug, "vlans"));
        (access.is_empty() && trunk.is_empty()) || access.contains(&vlan) || trunk.contains(&vlan)
    }

    /// The machines a resource runs on: itself for a machine, otherwise along `on` (a cluster
    /// for its servers, a pool for its members).
    pub fn machines(&self, id: ResourceId) -> Vec<ResourceId> {
        let mut out = Vec::new();
        self.collect_machines(id, 0, &mut out);
        out
    }

    fn collect_machines(&self, id: ResourceId, depth: usize, out: &mut Vec<ResourceId>) {
        if depth > 8 {
            return;
        }
        let base = self.base(id);
        if crate::schema::MACHINES.contains(&base) {
            if !out.contains(&id) {
                out.push(id);
            }
            return;
        }
        let members: Vec<ResourceId> = match base {
            "cluster" => self.members(id, "cluster"),
            // A participant of a process standing for several resources: its members.
            "participant" => self.model.reference(Holder::Resource(id), "members").to_vec(),
            "pool" => self.members(id, "pool"),
            _ => self.model.reference(Holder::Resource(id), "on").to_vec(),
        };
        for m in members {
            self.collect_machines(m, depth + 1, out);
        }
    }

    /// The servers of a cluster or the members of a pool: what designates it by `key`.
    fn members(&self, group: ResourceId, key: &str) -> Vec<ResourceId> {
        self.registry
            .ids()
            .filter(|&r| self.base(r) == "server" && self.model.reference(Holder::Resource(r), key).contains(&group))
            .collect()
    }

    /// The physical devices a machine is cabled through: itself, or the hosts of a virtual one.
    pub fn cabled(&self, machine: ResourceId) -> Vec<ResourceId> {
        let mut out = Vec::new();
        let mut todo = vec![(machine, 0)];
        while let Some((m, depth)) = todo.pop() {
            let resource = self.registry.resource(m);
            let hosts = self.model.reference(Holder::Resource(m), "on");
            if resource.is_virtual() && !hosts.is_empty() && depth < 8 {
                for &h in hosts {
                    match self.base(h) {
                        "cluster" => todo.extend(self.members(h, "cluster").into_iter().map(|s| (s, depth + 1))),
                        _ => todo.push((h, depth + 1)),
                    }
                }
            } else if !out.contains(&m) {
                out.push(m);
            }
        }
        out
    }

    /// The subnet of a machine: the one it is placed in, otherwise the narrowest holding its
    /// address.
    pub fn subnet(&self, machine: ResourceId) -> Option<ResourceId> {
        if let Some(s) =
            std::iter::once(machine).chain(self.registry.ancestors(machine)).find(|&a| self.base(a) == "subnet")
        {
            return Some(s);
        }
        let ip = self.registry.resource(machine).attribute("ip").and_then(|a| net::parse_ip(&a.value.raw))?;
        self.subnets.iter().filter(|(_, c)| c.contains(ip)).max_by_key(|(_, c)| c.prefix).map(|&(s, _)| s)
    }

    /// The gateways of a subnet: its `gateway`, otherwise the routers and firewalls with an
    /// interface in it (by `subnet` or by address).
    pub fn gateways(&self, subnet: ResourceId) -> Vec<ResourceId> {
        if let Some(&g) = self.model.reference(Holder::Resource(subnet), "gateway").first() {
            return vec![self.device(g)];
        }
        self.routers_in(subnet)
    }

    /// The routers and firewalls with an interface in a subnet.
    fn routers_in(&self, subnet: ResourceId) -> Vec<ResourceId> {
        let cidr = self.subnets.iter().find(|(s, _)| *s == subnet).map(|&(_, c)| c);
        let mut out: Vec<ResourceId> = Vec::new();
        for (&device, plugs) in &self.plugs {
            if !ROUTERS.contains(&self.base(device)) {
                continue;
            }
            let inside = plugs.iter().any(|&p| {
                self.model.reference(Holder::Resource(p), "subnet").contains(&subnet)
                    || cidr.is_some_and(|c| {
                        self.registry
                            .resource(p)
                            .attribute("ip")
                            .and_then(|a| net::parse_ip(&a.value.raw))
                            .is_some_and(|ip| c.contains(ip))
                    })
            });
            if inside {
                out.push(device);
            }
        }
        out.sort();
        out
    }

    fn vlan_of(&self, subnet: Option<ResourceId>) -> Option<i64> {
        subnet.and_then(|s| self.number(s, "vlan").first().copied())
    }

    /// The layer 2 path from devices to the nearest of targets, through switches carrying the
    /// VLAN of the subnet: its steps exclude the ends; the targets reached, the cables taken.
    fn layer2(
        &self,
        from: &[ResourceId],
        targets: &[ResourceId],
        subnet: Option<ResourceId>,
    ) -> Result<(Path, Vec<ResourceId>), NoPath> {
        if let Some(&both) = from.iter().find(|f| targets.contains(f)) {
            return Ok((Path::default(), vec![both]));
        }
        let vlan = self.vlan_of(subnet);
        let mut level: HashMap<ResourceId, usize> = from.iter().map(|&f| (f, 0)).collect();
        // Device -> the (previous device, cable) pairs reaching it at its level.
        let mut parents: HashMap<ResourceId, Vec<(ResourceId, Cable)>> = HashMap::new();
        let mut queue: VecDeque<ResourceId> = from.iter().copied().collect();
        let mut found: Vec<ResourceId> = Vec::new();
        let mut found_at = usize::MAX;
        let mut farthest: Vec<ResourceId> = from.to_vec();
        let mut farthest_at = 0;
        while let Some(device) = queue.pop_front() {
            let at = level[&device];
            if at >= found_at {
                break;
            }
            // Only the ends and the switches pass the traffic on.
            if at > 0 && !SWITCHES.contains(&self.base(device)) {
                continue;
            }
            for &plug in self.plugs.get(&device).into_iter().flatten() {
                let Some(&other) = self.cables.get(&plug) else { continue };
                if !self.carries(plug, vlan) || !self.carries(other, vlan) {
                    continue;
                }
                let next = self.device(other);
                match level.get(&next) {
                    Some(&l) if l == at + 1 => parents.entry(next).or_default().push((device, (plug, other))),
                    Some(_) => {}
                    None => {
                        level.insert(next, at + 1);
                        parents.entry(next).or_default().push((device, (plug, other)));
                        if targets.contains(&next) {
                            found.push(next);
                            found_at = at + 1;
                        } else {
                            queue.push_back(next);
                        }
                        if at + 1 > farthest_at && SWITCHES.contains(&self.base(next)) {
                            farthest_at = at + 1;
                            farthest.clear();
                        }
                        if at + 1 == farthest_at && SWITCHES.contains(&self.base(next)) {
                            farthest.push(next);
                        }
                    }
                }
            }
        }
        if found.is_empty() {
            farthest.sort();
            return Err(NoPath::Stops { at: farthest, subnet });
        }
        // Back from the targets: the devices of each level on a shortest way, the cables taken.
        let mut steps: Vec<BTreeSet<ResourceId>> = vec![BTreeSet::new(); found_at.saturating_sub(1)];
        let mut cables = Vec::new();
        let mut seen: HashSet<ResourceId> = HashSet::new();
        let mut todo: Vec<ResourceId> = found.clone();
        while let Some(device) = todo.pop() {
            if !seen.insert(device) {
                continue;
            }
            for &(previous, cable) in parents.get(&device).into_iter().flatten() {
                cables.push(cable);
                let l = level[&previous];
                if l > 0 {
                    steps[l - 1].insert(previous);
                }
                todo.push(previous);
            }
        }
        cables.sort();
        cables.dedup();
        found.sort();
        let steps = steps.into_iter().map(|s| s.into_iter().collect()).collect();
        Ok((Path { steps, cables, connectors: Vec::new(), internet: Vec::new() }, found))
    }

    /// The path between two resources: from the machines they run on, through the cabling; to
    /// an end reached over the internet, through the `egress` of the other one, from it through
    /// the `ingress` of the other one.
    pub fn between(&self, a: ResourceId, b: ResourceId) -> Result<Path, NoPath> {
        match (over_internet(self.registry, self.model, a), over_internet(self.registry, self.model, b)) {
            (true, true) => return Err(NoPath::Outside),
            (false, true) => return self.internet_way(a, b, "egress"),
            (true, false) => return self.internet_way(b, a, "ingress"),
            (false, false) => {}
        }
        self.cabled_between(a, b)
    }

    /// Between an internal resource and one reached over the internet: to the internet, the way
    /// to the first device of the `egress` of the internal one, then the `egress`; from it, the
    /// `ingress`, then the way from its last device.
    fn internet_way(&self, inside: ResourceId, outside: ResourceId, key: &'static str) -> Result<Path, NoPath> {
        let (_, items) =
            nearest(self.registry, self.model, inside, key).ok_or(NoPath::NoEgress { end: inside, key })?;
        let egress = key == "egress";
        let device = if egress { items[0] } else { items[items.len() - 1] };
        let way = self.towards(inside, device)?;
        let inner: Vec<Vec<ResourceId>> = way.steps.into_iter().filter(|s| !s.contains(&device)).collect();
        let mut internet: Vec<(ResourceId, ResourceId)> = items.windows(2).map(|w| (w[0], w[1])).collect();
        let mut steps: Vec<Vec<ResourceId>> = Vec::new();
        if egress {
            steps.extend(inner);
            steps.extend(items.iter().map(|&i| vec![i]));
            internet.push((items[items.len() - 1], outside));
        } else {
            steps.extend(items.iter().map(|&i| vec![i]));
            steps.extend(inner.into_iter().rev());
            internet.insert(0, (outside, items[0]));
        }
        Ok(Path { steps, cables: way.cables, connectors: way.connectors, internet })
    }

    /// The path between two internal resources, through the cabling.
    fn cabled_between(&self, a: ResourceId, b: ResourceId) -> Result<Path, NoPath> {
        let (ma, mb) = (self.machines(a), self.machines(b));
        let (Some(&first_a), Some(&first_b)) = (ma.first(), mb.first()) else {
            return Err(NoPath::NoMachine(if ma.is_empty() { a } else { b }));
        };
        let from: Vec<ResourceId> = ma.iter().flat_map(|&m| self.cabled(m)).collect();
        let to: Vec<ResourceId> = mb.iter().flat_map(|&m| self.cabled(m)).collect();
        let sa = self.subnet(first_a).ok_or(NoPath::NoSubnet(first_a))?;
        let sb = self.subnet(first_b).ok_or(NoPath::NoSubnet(first_b))?;
        if sa == sb {
            return self.layer2(&from, &to, Some(sa)).map(|(p, _)| p);
        }
        self.routed(&from, sa, &to, sb)
    }

    /// The path from devices to a device (an `egress` device): in the subnet if it is there,
    /// otherwise through the gateway.
    pub fn towards(&self, a: ResourceId, device: ResourceId) -> Result<Path, NoPath> {
        let ma = self.machines(a);
        let Some(&first) = ma.first() else { return Err(NoPath::NoMachine(a)) };
        let from: Vec<ResourceId> = ma.iter().flat_map(|&m| self.cabled(m)).collect();
        let subnet = self.subnet(first);
        if let Ok((path, _)) = self.layer2(&from, &[device], subnet) {
            return Ok(path);
        }
        let sa = subnet.ok_or(NoPath::NoSubnet(first))?;
        let gateways = self.gateways(sa);
        if gateways.is_empty() {
            return Err(NoPath::NoGateway(sa));
        }
        let (up, reached) = self.layer2(&from, &gateways, Some(sa))?;
        let mut path = up;
        path.steps.push(reached.clone());
        if !reached.contains(&device) {
            let (on, _) = self.layer2(&reached, &[device], None)?;
            path = path.then(on);
        }
        Ok(path)
    }

    /// From a subnet to another: up to a gateway, from router to router, down to the target.
    fn routed(&self, from: &[ResourceId], sa: ResourceId, to: &[ResourceId], sb: ResourceId) -> Result<Path, NoPath> {
        let (ga, gb) = (self.gateways(sa), self.gateways(sb));
        if ga.is_empty() {
            return Err(NoPath::NoGateway(sa));
        }
        if gb.is_empty() {
            return Err(NoPath::NoGateway(sb));
        }
        let (up, first) = self.layer2(from, &ga, Some(sa))?;
        let (hops, last, connectors) = self.route(&first, &gb).ok_or(NoPath::NoRoute(sa, sb))?;
        let (down, _) = self.layer2(&last, to, Some(sb))?;
        let mut path = up;
        path.steps.push(first);
        path.steps.extend(hops);
        if path.steps.last() != Some(&last) {
            path.steps.push(last);
        }
        path.connectors.extend(connectors);
        Ok(path.then(down))
    }

    /// Router to router: the shortest way from gateways to gateways, through the subnets they
    /// share and the connectors (`ends`) between them; the intermediate steps, the gateways
    /// reached, the connectors crossed.
    fn route(&self, from: &[ResourceId], targets: &[ResourceId]) -> Option<Route> {
        let reached: Vec<ResourceId> = from.iter().copied().filter(|f| targets.contains(f)).collect();
        if !reached.is_empty() {
            return Some((Vec::new(), reached, Vec::new()));
        }
        // Router -> its neighbours, with the connector between them if any.
        let mut neighbours: HashMap<ResourceId, Vec<(ResourceId, Option<ResourceId>)>> = HashMap::new();
        for &(subnet, _) in &self.subnets {
            let routers = self.routers_in(subnet);
            for &r in &routers {
                for &s in routers.iter().filter(|&&s| s != r) {
                    neighbours.entry(r).or_default().push((s, None));
                }
            }
        }
        for id in self.registry.ids() {
            let Some(key) = self.registry.schema.kind_def(&self.registry.resource(id).kind).and_then(|d| d.connector)
            else {
                continue;
            };
            if let [a, b] = self.model.reference(Holder::Resource(id), key) {
                let (a, b) = (self.device(*a), self.device(*b));
                neighbours.entry(a).or_default().push((b, Some(id)));
                neighbours.entry(b).or_default().push((a, Some(id)));
            }
        }
        let mut previous: HashMap<ResourceId, (ResourceId, Option<ResourceId>)> = HashMap::new();
        let mut queue: VecDeque<ResourceId> = from.iter().copied().collect();
        let mut seen: HashSet<ResourceId> = from.iter().copied().collect();
        while let Some(r) = queue.pop_front() {
            for &(n, connector) in neighbours.get(&r).into_iter().flatten() {
                if !seen.insert(n) {
                    continue;
                }
                previous.insert(n, (r, connector));
                if targets.contains(&n) {
                    let mut hops = Vec::new();
                    let mut connectors = Vec::new();
                    let mut current = n;
                    while let Some(&(p, c)) = previous.get(&current) {
                        if let Some(c) = c {
                            connectors.push(c);
                            hops.push(vec![c]);
                        }
                        if !from.contains(&p) {
                            hops.push(vec![p]);
                        }
                        current = p;
                    }
                    hops.reverse();
                    connectors.reverse();
                    return Some((hops, vec![n], connectors));
                }
                queue.push_back(n);
            }
        }
        None
    }

    /// The text of a missing path, for a diagnostic.
    pub fn explain(&self, error: &NoPath) -> String {
        let name = |id: ResourceId| self.registry.qualified_name(id);
        match error {
            NoPath::NoMachine(r) => format!("{} runs on no machine", name(*r)),
            NoPath::NoSubnet(m) => {
                format!("{} is in no subnet: neither placed in one nor with an address in one", name(*m))
            }
            NoPath::NoGateway(s) => {
                format!("no router nor firewall routes subnet {} (declare its `gateway`)", name(*s))
            }
            NoPath::NoRoute(a, b) => format!("no route between the gateways of {} and {}", name(*a), name(*b)),
            NoPath::NoEgress { end, key } => format!("{} has no `{key}` towards the internet", name(*end)),
            NoPath::Outside => "both ends are reached over the internet".to_owned(),
            NoPath::Stops { at, subnet } => {
                let vlan = self.vlan_of(*subnet).map(|v| format!(" in VLAN {v}")).unwrap_or_default();
                let devices: Vec<String> = at.iter().map(|&d| name(d)).collect();
                format!("the cables stop at {}{vlan}", devices.join(", "))
            }
        }
    }
}

/// The ambiguous gateways: subnets routed by several devices without a `gateway`.
pub fn ambiguous_gateways(network: &Network, path_subnets: &[ResourceId]) -> Vec<(ResourceId, Vec<ResourceId>)> {
    path_subnets
        .iter()
        .filter(|&&s| network.model.reference(Holder::Resource(s), "gateway").is_empty())
        .filter_map(|&s| {
            let g = network.routers_in(s);
            (g.len() > 1).then_some((s, g))
        })
        .collect()
}
