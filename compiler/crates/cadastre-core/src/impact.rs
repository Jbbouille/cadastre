//! Impact analysis: for each resource, the resources whose failure alone makes it unavailable
//! (`failures` in filters).
//!
//! A failure spreads:
//! - to what a resource contains (a site, a subnet, a server and its interfaces), and to what is
//!   in a rack (`rack:`);
//! - to what runs on it (`on`), once every host of a resource is down: `on: [web01, web02]`
//!   survives one of them;
//! - to a cluster or a pool, once all its members are down;
//! - to a tunnel or a connector, when one of its ends is down;
//! - along the links: the source of a link depends on its target and on the devices and
//!   tunnels it goes through (`through`).
//!
//! Logical resources (systems, teams, actors, data) do not fail by themselves.
//!
//! `failures(hosting)` leaves the links out: what a resource needs to run, not what it calls.

use std::collections::{BTreeSet, HashMap};

use crate::check::{Holder, Model};
use crate::registry::{Registry, ResourceId};

/// Kinds that do not fail by themselves.
const LOGICAL: &[&str] = &["system", "team", "actor", "data"];

/// What a resource needs to be available.
#[derive(Default)]
struct Needs {
    /// Down if any of these is down: parent, rack, connector ends, link targets and devices.
    all: Vec<ResourceId>,
    /// Groups of alternatives: down if every resource of a group is down (hosts, members).
    any: Vec<Vec<ResourceId>>,
}

/// For each resource, the other resources whose failure alone makes it unavailable, sorted;
/// without `links`, through hosting only.
pub fn failures(registry: &Registry, model: &Model, links: bool) -> HashMap<ResourceId, Vec<ResourceId>> {
    let dependencies = Dependencies::new(registry, model, links);
    let mut result: HashMap<ResourceId, Vec<ResourceId>> = HashMap::new();
    for seed in registry.ids().filter(|&id| !LOGICAL.contains(&registry.resource(id).kind.as_str())) {
        let down = dependencies.down(&[seed]);
        for id in registry.ids().filter(|&id| id != seed && down[id.0 as usize]) {
            result.entry(id).or_default().push(seed);
        }
    }
    for list in result.values_mut() {
        let sorted: BTreeSet<ResourceId> = list.drain(..).collect();
        list.extend(sorted);
    }
    result
}

/// For each resource, the pairs of resources whose failure together, and not of either alone,
/// makes it unavailable (`failures(pairs)`), links included; each pair sorted, the pairs sorted.
///
/// A pair matters through a group of alternatives (the hosts of an `on`, the members of a
/// cluster) that neither of its two resources takes down alone: the pairs tried are those whose
/// failures, each alone, cover such a group together; then what they take down together and
/// neither alone. A pair whose failures contain those of another pair is left out.
pub fn failure_pairs(registry: &Registry, model: &Model) -> HashMap<ResourceId, Vec<(ResourceId, ResourceId)>> {
    let dependencies = Dependencies::new(registry, model, true);
    let count = registry.resources.len();
    let fails = |id: ResourceId| !LOGICAL.contains(&registry.resource(id).kind.as_str());
    // What each failure takes down alone, and who takes each resource down.
    let mut alone: Vec<Vec<ResourceId>> = vec![Vec::new(); count];
    let mut killers: Vec<Vec<ResourceId>> = vec![Vec::new(); count];
    for seed in registry.ids().filter(|&id| fails(id)) {
        let down = dependencies.down(&[seed]);
        let taken: Vec<ResourceId> = registry.ids().filter(|&id| down[id.0 as usize]).collect();
        for &id in &taken {
            killers[id.0 as usize].push(seed);
        }
        alone[seed.0 as usize] = taken;
    }
    let takes = |a: ResourceId, b: ResourceId| alone[a.0 as usize].binary_search(&b).is_ok();
    // The pairs covering a group of alternatives, neither alone.
    let mut pairs: BTreeSet<(ResourceId, ResourceId)> = BTreeSet::new();
    for needs in &dependencies.needs {
        for group in needs.any.iter().filter(|g| g.len() >= 2) {
            for &a in &killers[group[0].0 as usize] {
                let rest: Vec<ResourceId> = group.iter().copied().filter(|&m| !takes(a, m)).collect();
                let Some(first) = rest.first() else { continue };
                for &b in &killers[first.0 as usize] {
                    if b != a && rest.iter().all(|&m| takes(b, m)) && !group.iter().all(|&m| takes(b, m)) {
                        pairs.insert((a.min(b), a.max(b)));
                    }
                }
            }
        }
    }
    let mut result: HashMap<ResourceId, Vec<(ResourceId, ResourceId)>> = HashMap::new();
    for (a, b) in pairs {
        let down = dependencies.down(&[a, b]);
        for id in registry.ids().filter(|&id| down[id.0 as usize] && !takes(a, id) && !takes(b, id)) {
            result.entry(id).or_default().push((a, b));
        }
    }
    // The most precise pairs only: a site and a server are left out when the rack of the site and
    // the server are a pair too, the failure of the site taking the rack down.
    let within = |big: ResourceId, small: ResourceId| big == small || takes(big, small);
    let covers = |p: (ResourceId, ResourceId), q: (ResourceId, ResourceId)| {
        p != q && ((within(p.0, q.0) && within(p.1, q.1)) || (within(p.0, q.1) && within(p.1, q.0)))
    };
    for list in result.values_mut() {
        let all = list.clone();
        list.retain(|&p| !all.iter().any(|&q| covers(p, q)));
    }
    result
}

