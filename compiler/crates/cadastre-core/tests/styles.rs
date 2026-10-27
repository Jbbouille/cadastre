//! Styles and legends (spec § 6, `style`): legends limited to what shows in the view, swatches,
//! fill colors, frame colors, free attributes as edge labels.

use cadastre_core::{Compilation, Severity, SourceFile, build, compile};

const REGISTRY: &str = r#"
site paris {
  hosting: on-prem
  network lan {
    subnet apps {
      cidr: 10.1.2.0/24
      server web01 { scope: external }
      server db01
    }
  }
}
router r1 { }
router r2 { }
wan_link fiber1 { ends: [r1, r2], technology: fiber, circuit_id: "FR-42-0007" }

system pricing {
  app ui { on: web01, -> db : jdbc/1521 [flow] { ticket: "CHG-1" } }
  database db { on: db01 }
}
"#;

const STYLE: &str = r##"
style {
  edges where proto in [http, https] { color: "#1d4ed8", legend: "HTTP / HTTPS" }
  edges where proto = jdbc           { color: "#4b5563", legend: "JDBC" }
  kind wan_link                      { color: "#0f766e", line: dashed, legend: "WAN links" }
  kind system                        { color: "#7c3aed", fill: "#ede9fe", legend: "Systems" }
  nodes where scope = external       { fill: "#1f2937", legend: "External" }
}
"##;

fn compilation(views: &str) -> Compilation {
    let c = compile(&[
        SourceFile { path: "a.ca".into(), text: REGISTRY.into() },
        SourceFile { path: "v.cav".into(), text: format!("{STYLE}\n{views}") },
    ]);
    let errors: Vec<_> = c.diagnostics.iter().filter(|d| d.severity == Severity::Error).collect();
    assert!(errors.is_empty(), "{errors:?}");
    c
}

fn output(c: &Compilation, view: &str, format: &str) -> String {
    let (outputs, _) = build(c, Some(view));
    outputs.into_iter().find(|o| o.format == format).unwrap().content
}

