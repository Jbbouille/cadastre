//! A one-off import of a Terraform state into `.ca` files, to bootstrap a registry: the sites,
//! networks, subnets, servers, load balancers, gateways, clusters and databases the state
//! describes, to be reviewed and completed by hand (owners, flows).
//!
//! The input is the JSON of `terraform show -json`, or a `.tfstate` file (format 4). AWS, Azure and
//! Google Cloud resources are read; the others are listed at the end of the files, not imported.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write;

use serde_json::Value;

/// A resource of the state: its address (`module.net.aws_subnet.a[0]`), its type and its values.
struct State {
    address: String,
    kind: String,
    name: String,
    values: Value,
}

/// An imported file: its path, relative to the output folder, and its text.
#[derive(Clone, Debug)]
pub struct Imported {
    pub path: String,
    pub text: String,
}

/// The `.ca` files of a Terraform state, one per site, formatted; an error when the text is not a
/// Terraform state.
pub fn import(text: &str) -> Result<Vec<Imported>, String> {
    let json: Value = serde_json::from_str(text).map_err(|e| format!("not JSON: {e}"))?;
    let resources = read(&json)?;
    let mut tree = Tree::default();
    let mut skipped: Vec<String> = Vec::new();
    aws(&resources, &mut tree);
    azure(&resources, &mut tree);
    gcp(&resources, &mut tree);
    for resource in &resources {
        if !tree.imported.contains(&resource.address) {
            skipped.push(resource.address.clone());
        }
    }
    Ok(tree.files(&skipped))
}

/// The managed resources of a state, in either format.
fn read(json: &Value) -> Result<Vec<State>, String> {
    let mut out = Vec::new();
    if let Some(root) = json.pointer("/values/root_module") {
        // `terraform show -json`.
        fn module(node: &Value, out: &mut Vec<State>) {
            for r in node.get("resources").and_then(Value::as_array).into_iter().flatten() {
                if r.get("mode").and_then(Value::as_str) != Some("managed") {
                    continue;
                }
                out.push(State {
                    address: text(r, "address"),
                    kind: text(r, "type"),
                    name: text(r, "name"),
                    values: r.get("values").cloned().unwrap_or(Value::Null),
                });
            }
            for child in node.get("child_modules").and_then(Value::as_array).into_iter().flatten() {
                module(child, out);
            }
        }
        module(root, &mut out);
    } else if let Some(resources) = json.get("resources").and_then(Value::as_array) {
        // A state file, format 4.
        for r in resources {
            if r.get("mode").and_then(Value::as_str) != Some("managed") {
                continue;
            }
            let (kind, name) = (text(r, "type"), text(r, "name"));
            let prefix = r.get("module").and_then(Value::as_str).map(|m| format!("{m}.")).unwrap_or_default();
            for instance in r.get("instances").and_then(Value::as_array).into_iter().flatten() {
                let index = match instance.get("index_key") {
                    Some(Value::Number(n)) => format!("[{n}]"),
                    Some(Value::String(s)) => format!("[\"{s}\"]"),
                    _ => String::new(),
                };
                out.push(State {
                    address: format!("{prefix}{kind}.{name}{index}"),
                    kind: kind.clone(),
                    name: name.clone(),
                    values: instance.get("attributes").cloned().unwrap_or(Value::Null),
                });
            }
        }
    } else {
        return Err("not a Terraform state: neither `values.root_module` (terraform show -json) nor `resources`".into());
    }
    Ok(out)
}

/// The resources of a type.
fn of<'a>(resources: &'a [State], kind: &'a str) -> impl Iterator<Item = &'a State> + 'a {
    resources.iter().filter(move |r| r.kind == kind)
}

fn text(value: &Value, key: &str) -> String {
    value.get(key).and_then(Value::as_str).unwrap_or_default().to_owned()
}

fn strings(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect()
}

/// The name of a cloud resource: its `Name` tag, its name, otherwise the Terraform name.
fn display_name(resource: &State) -> String {
    let tag = resource.values.pointer("/tags/Name").and_then(Value::as_str).filter(|s| !s.is_empty());
    let name = resource.values.get("name").and_then(Value::as_str).filter(|s| !s.is_empty());
    tag.or(name).unwrap_or(&resource.name).to_owned()
}

