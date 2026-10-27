//! Subnets of a site whose CIDRs overlap.

use cadastre_core::{Severity, SourceFile, compile};

#[test]
fn overlapping_subnets_of_a_site() {
    let registry = "site a { hosting: on-prem, network n {
                      subnet x { cidr: 10.0.1.0/24 }
                      subnet y { cidr: 10.0.1.128/25 }
                      subnet z { cidr: 10.0.2.0/24 } } }
                    site b { hosting: cloud, vpc v { subnet x { cidr: 10.0.1.0/24 } } }";
    let c = compile(&[SourceFile { path: "a.ca".into(), text: registry.into() }]);
    let errors: Vec<&str> =
        c.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.message.as_str()).collect();
    // Another site may reuse a private range.
    assert_eq!(errors, ["subnets a.n.x (10.0.1.0/24) and a.n.y (10.0.1.128/25) overlap"]);
}

#[test]
fn devices_off_the_path() {
    let registry = "team t
        site paris { hosting: on-prem, firewall fw01, network n { subnet a { cidr: 10.0.1.0/24, server s1 { } } } }
        site lyon { hosting: on-prem, firewall fw02, network n { subnet a { cidr: 10.0.2.0/24, server s2 { } } } }
        site rome { hosting: on-prem, firewall fw03 }
        system shop { owner: t
          app front { on: s1, -> back : https/443 [flow] { through: [fw01, fw02, fw03] } }
          app back { on: s2 }
        }";
    let c = compile(&[SourceFile { path: "a.ca".into(), text: registry.into() }]);
    let warnings: Vec<&str> =
        c.diagnostics.iter().filter(|d| d.severity == Severity::Warning).map(|d| d.message.as_str()).collect();
    // fw01 and fw02 are in the sites of the ends; fw03 is not.
    assert_eq!(warnings, ["firewall fw03 is in rome, where neither end of the flow is (paris, lyon)"]);
}