#[test]
fn the_legend_lists_what_shows_in_the_view() {
    let c = compilation(
        "view apps {\n  select app, database, server group-by system\n  edges [flow]\n  render dot, text { legend: true }\n}",
    );
    let text = output(&c, "apps", "text");
    let legend = &text[text.find("\nlegend\n").unwrap()..];
    // JDBC and the systems show; HTTP (no such link) and MPLS (routers not selected) do not.
    assert!(legend.contains("edges JDBC: color #4b5563"), "{legend}");
    assert!(legend.contains("nodes Systems: color #7c3aed, fill #ede9fe"), "{legend}");
    assert!(legend.contains("nodes External: fill #1f2937"), "{legend}");
    assert!(!legend.contains("HTTP") && !legend.contains("WAN"), "{legend}");

    // Swatches: a thick line for edges, a filled square for nodes.
    let dot = output(&c, "apps", "dot");
    assert!(dot.contains(r##"height="3" bgcolor="#4b5563""##), "{dot}");
    assert!(dot.contains(r##"bgcolor="#ede9fe" color="#7c3aed" border="1""##), "{dot}");
}

#[test]
fn an_overridden_rule_leaves_the_legend() {
    let c = compilation(
        "view apps {\n  select app, database\n  edges [flow]\n  style { edges { color: red } }\n  render text { legend: true }\n}",
    );
    let text = output(&c, "apps", "text");
    assert!(!text.contains("JDBC"), "{text}");
}

#[test]
fn a_view_relabels_a_rule_of_the_project() {
    let c = compilation(
        "view apps {\n  select app, database\n  edges [flow]\n  style { edges where proto = jdbc { color: \"#4b5563\", legend: \"Oracle JDBC\" } }\n  render text { legend: true }\n}",
    );
    let text = output(&c, "apps", "text");
    let legend = &text[text.find("\nlegend\n").unwrap()..];
    assert!(legend.contains("edges Oracle JDBC: color #4b5563") && !legend.contains("edges JDBC"), "{legend}");
}

#[test]
fn fills_and_frames() {
    let c = compilation(
        "view apps {\n  select app, database, server group-by system\n  edges [flow]\n  render dot, mermaid\n}",
    );
    let dot = output(&c, "apps", "dot");
    // The system frame: border and fill; `nodes` rules leave frames out.
    assert!(dot.contains(r##"style="rounded,filled"; color="#7c3aed"; fillcolor="#ede9fe";"##), "{dot}");
    // A dark fill turns the text white.
    let web = dot.lines().find(|l| l.contains("tooltip=\"server paris.lan.apps.web01")).unwrap();
    assert!(
        web.contains("filled") && web.contains(r##"fillcolor="#1f2937""##) && web.contains("fontcolor=\"white\""),
        "{web}"
    );

    let mermaid = output(&c, "apps", "mermaid");
    assert!(mermaid.contains("stroke:#7c3aed,fill:#ede9fe"), "{mermaid}");
    assert!(mermaid.contains("fill:#1f2937,color:white"), "{mermaid}");
}

#[test]
fn free_attributes_as_edge_labels() {
    // On a connector (a fiber), and on a written link.
    let c = compilation(
        "view wan {\n  select router\n  edges wan_link\n  render text { edge_labels: [circuit_id] }\n}\n\
         view apps {\n  select app, database\n  edges [flow]\n  render text { edge_labels: [port, ticket] }\n}",
    );
    assert!(output(&c, "wan", "text").contains("[FR-42-0007]"), "{}", output(&c, "wan", "text"));
    assert!(output(&c, "apps", "text").contains("[jdbc/1521, CHG-1]"), "{}", output(&c, "apps", "text"));

    let c = compile(&[
        SourceFile { path: "a.ca".into(), text: REGISTRY.into() },
        SourceFile {
            path: "v.cav".into(),
            text: "view wan { select router render text { edge_labels: [circuitid] } }".into(),
        },
    ]);
    let unknown: Vec<_> = c.diagnostics.iter().filter(|d| d.message == "unknown edge label: circuitid").collect();
    assert_eq!(unknown.len(), 1, "{:?}", c.diagnostics);
    assert_eq!(unknown[0].help.as_deref(), Some("did you mean `circuit_id`?"));
}

#[test]
fn legends_in_mermaid_and_ascii_art() {
    let c = compilation("view wan {\n  select router\n  edges wan_link\n  render mermaid, ascii { legend: true }\n}");
    let mermaid = output(&c, "wan", "mermaid");
    assert!(
        mermaid.contains("subgraph legend[\"Legend\"]") && mermaid.contains("legend0[\"╍╍╍ WAN links\"]"),
        "{mermaid}"
    );
    assert!(mermaid.contains("style legend0 fill:none,stroke:none,color:#0f766e"), "{mermaid}");
    let ascii = output(&c, "wan", "ascii");
    assert!(ascii.contains("╌╌╌ edges WAN links: color #0f766e, dashed"), "{ascii}");
}

#[test]
fn interface_keys_on_peer_cables() {
    let registry = r#"
site dc {
  hosting: on-prem
  network lan {
    switch sw1 {
      interface g25 { peer: sw2.g25, circuit_id: "C-1" }
      interface te1 { label: "Ten1/0/1", peer: sw2.te1 }
    }
    switch sw2 {
      interface g25
      interface te1 { label: "TE 1/0/1" }
    }
  }
}
"#;
    let views = "view cables {\n  select switch\n  edges lift(peer, via: parent, merge: none)\n  render text { edge_labels: [label, circuit_id] }\n}\n\
                 view plain {\n  select switch\n  edges lift(peer, via: parent, merge: none)\n  render text\n}\n\
                 view circuits {\n  select switch\n  edges lift(peer, via: parent, merge: none)\n  render text { edge_labels: [circuit_id] }\n}\n\
                 view named {\n  select switch\n  edges lift(peer, via: parent, merge: none)\n  render text { edge_labels: [circuit_id, relation] }\n}";
    let c = compile(&[
        SourceFile { path: "a.ca".into(), text: registry.into() },
        SourceFile { path: "v.cav".into(), text: views.into() },
    ]);
    let errors: Vec<_> = c.diagnostics.iter().filter(|d| d.severity == Severity::Error).collect();
    assert!(errors.is_empty(), "{errors:?}");
    // One edge per cable after the lift; the keys of the interfaces, the declaring one first: its
    // label or, failing that, its name; the label of the other end when it has one.
    let cables = output(&c, "cables", "text");
    assert!(cables.contains("[g25, C-1]"), "{cables}");
    assert!(cables.contains("[Ten1/0/1, TE 1/0/1]"), "{cables}");
    // Without labels, the edges are named after their relation.
    assert_eq!(output(&c, "plain", "text").matches("[peer]").count(), 2);
    // With keys of its own, a cable without them stays bare, unless `relation` is asked for.
    let circuits = output(&c, "circuits", "text");
    assert!(circuits.contains("[C-1]") && !circuits.contains("[peer]"), "{circuits}");
    let named = output(&c, "named", "text");
    assert!(named.contains("[C-1, peer]") && named.contains("[peer]"), "{named}");
}

#[test]
fn labels_on_several_lines() {
    let registry = "data customer-accounts\ndata invoices\ndata payments\n\
                    app a { -> b : https/443 [flow] { carries: [customer-accounts, invoices, payments] } }\napp b";
    let view = |wrap: &str| {
        format!(
            "view v {{\n  select app\n  edges [flow]\n  render dot, mermaid, text {{ edge_labels: [carries]{wrap} }}\n}}"
        )
    };
    let build_view = |wrap: &str| {
        let c = compile(&[
            SourceFile { path: "a.ca".into(), text: registry.into() },
            SourceFile { path: "v.cav".into(), text: view(wrap) },
        ]);
        let errors: Vec<_> =
            c.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.message.clone()).collect();
        (c, errors)
    };
    // One value per line: line breaks in DOT, <br/> in Mermaid, a single line in the text output.
    let (c, errors) = build_view(", edge_labels_wrap: values");
    assert!(errors.is_empty(), "{errors:?}");
    assert!(output(&c, "v", "dot").contains("label=\"customer-accounts\\ninvoices\\npayments\""));
    assert!(output(&c, "v", "mermaid").contains("|\"customer-accounts<br/>invoices<br/>payments\"|"));
    assert!(output(&c, "v", "text").contains("[customer-accounts, invoices, payments]"));
    // A width: values gathered while they fit, never cut.
    let (c, _) = build_view(", edge_labels_wrap: 30");
    assert!(output(&c, "v", "dot").contains("label=\"customer-accounts, invoices\\npayments\""));
    let (c, _) = build_view(", edge_labels_wrap: 10");
    assert!(output(&c, "v", "dot").contains("label=\"customer-accounts\\ninvoices\\npayments\""));
    // Without the option, one line.
    let (c, _) = build_view("");
    assert!(output(&c, "v", "dot").contains("label=\"customer-accounts, invoices, payments\""));
    let (_, errors) = build_view(", edge_labels_wrap: wide");
    assert_eq!(errors, ["invalid value for edge_labels_wrap: wide"]);
}

#[test]
fn a_summary_on_merged_edges() {
    let registry = "system shop {\n  app web { -> billing.a : sftp [flow], -> billing.b : sftp [flow] }\n  \
                    app api { -> billing.c : https/443 [flow] }\n}\nsystem billing { app a, app b, app c }";
    let view = |option: &str| {
        format!(
            "view v {{\n  select system\n  edges lift([flow], via: ancestor(system))\n  render dot {{ {option} }}\n}}"
        )
    };
    let dot = |option: &str| {
        let c = compile(&[
            SourceFile { path: "a.ca".into(), text: registry.into() },
            SourceFile { path: "v.cav".into(), text: view(option) },
        ]);
        let errors: Vec<_> =
            c.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.message.clone()).collect();
        (output(&c, "v", "dot"), errors)
    };
    // Three links merged: their protocols and their number; the detail stays in the tooltip.
    let (summary, errors) = dot("merge_label: count");
    assert!(errors.is_empty(), "{errors:?}");
    assert!(
        summary.contains("label=\"https, sftp ×3\"") && summary.contains("from: shop.web -> billing.a : sftp"),
        "{summary}"
    );
    // From 5 links on only: the usual label here.
    let (usual, _) = dot("merge_label: 5");
    assert!(usual.contains("label=\"https/443, sftp\""), "{usual}");
    let (_, errors) = dot("merge_label: 1");
    assert_eq!(errors, ["invalid value for merge_label: 1"]);
}

/// The lines of the default theme in the legend, unless a rule with a legend says what they mean.
#[test]
fn the_legend_explains_the_default_theme() {
    let registry = "app crm { scope: external }
app billing {
  -> crm : https [flow] { confidence: assumed }
  -> ledger : jdbc [flow]
}
database ledger
";
    let legend = |style: &str| {
        let c = compile(&[
            SourceFile { path: "a.ca".into(), text: registry.into() },
            SourceFile {
                path: "v.cav".into(),
                text: format!(
                    "view v {{
  select app, database
  edges [flow]
  style {{ {style} }}
  render text {{ legend: true }}
}}"
                ),
            },
        ]);
        let (outputs, _) = build(&c, None);
        let text = outputs[0].content.clone();
        text.find(
            "
legend
",
        )
        .map(|at| text[at..].to_owned())
        .unwrap_or_default()
    };
    let automatic = legend("");
    assert!(automatic.contains("edges Assumed link: dashed"), "{automatic}");
    assert!(automatic.contains("nodes External: dashed"), "{automatic}");
    // A rule of its own about the same thing replaces them.
    let own = legend(
        "edges where confidence = assumed { legend: \"Supposé\" }
  nodes where scope = external { fill: \"#eeeeee\", legend: \"Hors périmètre\" }",
    );
    assert!(own.contains("edges Supposé") && own.contains("nodes Hors périmètre"), "{own}");
    assert!(!own.contains("Assumed link") && !own.contains("nodes External"), "{own}");
    // A rule on something else leaves them.
    let other = legend("edges where proto = jdbc { color: \"#4b5563\", legend: \"JDBC\" }");
    assert!(other.contains("Assumed link") && other.contains("JDBC"), "{other}");
}