/// A Cadastre identifier: lowercase letters, digits, `_` and single hyphens, not starting with a digit.
pub(crate) fn identifier(text: &str) -> String {
    let mut out = String::new();
    for c in text.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').to_owned();
    match out.chars().next() {
        None => "unnamed".to_owned(),
        Some(c) if c.is_ascii_digit() => format!("r-{out}"),
        _ => out,
    }
}

// ---------------------------------------------------------------------------
// The tree of resources
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Node {
    kind: String,
    name: String,
    /// The Terraform address, in a comment.
    address: Option<String>,
    attributes: Vec<(String, String)>,
    children: Vec<usize>,
    parent: Option<usize>,
    /// The file of a root node.
    file: String,
}

#[derive(Default)]
struct Tree {
    nodes: Vec<Node>,
    /// Sites by name.
    sites: HashMap<String, usize>,
    /// Cloud identifiers (`vpc-0a1b`, subnet ids) to nodes.
    by_cloud_id: HashMap<String, usize>,
    imported: HashSet<String>,
}

impl Tree {
    fn site(&mut self, name: &str, provider: &str, region: &str) -> usize {
        if let Some(&site) = self.sites.get(name) {
            return site;
        }
        let mut attributes =
            vec![("hosting".to_owned(), "cloud".to_owned()), ("provider".to_owned(), provider.to_owned())];
        if !region.is_empty() {
            attributes.push(("region".to_owned(), region.to_owned()));
        }
        let index = self.add(None, "site", name, None, attributes);
        self.nodes[index].file = format!("{name}.ca");
        self.sites.insert(name.to_owned(), index);
        index
    }

    /// Adds a node under a parent, its name unique among its siblings.
    fn add(
        &mut self,
        parent: Option<usize>,
        kind: &str,
        name: &str,
        address: Option<&State>,
        attributes: Vec<(String, String)>,
    ) -> usize {
        let siblings: HashSet<String> = match parent {
            Some(p) => self.nodes[p].children.iter().map(|&c| self.nodes[c].name.clone()).collect(),
            None => self.nodes.iter().filter(|n| n.parent.is_none()).map(|n| n.name.clone()).collect(),
        };
        let base = identifier(name);
        let mut unique = base.clone();
        let mut n = 2;
        while siblings.contains(&unique) {
            unique = format!("{base}-{n}");
            n += 1;
        }
        let index = self.nodes.len();
        let file = parent.map(|p| self.nodes[p].file.clone()).unwrap_or_default();
        self.nodes.push(Node {
            kind: kind.to_owned(),
            name: unique,
            address: address.map(|a| a.address.clone()),
            attributes,
            children: Vec::new(),
            parent,
            file,
        });
        if let Some(p) = parent {
            self.nodes[p].children.push(index);
        }
        if let Some(resource) = address {
            self.imported.insert(resource.address.clone());
            let id = text(&resource.values, "id");
            if !id.is_empty() {
                self.by_cloud_id.insert(id, index);
            }
        }
        index
    }

    fn qualified(&self, index: usize) -> String {
        let mut names = vec![self.nodes[index].name.clone()];
        let mut current = self.nodes[index].parent;
        while let Some(p) = current {
            names.push(self.nodes[p].name.clone());
            current = self.nodes[p].parent;
        }
        names.reverse();
        names.join(".")
    }

    /// One file per site, its root resources (databases) in it; the resources not imported listed
    /// in a comment.
    fn files(&self, skipped: &[String]) -> Vec<Imported> {
        let mut texts: BTreeMap<String, String> = BTreeMap::new();
        for (index, node) in self.nodes.iter().enumerate().filter(|(_, n)| n.parent.is_none()) {
            let out = texts.entry(node.file.clone()).or_insert_with(|| {
                "// Imported from a Terraform state: review and complete (owners, operators, flows).\n\n".to_owned()
            });
            self.write(index, 0, out);
            out.push('\n');
        }
        if !skipped.is_empty() {
            let out = texts.entry("not-imported.ca".to_owned()).or_default();
            out.push_str("// Resources of the Terraform state not imported:\n");
            for address in skipped {
                let _ = writeln!(out, "//   {address}");
            }
        }
        texts
            .into_iter()
            .map(|(path, text)| Imported { text: crate::format::format(&text).unwrap_or(text), path })
            .collect()
    }

