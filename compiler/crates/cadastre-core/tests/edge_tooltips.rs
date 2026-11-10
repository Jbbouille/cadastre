//! The tooltip of an edge of links: its ends and ports, then where its data go, in words,
//! whatever the orientation of the view.

use cadastre_core::{Language, Options, SourceFile, build, compile_with};

const REGISTRY: &str = r#"app reader {
  label: "Reader"
  -> files : sftp/22 [flow] { exchange: pull }
}
app sender {
  label: "Sender"
  -> files : sftp/22 [flow] { exchange: push }
  -> files : https/443 [flow]
}
app both {
  -> files : https/443 [flow] { exchange: both }
}
server files { label: "File hub" }
"#;

fn dot(language: Language) -> String {
    let views = "view v {\n  select app, server\n  edges [flow]\n  render svg\n}\n";
    let c = compile_with(
        &[
            SourceFile { path: "a.ca".into(), text: REGISTRY.into() },
            SourceFile { path: "v.cav".into(), text: views.into() },
        ],
        Options { language, ..Options::default() },
    );
    assert!(c.diagnostics.iter().all(|d| d.severity != cadastre_core::Severity::Error), "{:?}", c.diagnostics);
    build(&c, None).0.into_iter().find(|o| o.format == "svg").unwrap().content
}

#[test]
fn the_tooltip_says_where_the_data_go() {
    let dot = dot(Language::En);
    // Drawn in the direction of the connection, the reader pulling: the tooltip says the data
    // come from the file hub.
    assert!(
        dot.contains("tooltip=\"Reader → File hub : sftp/22\\ndata: File hub → Reader (pull, fetched)\\n"),
        "{dot}"
    );
    assert!(dot.contains("tooltip=\"both → File hub : https/443\\ndata: both ↔ File hub (both ways)\\n"), "{dot}");
    // Each link with its own exchange, where it comes from.
    assert!(
        dot.contains(
            "tooltip=\"Sender → File hub : sftp/22\\ndata: Sender → File hub (push, sent)\\n\
             from: sender -> files : sftp/22 (push)\""
        ),
        "{dot}"
    );
    assert!(
        dot.contains("tooltip=\"Sender → File hub : https/443\\ndata: direction not stated (no exchange)\\n"),
        "{dot}"
    );
}

#[test]
fn the_tooltip_in_the_language_of_the_project() {
    let dot = dot(Language::Fr);
    assert!(dot.contains("\\ndonnées : File hub → Reader (pull, récupérées)\\n"), "{dot}");
}