/// The resources unavailable when these fail together (`failing: [fw01, fw02]` in a view or a
/// check), those included; a logical resource (a system) fails as what it contains.
pub fn down(registry: &Registry, model: &Model, failing: &[ResourceId]) -> BTreeSet<ResourceId> {
    let seeds = failing_seeds(registry, failing);
    if seeds.is_empty() {
        return BTreeSet::new();
    }
    let down = Dependencies::new(registry, model, true).down(&seeds);
    registry.ids().filter(|&id| down[id.0 as usize]).collect()
}

/// The resources that fail: those named, a logical one replaced by what it contains.
pub fn failing_seeds(registry: &Registry, failing: &[ResourceId]) -> Vec<ResourceId> {
    let logical = |id: ResourceId| LOGICAL.contains(&registry.resource(id).kind.as_str());
    let mut seeds = Vec::new();
    for &id in failing {
        if logical(id) {
            seeds.extend(registry.descendants(id).filter(|&d| !logical(d)));
        } else {
            seeds.push(id);
        }
    }
    seeds
}

/// What each resource needs, and who to look at when a resource goes down.
struct Dependencies {
    needs: Vec<Needs>,
    dependents: Vec<Vec<ResourceId>>,
}

impl Dependencies {
    fn new(registry: &Registry, model: &Model, links: bool) -> Self {
        let count = registry.resources.len();
        let mut needs: Vec<Needs> = (0..count).map(|_| Needs::default()).collect();
        for id in registry.ids() {
            let resource = registry.resource(id);
            let needed = &mut needs[id.0 as usize];
            needed.all.extend(resource.parent.filter(|&p| !LOGICAL.contains(&registry.resource(p).kind.as_str())));
            needed.all.extend(model.reference(Holder::Resource(id), "rack").iter().copied());
            if let Some(key) = registry.schema.kind_def(&resource.kind).and_then(|d| d.connector) {
                needed.all.extend(model.reference(Holder::Resource(id), key).iter().copied());
            }
            let hosts = model.reference(Holder::Resource(id), "on");
            if !hosts.is_empty() {
                needed.any.push(hosts.to_vec());
            }
        }
        // A cluster or a pool needs one of its members.
        let mut members: HashMap<ResourceId, Vec<ResourceId>> = HashMap::new();
        for id in registry.ids() {
            for key in ["cluster", "pool"] {
                for &group in model.reference(Holder::Resource(id), key) {
                    members.entry(group).or_default().push(id);
                }
            }
        }
        for (group, list) in members {
            needs[group.0 as usize].any.push(list);
        }
        // The source of a link needs its target and what the link goes through.
        for (index, ends) in model.link_ends.iter().enumerate().filter(|_| links) {
            let Some((source, target)) = *ends else { continue };
            if source == target {
                continue;
            }
            let needed = &mut needs[source.0 as usize];
            needed.all.push(target);
            needed.all.extend(model.reference(Holder::Link(index), "through").iter().copied());
        }

        // Who to look at when a resource goes down.
        let mut dependents: Vec<Vec<ResourceId>> = vec![Vec::new(); count];
        for id in registry.ids() {
            let needed = &needs[id.0 as usize];
            for &n in needed.all.iter().chain(needed.any.iter().flatten()) {
                dependents[n.0 as usize].push(id);
            }
        }

        Dependencies { needs, dependents }
    }

    /// The resources down, by index, when the seeds fail together.
    fn down(&self, seeds: &[ResourceId]) -> Vec<bool> {
        let mut down = vec![false; self.needs.len()];
        for seed in seeds {
            down[seed.0 as usize] = true;
        }
        let mut queue = seeds.to_vec();
        while let Some(failed) = queue.pop() {
            for &dependent in &self.dependents[failed.0 as usize] {
                if down[dependent.0 as usize] {
                    continue;
                }
                let needed = &self.needs[dependent.0 as usize];
                let is_down = |r: &ResourceId| down[r.0 as usize];
                if needed.all.iter().any(is_down) || needed.any.iter().any(|group| group.iter().all(is_down)) {
                    down[dependent.0 as usize] = true;
                    queue.push(dependent);
                }
            }
        }
        down
    }
}
