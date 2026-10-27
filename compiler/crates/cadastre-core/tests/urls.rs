//! URLs: the address of a service (`url`) and the access for its administration (`console`),
//! checked, without secrets, and opened by a click in the diagrams and the HTML tables.

use cadastre_core::{Compilation, Severity, SourceFile, build, compile};

const REGISTRY: &str = r#"
server web01 { console: ["https://idrac-web01.example.com", "ssh://admin@web01.example.com"] }
server web02
app labels { on: web01, url: "https://labels.example.com/" }
app batch { on: web02 }
"#;

fn compilation(registry: &str, views: &str) -> Compilation {
    compile(&[
        SourceFile { path: "a.ca".into(), text: registry.into() },
        SourceFile { path: "v.cav".into(), text: views.into() },
    ])
}

fn errors(c: &Compilation) -> Vec<String> {
    c.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.message.clone()).collect()
}

#[test]
fn urls_are_checked() {
    assert!(errors(&compilation(REGISTRY, "")).is_empty());
    let c = compilation(
        r#"
server web01 { console: "https://admin:secret@idrac-web01" }
app labels { on: web01, url: "labels.example.com" }
app api { on: web01, url: "https://api.example.com/v1?apikey=abc" }
"#,
        "",
    );
    assert_eq!(
        errors(&c),
        [
            r#"console holds a password: "https://admin:secret@idrac-web01""#,
            r#"invalid value for url: "labels.example.com" (a URL such as "https://host/path" expected)"#,
            r#"url holds a secret (`apikey`): "https://api.example.com/v1?apikey=abc""#,
        ]
    );
    // On a kind without the key, `url` stays a free key, not checked.
    assert!(errors(&compilation("server web01 { url: \"anything\" }", "")).is_empty());
}

#[test]
fn a_click_opens_the_url_then_the_console() {
    let c = compilation(REGISTRY, "view hosts {\n  select app, server\n  render dot, mermaid\n}");
    let (outputs, _) = build(&c, None);
    let dot = &outputs.iter().find(|o| o.format == "dot").unwrap().content;
    let mermaid = &outputs.iter().find(|o| o.format == "mermaid").unwrap().content;
    assert!(dot.contains(r#"URL="https://labels.example.com/", target="_blank""#), "{dot}");
    assert!(dot.contains(r#"URL="https://idrac-web01.example.com", target="_blank""#), "{dot}");
    assert_eq!(dot.matches("URL=").count(), 2, "{dot}");
    assert!(mermaid.contains(r#"href "https://labels.example.com/" _blank"#), "{mermaid}");
}

#[test]
fn html_tables_link_the_urls() {
    let c = compilation(REGISTRY, "view hosts {\n  select server\n  render html { columns: [name, console] }\n}");
    let (outputs, _) = build(&c, None);
    let html = &outputs[0].content;
    assert!(
        html.contains(
            r#"<td><a href="https://idrac-web01.example.com">https://idrac-web01.example.com</a>, <a href="ssh://admin@web01.example.com">ssh://admin@web01.example.com</a></td>"#
        ),
        "{html}"
    );
}