    fn write(&self, index: usize, depth: usize, out: &mut String) {
        let node = &self.nodes[index];
        let indent = "  ".repeat(depth);
        if let Some(address) = &node.address {
            let _ = writeln!(out, "{indent}// {address}");
        }
        let _ = writeln!(out, "{indent}{} {} {{", node.kind, node.name);
        for (key, value) in &node.attributes {
            let _ = writeln!(out, "{indent}  {key}: {}", if is_text(key) { quoted(value) } else { literal(value) });
        }
        for &child in &node.children {
            self.write(child, depth + 1, out);
        }
        let _ = writeln!(out, "{indent}}}");
    }
}

/// Whether the schema types a key as a text, always quoted.
fn is_text(key: &str) -> bool {
    crate::schema::KINDS.iter().flat_map(|k| k.attrs.iter()).chain(crate::schema::RESOURCE_KEYS).any(|a| {
        a.key == key && matches!(a.ty, crate::schema::Ty::String | crate::schema::Ty::Strings | crate::schema::Ty::Url)
    })
}

pub(crate) fn quoted(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// A value as written: bare when it is an identifier, a name, an address or a CIDR, a string otherwise.
pub(crate) fn literal(value: &str) -> String {
    let bare = !value.is_empty()
        && (value.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | ':'))
            && value.chars().next().is_some_and(|c| c.is_ascii_alphanumeric()))
        && !value.ends_with('-')
        && (crate::net::parse_ip(value).is_some()
            || crate::net::parse_cidr(value).is_some()
            || value.split('.').all(|s| {
                !s.is_empty() && s.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) && !s.contains(['/', ':'])
            }));
    if bare { value.to_owned() } else { quoted(value) }
}

fn attrs(pairs: &[(&str, String)]) -> Vec<(String, String)> {
    pairs.iter().filter(|(_, v)| !v.is_empty()).map(|(k, v)| ((*k).to_owned(), v.clone())).collect()
}

// ---------------------------------------------------------------------------
// AWS
// ---------------------------------------------------------------------------

/// The region and the account of an ARN: `arn:aws:ec2:eu-west-3:111111111111:vpc/vpc-0a1b`.
fn arn(resource: &State) -> (String, String) {
    let arn = text(&resource.values, "arn");
    let parts: Vec<&str> = arn.split(':').collect();
    (parts.get(3).copied().unwrap_or_default().to_owned(), parts.get(4).copied().unwrap_or_default().to_owned())
}

