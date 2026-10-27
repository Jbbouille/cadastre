//! Virtual IPs: addresses and host names served by a load balancer, a proxy or a cluster,
//! usable as flow targets by their host name (spec § 3, Virtual IPs).

use cadastre_core::source::FileId;
use cadastre_core::{Compilation, Diagnostic, Severity, SourceFile, build, compile};

const REGISTRY: &str = r#"
site paris {
  hosting: on-prem
  network lan {
    subnet dmz { cidr: 10.1.2.0/24, server web01 { ip: 10.1.2.10 } }
    load_balancer f5 {
      vip labels { host: [print.example.com, labels.example.com], ip: 10.1.2.30, subnet: dmz
        -> label-server : http/8080 [flow]
      }
      vip intranet { host: "intranet.example.com" }
    }
  }
}

app label-server { on: web01 }
app desktop-label {
  on: web01
  -> print.example.com : https/443 [flow]
}
"#;

fn compilation(extra: &str) -> Compilation {
    compile(&[
        SourceFile { path: "a.ca".into(), text: format!("{REGISTRY}\n{extra}") },
        SourceFile {
            path: "v.cav".into(),
            text: "view flows {\n  select app, vip\n  edges [flow]\n  render text\n}".into(),
        },
    ])
}

fn messages(c: &Compilation, severity: Severity) -> Vec<String> {
    c.diagnostics.iter().filter(|d| d.severity == severity).map(|d| d.message.clone()).collect()
}

fn errors(c: &Compilation) -> Vec<String> {
    messages(c, Severity::Error)
}

#[test]
fn a_host_name_is_a_flow_target() {
    let c = compilation("");
    assert!(errors(&c).is_empty(), "{:?}", errors(&c));
    let link = c.registry.links.iter().position(|l| l.target.text() == "print.example.com").unwrap();
    let (_, target) = c.model.link_ends[link].unwrap();
    assert_eq!(c.registry.qualified_name(target), "paris.lan.f5.labels");

    // Go to definition on the host name.
    let text = format!("{REGISTRY}\n");
    let offset = text.rfind("print.example.com").unwrap() + 3;
    let targets: Vec<String> =
        c.definitions(FileId(0), offset).into_iter().map(|id| c.registry.qualified_name(id)).collect();
    assert_eq!(targets, ["paris.lan.f5.labels"]);
}

#[test]
fn views_show_the_host_name() {
    let c = compilation("");
    let (outputs, _) = build(&c, None);
    let text = &outputs[0].content;
    assert!(text.contains("print.example.com [vip]") || text.contains("print.example.com"), "{text}");
    assert!(text.contains("desktop-label -> paris.lan.f5.labels") || text.contains("-> labels"), "{text}");
}

#[test]
fn host_names_are_checked() {
    let c = compilation(r#"load_balancer lb2 { vip bad { host: "-nope.example.com" } }"#);
    assert!(errors(&c).iter().any(|m| m.starts_with("invalid value for host")), "{:?}", errors(&c));

    let c = compilation("load_balancer lb2 { vip other { host: PRINT.example.com } }");
    assert!(
        errors(&c).iter().any(|m| m.contains("host print.example.com is served by two virtual IPs")),
        "{:?}",
        errors(&c)
    );
}

#[test]
fn addresses_are_checked() {
    // Outside the subnet, and shared with a server of the subnet.
    let outside = REGISTRY.replace("ip: 10.1.2.30", "ip: 10.9.9.9");
    let c = compile(&[SourceFile { path: "a.ca".into(), text: outside }]);
    assert!(errors(&c).iter().any(|m| m == "10.9.9.9 is outside the CIDR of subnet dmz"), "{:?}", errors(&c));
    let shared = REGISTRY.replace("ip: 10.1.2.30", "ip: 10.1.2.10");
    let c = compile(&[SourceFile { path: "a.ca".into(), text: shared }]);
    assert!(errors(&c).iter().any(|m| m == "10.1.2.10 is used twice in subnet dmz"), "{:?}", errors(&c));

    // Outside any subnet: unique among virtual IPs.
    let c = compilation("load_balancer lb2 { vip a { ip: 192.0.2.1 }, vip b { ip: 192.0.2.1 } }");
    assert!(errors(&c).iter().any(|m| m == "192.0.2.1 is the address of two virtual IPs"), "{:?}", errors(&c));
}

#[test]
fn a_virtual_ip_needs_an_address_or_a_host() {
    let c = compilation("load_balancer lb2 { vip empty }");
    let incomplete = messages(&c, Severity::Incomplete);
    assert!(incomplete.iter().any(|m| m == "vip lb2.empty: neither ip nor host"), "{incomplete:?}");
}

#[test]
fn a_host_name_where_no_virtual_ip_is_expected() {
    let c = compilation("app reporting { on: print.example.com }");
    let wrong: Vec<&Diagnostic> =
        c.diagnostics.iter().filter(|d| d.message.starts_with("print.example.com is a vip")).collect();
    assert_eq!(wrong.len(), 1, "{:?}", errors(&c));
}
