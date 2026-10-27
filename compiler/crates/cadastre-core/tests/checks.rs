//! Checks: rules written by the user on the model, run by the compilation.

use cadastre_core::{Diagnostic, Severity, SourceFile, compile};

const REGISTRY: &str = "team t
site paris { hosting: on-prem, firewall fw01
  network n {
    subnet dmz { cidr: 10.0.1.0/24, exposed: true, server web01 { os: debian12, ip: 10.0.1.10 } }
    subnet lan { cidr: 10.0.2.0/24, server db01 { os: debian9, ip: 10.0.2.10 } }
  }
}
data customer { classification: personal }
data invoice { classification: confidential }
system shop {
  owner: t
  app front { on: web01, -> back : https/443 [flow] { carries: [invoice], through: [fw01] } }
  app back { on: db01, -> store : tcp/5432 [flow] { carries: [customer] } }
  database store { on: db01 }
}
app orphan { on: web01 }
";

/// The diagnostics of the checks of `plans`, compiled with the registry.
fn checks(plans: &str) -> Vec<Diagnostic> {
    let files = [
        SourceFile { path: "a.ca".into(), text: REGISTRY.into() },
        SourceFile { path: "checks.cac".into(), text: plans.into() },
    ];
    let c = compile(&files);
    c.diagnostics.into_iter().filter(|d| d.message.starts_with("check ")).collect()
}

#[test]
fn expect_none_lists_what_matches() {
    let d = checks(
        "check owners {
           select app where not exists(owner)
           expect none
           message: \"every app has an owner\"
         }",
    );
    assert_eq!(d.len(), 1, "{d:?}");
    assert_eq!(d[0].severity, Severity::Error);
    assert_eq!(d[0].message, "check owners failed: 1 node selected, none expected: orphan");
    assert_eq!(d[0].help.as_deref(), Some("every app has an owner"));
    // The check, then the offending resource.
    assert_eq!(d[0].spans.len(), 2);
    assert_eq!((d[0].spans[1].line, d[0].spans[1].column), (15, 4));
}

#[test]
fn passing_checks_are_silent() {
    let d = checks(
        "check exposed-servers { select server where ancestor(subnet).exposed expect count = 1 }
         check some-database { select database expect some }
         check no-mainframe { select server where os = zos expect none }",
    );
    assert!(d.is_empty(), "{d:?}");
}

#[test]
fn edges_without_select() {
    // No personal data towards an exposed subnet: `back -> store` carries some, but on the LAN.
    let ok = checks(
        "check no-personal-data-exposed {
           select server
           edges lift([flow], via: on) where carries has customer and target.ancestor(subnet).exposed
           expect none
         }",
    );
    assert!(ok.is_empty(), "{ok:?}");
    // The `where` after `lift` filters the lifted edges, between servers.
    let d = checks(
        "check front-servers-only-call-the-lan {
           select server
           edges lift([flow], via: on) where source.ancestor(subnet).exposed
           expect none
         }",
    );
    assert_eq!(d.len(), 1, "{d:?}");
    assert!(d[0].message.ends_with("none expected: paris.n.dmz.web01 -> paris.n.lan.db01"), "{}", d[0].message);
    let d = checks("check no-personal-data { edges [flow] where carries has customer expect none }");
    assert_eq!(d.len(), 1, "{d:?}");
    assert!(d[0].message.ends_with("1 edge selected, none expected: shop.back -> shop.store"), "{}", d[0].message);
    assert_eq!(d[0].spans[1].line, 12);
}

#[test]
fn expect_all_reports_the_failing_items() {
    let d = checks(
        "check flows-through-fw01 { select app, database edges [flow] expect all through has fw01 }
         check supported-os { select server expect all os != debian9 severity: warning }",
    );
    assert_eq!(d.len(), 2, "{d:?}");
    assert_eq!(
        d[0].message,
        "check flows-through-fw01 failed: 1 of 2 edges does not meet the expectation: shop.back -> shop.store"
    );
    assert_eq!(d[1].severity, Severity::Warning);
    assert_eq!(d[1].message, "check supported-os failed: 1 of 2 nodes does not meet the expectation: paris.n.lan.db01");
}

#[test]
fn expect_some_and_count() {
    let d = checks(
        "check a-firewall-per-site { select firewall where ancestor(site) = lyon expect some }
         check two-databases { select database expect count >= 2 }",
    );
    assert_eq!(d.len(), 2, "{d:?}");
    assert_eq!(d[0].message, "check a-firewall-per-site failed: no node selected, at least one expected");
    assert_eq!(d[1].message, "check two-databases failed: 1 node selected, expected count >= 2");
}

#[test]
fn invalid_checks() {
    let files = [
        SourceFile { path: "a.ca".into(), text: REGISTRY.into() },
        SourceFile {
            path: "checks.cac".into(),
            text: "check a { select app }
                   check b { expect none }
                   check c { select app expect none severity: fatal, mesage: \"x\" }
                   check c { select serveur expect some }"
                .into(),
        },
    ];
    let c = compile(&files);
    let messages: Vec<&str> = c.diagnostics.iter().map(|d| d.message.as_str()).collect();
    for expected in [
        "check a has no expect clause",
        "check b selects nothing",
        "invalid value for severity: fatal",
        "unknown check attribute: mesage",
        "check c declared twice",
        "unknown kind: serveur",
    ] {
        assert!(messages.contains(&expected), "{expected} not in {messages:?}");
    }
    // An invalid model is not checked.
    assert!(!messages.iter().any(|m| m.starts_with("check c failed")), "{messages:?}");
}

#[test]
fn a_check_of_severity_info_reports_an_information() {
    let d = checks("check supported-os { select server expect all os != debian9 severity: info }");
    assert_eq!(d.len(), 1, "{d:?}");
    assert_eq!(d[0].severity, Severity::Info);
    assert!(d[0].message.starts_with("check supported-os failed: "), "{}", d[0].message);
    assert_eq!(cadastre_core::diagnostic::Summary::of(&d).infos, 1);
    assert!(d[0].render(&[]).starts_with("info: check supported-os failed"));
}
