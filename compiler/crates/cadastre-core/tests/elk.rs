//! `layout: elk`: the SVG of a view laid out by Eclipse ELK and drawn by the compiler, with a
//! compiler built with the `elk` feature; laid out by Graphviz otherwise, with a warning.

use cadastre_core::{Severity, SourceFile, build, compile};

const REGISTRY: &str = r#"
site dc {
  server srv1
  server srv2
}
app shop { on: srv1, url: "https://shop.example.com"  -> erp : https/443 [flow] }
app erp { on: srv2  -> erpdb : jdbc [flow] }
database erpdb { on: srv2 }
"#;

const VIEWS: &str = r##"style {
  layer flow where proto = https { color: "#1d4ed8", legend: "HTTPS" }
}

view v {
  select app, database group-by site
  edges [flow]
  render svg { layout: elk, legend: true }
}
"##;

fn svg(views: &str) -> (String, Vec<String>) {
    let c = compile(&[
        SourceFile { path: "a.ca".into(), text: REGISTRY.into() },
        SourceFile { path: "v.cav".into(), text: views.into() },
    ]);
    let errors: Vec<String> =
        c.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.message.clone()).collect();
    assert!(errors.is_empty(), "{errors:?}");
    let (outputs, diagnostics) = build(&c, None);
    let warnings = diagnostics.iter().filter(|d| d.severity == Severity::Warning).map(|d| d.message.clone()).collect();
    (outputs.into_iter().find(|o| o.format == "svg").unwrap().content, warnings)
}

#[cfg(feature = "elk")]
#[test]
fn a_view_laid_out_by_elk() {
    let (svg, warnings) = svg(VIEWS);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(cadastre_core::render::is_drawn(&svg), "{svg}");
    // Its frame, its nodes and its edges, labeled; the icons of the kinds; the shape of a
    // database; the link of a node and the tooltips; the legend.
    for text in [
        ">dc<",
        ">shop<",
        ">erp<",
        ">https/443<",
        "<polyline",
        "<symbol id=\"icon-app\"",
        "<use xlink:href=\"#icon-database\"",
        "<path d=\"M",
        "<a xlink:href=\"https://shop.example.com\"",
        "<title>app shop",
        ">Legend<",
        ">HTTPS<",
        "stroke=\"#1d4ed8\"",
    ] {
        assert!(svg.contains(text), "{text}\n{svg}");
    }
}

#[cfg(not(feature = "elk"))]
#[test]
fn without_elk_graphviz_lays_it_out() {
    let (svg, warnings) = svg(VIEWS);
    assert!(!cadastre_core::render::is_drawn(&svg));
    assert!(svg.starts_with("digraph"), "{svg}");
    assert_eq!(warnings, ["view v: layout: elk needs a compiler built with the `elk` feature"]);
}

#[test]
fn layout_takes_graphviz_or_elk() {
    let c = compile(&[
        SourceFile { path: "a.ca".into(), text: REGISTRY.into() },
        SourceFile { path: "v.cav".into(), text: VIEWS.replace("layout: elk", "layout: dagre") },
    ]);
    assert!(c.diagnostics.iter().any(|d| d.severity == Severity::Error && d.message.contains("dagre")));
}

/// The top left corner of the shape of a node, by the tooltip of its group.
#[cfg(feature = "elk")]
fn corner(svg: &str, name: &str) -> (f64, f64) {
    let at = svg.find(&format!("<title>app {name}\n")).or_else(|| svg.find(&format!("<title>app {name}<"))).unwrap();
    let rect = &svg[at..][svg[at..].find("<rect ").unwrap()..];
    let number = |key: &str| {
        let start = rect.find(&format!(" {key}=\"")).unwrap() + key.len() + 3;
        rect[start..start + rect[start..].find('"').unwrap()].parse::<f64>().unwrap()
    };
    (number("x"), number("y"))
}

#[cfg(feature = "elk")]
#[test]
fn cross_and_first_order_the_nodes() {
    let registry =
        "app src { -> a : https [flow]  -> b : https [flow]  -> c : https [flow] }\napp a\napp b\napp c\napp d\n";
    let views = |options: &str| {
        format!("view v {{\n  select app\n  edges [flow]\n  render svg {{ layout: elk, {options} }}\n}}")
    };
    let render = |options: &str| {
        let c = compile(&[
            SourceFile { path: "a.ca".into(), text: registry.into() },
            SourceFile { path: "v.cav".into(), text: views(options) },
        ]);
        let (outputs, _) = build(&c, None);
        outputs.into_iter().find(|o| o.format == "svg").unwrap().content
    };
    // `cross`: a layer in this order across the direction, top to bottom in `lr`.
    let svg = render("cross: [c, a, b]");
    let (c, a, b) = (corner(&svg, "c").1, corner(&svg, "a").1, corner(&svg, "b").1);
    assert!(c < a && a < b, "c {c}, a {a}, b {b}\n{svg}");
    // `first`: before the other items of its frame, left in `lr`.
    let svg = render("first: [d]");
    let d = corner(&svg, "d").0;
    assert!(["src", "a", "b", "c"].iter().all(|n| d < corner(&svg, n).0), "{svg}");
}

#[cfg(feature = "elk")]
#[test]
fn edges_reaching_a_node_on_one_side_are_spread() {
    let registry = "app a { -> hub : https [flow] }\napp b { -> hub : https [flow] }\napp c { -> hub : https [flow] }\n\
                    app d { -> hub : https [flow] }\napp hub\n";
    let c = compile(&[
        SourceFile { path: "a.ca".into(), text: registry.into() },
        SourceFile {
            path: "v.cav".into(),
            text: "view v {\n  select app\n  edges [flow]\n  render svg { layout: elk }\n}".into(),
        },
    ]);
    let (outputs, _) = build(&c, None);
    let svg = outputs.into_iter().find(|o| o.format == "svg").unwrap().content;
    // The last points of the four edges, on the left side of `hub`: apart, the route orthogonal.
    let mut ends: Vec<(f64, f64)> = Vec::new();
    for route in svg.split("<polyline points=\"").skip(1) {
        let points: Vec<(f64, f64)> = route[..route.find('"').unwrap()]
            .split(' ')
            .map(|p| {
                let (x, y) = p.split_once(',').unwrap();
                (x.parse().unwrap(), y.parse().unwrap())
            })
            .collect();
        assert!(points.windows(2).all(|w| w[0].0 == w[1].0 || w[0].1 == w[1].1), "{points:?}");
        ends.push(*points.last().unwrap());
    }
    ends.sort_by(|a, b| a.1.total_cmp(&b.1));
    assert_eq!(ends.len(), 4);
    assert!(ends.windows(2).all(|w| w[1].1 - w[0].1 >= 4.0), "{ends:?}");
}
