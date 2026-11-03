//! Rack elevations (`render racks`): a column per rack, a box per device at its height.

use cadastre_core::{Compilation, Diagnostic, Severity, SourceFile, build, compile};

const REGISTRY: &str = r#"
site dc {
  hosting: on-prem
  rack r1 { room: "Room 2", units: 12 }
  rack r2
  switch sw1 { rack: r1, u: 12 }
  server big { rack: r1, u: 3, height: 4 }
  server small { rack: r1, u: 5 }
  server tall { rack: r1, u: 11, height: 3 }
  server loose { rack: r1 }
  server other { rack: r2, u: 1 }
}
"#;

fn compilation(views: &str) -> Compilation {
    let c = compile(&[
        SourceFile { path: "a.ca".into(), text: REGISTRY.into() },
        SourceFile { path: "v.cav".into(), text: views.into() },
    ]);
    let errors: Vec<_> = c.diagnostics.iter().filter(|d| d.severity == Severity::Error).collect();
    assert!(errors.is_empty(), "{errors:?}");
    c
}

fn racks(views: &str, view: &str) -> (String, Vec<Diagnostic>) {
    let (outputs, diagnostics) = build(&compilation(views), Some(view));
    let svg = outputs.into_iter().find(|o| o.format == "racks").unwrap().content;
    (svg, diagnostics)
}

#[test]
fn devices_at_their_height() {
    let (svg, diagnostics) = racks("view v { select rack  render racks }", "v");
    assert!(svg.starts_with("<svg"), "{svg}");
    // Both racks, under their site, with their units; r2 has 42 by default.
    assert!(svg.contains(">r1 · Room 2</text>") && svg.contains(">r2</text>"), "{svg}");
    assert!(svg.contains(">12</text>") && svg.contains(">42</text>"), "{svg}");
    // Everything placed in r1, a device without `u` listed under it.
    for name in ["sw1", "big", "small", "tall", "other"] {
        assert!(svg.contains(&format!(">{name}</text>")), "{name}: {svg}");
    }
    assert!(svg.contains(">Not placed (no u):</text>") && svg.contains(">loose</text>"), "{svg}");
    // `big` takes units 3 to 6: from 6 units under the top of 12, 4 units high.
    assert!(svg.contains("height=\"62.0\""), "{svg}");
    let warnings: Vec<&str> =
        diagnostics.iter().filter(|d| d.severity == Severity::Warning).map(|d| d.message.as_str()).collect();
    assert_eq!(
        warnings,
        [
            "view v: big and small in r1: on the same units",
            "view v: tall in r1: units 11 to 13 beyond the 12 units of the rack",
            "view v: tall and sw1 in r1: on the same units",
        ]
    );
}

#[test]
fn the_racks_of_the_devices_selected() {
    // A rack reached through the devices selected shows these devices only.
    let (svg, _) = racks("view v { select server where rack = r2  render racks }", "v");
    assert!(svg.contains(">r2</text>") && svg.contains(">other</text>"), "{svg}");
    assert!(!svg.contains(">r1") && !svg.contains(">big</text>"), "{svg}");
    let (svg, _) = racks("view v { select app  render racks }", "v");
    assert!(svg.contains("no rack"), "{svg}");
}