fn aws(resources: &[State], tree: &mut Tree) {
    // A site per region, per account when the state spans several.
    let accounts: HashSet<String> = of(resources, "aws_vpc").map(|r| arn(r).1).filter(|a| !a.is_empty()).collect();
    for vpc in of(resources, "aws_vpc") {
        let (region, account) = arn(vpc);
        let site_name = match (accounts.len() > 1, region.is_empty()) {
            (true, _) => format!("aws-{account}-{region}"),
            (false, false) => format!("aws-{region}"),
            (false, true) => "aws".to_owned(),
        };
        let site = tree.site(&site_name, "aws", &region);
        let attributes = attrs(&[("cidr", text(&vpc.values, "cidr_block")), ("cloud_id", text(&vpc.values, "id"))]);
        // `cidr` is not a key of a VPC: the label keeps the id, the CIDR goes to its description.
        let attributes: Vec<(String, String)> = attributes
            .into_iter()
            .map(|(k, v)| if k == "cidr" { ("description".to_owned(), format!("CIDR {v}")) } else { (k, v) })
            .collect();
        tree.add(Some(site), "vpc", &display_name(vpc), Some(vpc), attributes);
    }
    for subnet in of(resources, "aws_subnet") {
        let Some(&vpc) = tree.by_cloud_id.get(&text(&subnet.values, "vpc_id")) else { continue };
        let attributes =
            attrs(&[("cidr", text(&subnet.values, "cidr_block")), ("az", text(&subnet.values, "availability_zone"))]);
        tree.add(Some(vpc), "subnet", &display_name(subnet), Some(subnet), attributes);
    }
    for gateway in of(resources, "aws_internet_gateway") {
        let Some(&vpc) = tree.by_cloud_id.get(&text(&gateway.values, "vpc_id")) else { continue };
        let attributes = attrs(&[("cloud_id", text(&gateway.values, "id")), ("role", "internet gateway".to_owned())]);
        tree.add(Some(vpc), "router", &display_name(gateway), Some(gateway), attributes);
    }
    for nat in of(resources, "aws_nat_gateway") {
        let Some(&subnet) = tree.by_cloud_id.get(&text(&nat.values, "subnet_id")) else { continue };
        let vpc = tree.nodes[subnet].parent.unwrap_or(subnet);
        let attributes = attrs(&[
            ("cloud_id", text(&nat.values, "id")),
            ("public_ip", text(&nat.values, "public_ip")),
            ("role", "NAT gateway".to_owned()),
        ]);
        tree.add(Some(vpc), "router", &display_name(nat), Some(nat), attributes);
    }
    for instance in of(resources, "aws_instance") {
        let Some(&subnet) = tree.by_cloud_id.get(&text(&instance.values, "subnet_id")) else { continue };
        let attributes = attrs(&[
            ("ip", text(&instance.values, "private_ip")),
            ("instance_type", text(&instance.values, "instance_type")),
            ("cloud_id", text(&instance.values, "id")),
        ]);
        tree.add(Some(subnet), "virtual server", &display_name(instance), Some(instance), attributes);
    }
    for kind in ["aws_lb", "aws_alb", "aws_elb"] {
        for lb in of(resources, kind) {
            let subnet = strings(&lb.values, "subnets").into_iter().find_map(|s| tree.by_cloud_id.get(&s).copied());
            let Some(subnet) = subnet else { continue };
            let vpc = tree.nodes[subnet].parent.unwrap_or(subnet);
            let internal = lb.values.get("internal").and_then(Value::as_bool).unwrap_or(false);
            let attributes = attrs(&[
                ("dns_name", text(&lb.values, "dns_name")),
                ("scope", if internal { String::new() } else { "external".to_owned() }),
            ]);
            // An internet-facing load balancer is reached from outside: `scope` would mean the
            // resource is external; its exposure goes to the description instead.
            let attributes: Vec<(String, String)> = attributes
                .into_iter()
                .map(
                    |(k, v)| {
                        if k == "scope" { ("description".to_owned(), "internet-facing".to_owned()) } else { (k, v) }
                    },
                )
                .collect();
            tree.add(Some(vpc), "load_balancer", &display_name(lb), Some(lb), attributes);
        }
    }
    for cluster in of(resources, "aws_eks_cluster") {
        let subnets =
            cluster.values.pointer("/vpc_config/0/subnet_ids").and_then(Value::as_array).cloned().unwrap_or_default();
        let subnet = subnets.iter().filter_map(Value::as_str).find_map(|s| tree.by_cloud_id.get(s).copied());
        let Some(subnet) = subnet else { continue };
        let vpc = tree.nodes[subnet].parent.unwrap_or(subnet);
        let attributes = attrs(&[("platform", "eks".to_owned()), ("version", text(&cluster.values, "version"))]);
        let attributes: Vec<(String, String)> = attributes
            .into_iter()
            .map(|(k, v)| if k == "version" { ("kubernetes_version".to_owned(), v) } else { (k, v) })
            .collect();
        tree.add(Some(vpc), "cluster", &display_name(cluster), Some(cluster), attributes);
    }
    // Databases: at the root, on the VPC of their subnet group.
    let groups: HashMap<String, Vec<String>> = of(resources, "aws_db_subnet_group")
        .map(|g| (text(&g.values, "name"), strings(&g.values, "subnet_ids")))
        .collect();
    for group in of(resources, "aws_db_subnet_group") {
        tree.imported.insert(group.address.clone());
    }
    for db in of(resources, "aws_db_instance").chain(of(resources, "aws_rds_cluster")) {
        let group = text(&db.values, "db_subnet_group_name");
        let vpc = groups
            .get(&group)
            .into_iter()
            .flatten()
            .find_map(|s| tree.by_cloud_id.get(s).copied())
            .and_then(|subnet| tree.nodes[subnet].parent);
        let Some(vpc) = vpc else { continue };
        let site = tree.nodes[vpc].parent.unwrap_or(vpc);
        let on = tree.qualified(vpc);
        let version = text(&db.values, "engine_version");
        let attributes = attrs(&[
            ("on", on),
            ("engine", text(&db.values, "engine")),
            ("version", version),
            ("managed", "rds".to_owned()),
        ]);
        let index = tree.add(None, "database", &display_name(db), Some(db), attributes);
        tree.nodes[index].file = tree.nodes[site].file.clone();
    }
}

// ---------------------------------------------------------------------------
// Azure
// ---------------------------------------------------------------------------

