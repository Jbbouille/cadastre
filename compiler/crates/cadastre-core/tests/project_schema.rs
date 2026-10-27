//! The schema of a project: typed keys added to the built-in kinds, kinds of the project `like`
//! a built-in one.

use cadastre_core::{Compilation, Severity, SourceFile, build, compile};

const SCHEMA: &str = r#"
kind server {
  os: string
  ram: number
  hypervisor: vmware | hyperv
  vmware_cluster: ref(cluster)
  stretched_to: ref(site)*
  inventory_id: pattern("INV-[0-9]+") (required)
}
kind app { catalog_id: pattern("APP-\d{3}") }
kind printer { like: device, parents: site | subnet, serial: string }
"#;

fn compilation(registry: &str, views: &str) -> Compilation {
    compile(&[
        SourceFile { path: "schema.ca".into(), text: SCHEMA.into() },
        SourceFile { path: "a.ca".into(), text: registry.into() },
        SourceFile { path: "v.cav".into(), text: views.into() },
    ])
}

fn messages(c: &Compilation, severity: Severity) -> Vec<String> {
    c.diagnostics.iter().filter(|d| d.severity == severity).map(|d| d.message.clone()).collect()
}

const REGISTRY: &str = r#"
team t
site dc {
  hosting: on-prem
  network lan {
    subnet s {
      cidr: 10.0.0.0/24
      server web01 { owner: t, os: "linux", ram: 16, hypervisor: vmware, inventory_id: "INV-42" }
      printer p1 { serial: "SN1" }
    }
  }
  printer p2
}
app crm { owner: t, on: web01, catalog_id: "APP-123" }
"#;

#[test]
fn typed_keys_are_checked() {
    let c = compilation(REGISTRY, "");
    assert!(messages(&c, Severity::Error).is_empty(), "{:?}", messages(&c, Severity::Error));
    let wrong = REGISTRY
        .replace("ram: 16", "ram: \"big\"")
        .replace("hypervisor: vmware", "hypervisor: kvm")
        .replace("\"INV-42\"", "\"X-1\"")
        .replace("\"APP-123\"", "\"APP-12\"");
    assert_eq!(
        messages(&compilation(&wrong, ""), Severity::Error),
        [
            r#"invalid value for ram: "big" (a number expected)"#,
            "invalid value for hypervisor: kvm",
            r#"invalid value for inventory_id: "X-1""#,
            r#"invalid value for catalog_id: "APP-12""#,
        ]
    );
    // A required key missing: the server is incomplete.
    let missing = REGISTRY.replace(", inventory_id: \"INV-42\"", "");
    assert!(
        messages(&compilation(&missing, ""), Severity::Incomplete)
            .contains(&"server dc.lan.s.web01: missing attribute inventory_id".to_owned())
    );
}

#[test]
fn a_kind_of_the_project_is_like_a_built_in_one() {
    // Selected with the devices, drawn as one.
    let c = compilation(REGISTRY, "view v {\n  select device group-by site\n  render text, dot\n}");
    assert!(messages(&c, Severity::Error).is_empty(), "{:?}", messages(&c, Severity::Error));
    let (outputs, _) = build(&c, None);
    let text = &outputs.iter().find(|o| o.format == "text").unwrap().content;
    assert!(text.contains("p1 [printer]") && text.contains("p2 [printer]"), "{text}");
    let dot = &outputs.iter().find(|o| o.format == "dot").unwrap().content;
    assert!(dot.contains("class=\"kind_device\""), "{dot}");
    // Its parents: a printer in an app cannot be.
    let c = compilation(&REGISTRY.replace("app crm { owner: t,", "app crm { owner: t, printer p3,"), "");
    assert!(
        messages(&c, Severity::Error).iter().any(|m| m.contains("printer") && m.contains("p3")),
        "{:?}",
        messages(&c, Severity::Error)
    );
}

#[test]
fn a_wrong_schema_is_reported() {
    let c = compile(&[SourceFile {
        path: "schema.ca".into(),
        text: r#"
kind server { os: strng, ip: string, like: device }
kind scanner { parents: site }
kind badge { like: devise }
kind lamp { like: device, parents: room, color: ref(colour), code: pattern("[a-") }
kind lamp { like: device }
"#
        .into(),
    }]);
    assert_eq!(
        messages(&c, Severity::Error),
        [
            "like of lamp written twice",
            "kind scanner: a kind of the project is like a built-in one",
            "like: devise is not a built-in kind",
            "unknown kind: room",
            "unknown type: strng",
            "ip is already a key of server",
            "like: the like of a built-in kind does not change",
            "unknown kind: colour",
            "invalid pattern: [a-",
        ]
    );
    // Not in a plan file.
    let c = compile(&[SourceFile { path: "v.cav".into(), text: "kind server { os: string }".into() }]);
    assert_eq!(messages(&c, Severity::Error), ["a kind cannot be declared in a plan file (.cav, .cac)"]);
}

