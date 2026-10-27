//! The `bandwidth` and `spdx` value types; `license`, an SPDX expression.

use cadastre_core::{Compilation, Severity, SourceFile, build, compile};

const REGISTRY: &str = r#"
kind wan_link { bandwidth: bandwidth }
team t
site a { hosting: on-prem, router r1 }
site b { hosting: on-prem, router r2 }
site c { hosting: on-prem, router r3 }
wan_link fast { ends: [r1, r2], bandwidth: 1Gbps }
wan_link slow { ends: [r2, r3], bandwidth: "100 Mbit/s" }
system shop {
  owner: t
  app api {
    on: r1
    license: "MIT OR Apache-2.0"
    component spring { version: 3.2.1, license: "Apache-2.0" }
  }
}
"#;

fn compilation(extra: &str, views: &str) -> Compilation {
    compile(&[
        SourceFile { path: "a.ca".into(), text: format!("{REGISTRY}{extra}") },
        SourceFile { path: "v.cav".into(), text: views.into() },
    ])
}

fn messages(c: &Compilation, severity: Severity) -> Vec<String> {
    c.diagnostics.iter().filter(|d| d.severity == severity).map(|d| d.message.clone()).collect()
}

#[test]
fn bandwidths_compare_in_bits_per_second() {
    let c = compilation("", "view big { select wan_link where bandwidth >= 500Mbps render text }");
    let errors = messages(&c, Severity::Error);
    assert!(errors.iter().all(|e| !e.contains("bandwidth") && !e.contains("license")), "{errors:?}");
    let (outputs, _) = build(&c, None);
    let text = &outputs.iter().find(|o| o.format == "text").unwrap().content;
    assert!(text.contains("fast") && !text.contains("slow"), "{text}");
}

#[test]
fn invalid_values() {
    let c = compilation(
        "wan_link odd { ends: [r1, r3], bandwidth: 10GB }\napp other { owner: t, on: r1, license: \"Apache-2\" }\n\
         app third { owner: t, on: r1, license: \"MIT or BSD-3-Clause\" }\n",
        "",
    );
    let errors = messages(&c, Severity::Error);
    for expected in [
        "invalid value for bandwidth: 10GB (a bandwidth such as 100Mbps or \"1.5 Gbit/s\" expected)",
        "license: Apache-2 is not an SPDX license identifier",
        "license: `or`: the operators are written AND, OR, WITH",
    ] {
        assert!(errors.contains(&expected.to_owned()), "{expected}: {errors:?}");
    }
    let help = c.diagnostics.iter().find(|d| d.message.contains("Apache-2 is")).and_then(|d| d.help.clone());
    assert_eq!(help.as_deref(), Some("did you mean `Apache-2.0`?"));
}