fn azure(resources: &[State], tree: &mut Tree) {
    let mut networks: HashMap<String, usize> = HashMap::new();
    for network in of(resources, "azurerm_virtual_network") {
        let location = text(&network.values, "location");
        let site = tree.site(&format!("azure-{location}"), "azure", &location);
        let space = strings(&network.values, "address_space").join(", ");
        let attributes =
            attrs(&[("description", if space.is_empty() { space } else { format!("address space {space}") })]);
        let index = tree.add(Some(site), "vpc", &display_name(network), Some(network), attributes);
        networks.insert(
            format!("{}/{}", text(&network.values, "resource_group_name"), text(&network.values, "name")),
            index,
        );
    }
    for subnet in of(resources, "azurerm_subnet") {
        let key =
            format!("{}/{}", text(&subnet.values, "resource_group_name"), text(&subnet.values, "virtual_network_name"));
        let Some(&network) = networks.get(&key) else { continue };
        let cidr = strings(&subnet.values, "address_prefixes")
            .into_iter()
            .next()
            .unwrap_or_else(|| text(&subnet.values, "address_prefix"));
        tree.add(Some(network), "subnet", &display_name(subnet), Some(subnet), attrs(&[("cidr", cidr)]));
    }
    // Network interfaces give the subnet and the address of the machines.
    let interfaces: HashMap<String, (String, String)> = of(resources, "azurerm_network_interface")
        .map(|nic| {
            let config = nic.values.pointer("/ip_configuration/0").cloned().unwrap_or(Value::Null);
            (text(&nic.values, "id"), (text(&config, "subnet_id"), text(&config, "private_ip_address")))
        })
        .collect();
    for nic in of(resources, "azurerm_network_interface") {
        tree.imported.insert(nic.address.clone());
    }
    for vm in of(resources, "azurerm_linux_virtual_machine").chain(of(resources, "azurerm_windows_virtual_machine")) {
        let Some((subnet, ip)) =
            strings(&vm.values, "network_interface_ids").into_iter().find_map(|id| interfaces.get(&id).cloned())
        else {
            continue;
        };
        let Some(&subnet) = tree.by_cloud_id.get(&subnet) else { continue };
        let attributes = attrs(&[("ip", ip), ("size", text(&vm.values, "size"))]);
        tree.add(Some(subnet), "virtual server", &display_name(vm), Some(vm), attributes);
    }
}

// ---------------------------------------------------------------------------
// Google Cloud
// ---------------------------------------------------------------------------

fn gcp(resources: &[State], tree: &mut Tree) {
    let mut networks: HashMap<String, usize> = HashMap::new();
    for network in of(resources, "google_compute_network") {
        let project = text(&network.values, "project");
        let site = tree.site(&if project.is_empty() { "gcp".to_owned() } else { format!("gcp-{project}") }, "gcp", "");
        let index = tree.add(Some(site), "vpc", &display_name(network), Some(network), Vec::new());
        networks.insert(text(&network.values, "self_link"), index);
        networks.insert(text(&network.values, "name"), index);
    }
    for subnet in of(resources, "google_compute_subnetwork") {
        let network = text(&subnet.values, "network");
        let parent = networks.get(&network).or_else(|| networks.get(network.rsplit('/').next().unwrap_or_default()));
        let Some(&parent) = parent else { continue };
        let attributes =
            attrs(&[("cidr", text(&subnet.values, "ip_cidr_range")), ("region", text(&subnet.values, "region"))]);
        let index = tree.add(Some(parent), "subnet", &display_name(subnet), Some(subnet), attributes);
        tree.by_cloud_id.insert(text(&subnet.values, "self_link"), index);
        tree.by_cloud_id.insert(text(&subnet.values, "name"), index);
    }
    for instance in of(resources, "google_compute_instance") {
        let nic = instance.values.pointer("/network_interface/0").cloned().unwrap_or(Value::Null);
        let subnetwork = text(&nic, "subnetwork");
        let subnet = tree
            .by_cloud_id
            .get(&subnetwork)
            .or_else(|| tree.by_cloud_id.get(subnetwork.rsplit('/').next().unwrap_or_default()));
        let Some(&subnet) = subnet else { continue };
        let attributes =
            attrs(&[("ip", text(&nic, "network_ip")), ("machine_type", text(&instance.values, "machine_type"))]);
        tree.add(Some(subnet), "virtual server", &display_name(instance), Some(instance), attributes);
    }
}