#[test]
fn formatted_one_key_per_line() {
    let flat = "kind server { os: string, ram: number, hypervisor: vmware | hyperv, inventory_id: pattern(\"INV-[0-9]+\") (required) }\n";
    let formatted = cadastre_core::format::format(flat).unwrap();
    assert_eq!(
        formatted,
        "kind server {\n  os: string\n  ram: number\n  hypervisor: vmware | hyperv\n  inventory_id: pattern(\"INV-[0-9]+\") (required)\n}\n"
    );
    assert_eq!(cadastre_core::format::format(&formatted).unwrap(), formatted);
    // Short: on one line.
    let short = "kind printer { like: device, serial: string }\n";
    assert_eq!(cadastre_core::format::format(short).unwrap(), short);
}

#[test]
fn keys_of_the_links() {
    let schema = "link { volume: number }
layer flow { package: pattern(\"PKG-[0-9]+\") (required) }
";
    let registry = "app a { -> b : https [flow] { volume: 3, package: \"PKG-1\" } }
app b { -> a : ldap [auth] { volume: 2 } }
";
    let check = |schema: &str, registry: &str| {
        let c = compile(&[
            SourceFile { path: "schema.ca".into(), text: schema.into() },
            SourceFile { path: "a.ca".into(), text: registry.into() },
        ]);
        (messages(&c, Severity::Error), messages(&c, Severity::Incomplete), messages(&c, Severity::Warning))
    };
    let (errors, incomplete, _) = check(schema, registry);
    assert!(errors.is_empty(), "{errors:?}");
    assert!(!incomplete.iter().any(|m| m.contains("package")), "{incomplete:?}");
    // Checked: on every link, and on the links of the layer only.
    let wrong = registry.replace("volume: 3", "volume: high").replace("\"PKG-1\"", "\"X\"");
    let (errors, _, _) = check(schema, &wrong);
    assert_eq!(errors, [r#"invalid value for volume: high (a number expected)"#, r#"invalid value for package: "X""#]);
    // Required on the flows: a flow without it is incomplete; the auth link is not concerned.
    let (_, incomplete, _) = check(schema, &registry.replace(", package: \"PKG-1\"", ""));
    assert!(incomplete.contains(&"link a -> b: missing attribute package".to_owned()), "{incomplete:?}");
    assert!(!incomplete.iter().any(|m| m.starts_with("link b -> a")), "{incomplete:?}");
    // A built-in key of the links cannot be declared again.
    let (errors, _, _) = check("link { carries: string }", "");
    assert_eq!(errors, ["carries is already a key of the links"]);
}

#[test]
fn a_list_of_one_value() {
    let c = compile(&[
        SourceFile { path: "schema.ca".into(), text: "kind server { hypervisor: vmware, os: strng }".into() },
        SourceFile {
            path: "a.ca".into(),
            text: "server a { hypervisor: vmware }
server b { hypervisor: kvm }"
                .into(),
        },
    ]);
    // `vmware`: one value; `strng`: a type, misspelt.
    assert_eq!(messages(&c, Severity::Error), ["unknown type: strng", "invalid value for hypervisor: kvm"]);
}

#[test]
fn blocks_of_a_kind_merge() {
    // The schema split by theme: two blocks for a kind, two for the links.
    let schema = "kind server { os: string }
link { volume: number }
";
    let other = "kind server { ram: number }
link { owner_team: string }
kind server { os: number }
";
    let c = compile(&[
        SourceFile { path: "network.ca".into(), text: schema.into() },
        SourceFile { path: "apps.ca".into(), text: other.into() },
        SourceFile {
            path: "a.ca".into(),
            text: "server s { os: \"linux\", ram: big }
app a { -> b : https [flow] { volume: x } }
app b
"
            .into(),
        },
    ]);
    // Both blocks apply; a key declared in two of them is an error.
    assert_eq!(
        messages(&c, Severity::Error),
        [
            "key os of server declared twice",
            "invalid value for ram: big (a number expected)",
            "invalid value for volume: x (a number expected)"
        ]
    );
}

#[test]
fn parent_in_a_kind_block_is_parents() {
    let c = compile(&[SourceFile {
        path: "s.ca".into(),
        text: "kind printer { like: device, parent: site }
"
        .into(),
    }]);
    assert_eq!(messages(&c, Severity::Error), ["the parents of a kind are written `parents:`"]);
}
