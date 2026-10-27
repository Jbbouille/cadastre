//! The site of a project: its pages and its JSON model.

use std::collections::HashMap;

use cadastre_core::{SourceFile, compile, site};

const REGISTRY: &str = r#"
team ops
site dc1 {
  hosting: on-prem
  owner: ops
  network lan { subnet apps { cidr: 10.0.1.0/24, server web01 { ip: 10.0.1.10, interface eth0 { } } } }
}
server spare
site cloud1 { hosting: cloud, owner: ops }
data order
system sales {
  owner: ops
  description: "Sales and orders"
  app shop { label: "Web shop", on: web01, -> orders : https/443 [flow] { carries: [order] } }
  app orders { on: web01 }
}
"#;

const VIEWS: &str = "view apps {\n  title: \"Applications\"\n  description: \"The apps and their flows.\"\n  select app group-by system\n  edges [flow]\n  render svg\n}\nview trail {\n  select app\n  edges path(shop, orders)\n  render traversal\n}\n";
const CHECKS: &str = "check owned {\n  select system where not exists(owner)\n  expect none\n  message: \"every system has an owner\"\n}\ncheck labeled {\n  select app where not exists(label)\n  expect none\n  severity: info\n  message: \"an app without a label\"\n}\n";

fn files() -> Vec<SourceFile> {
    vec![
        SourceFile { path: "a.ca".into(), text: REGISTRY.into() },
        SourceFile { path: "v.cav".into(), text: VIEWS.into() },
        SourceFile { path: "c.cac".into(), text: CHECKS.into() },
    ]
}

fn built(svgs: &HashMap<String, String>, viz: Option<&str>) -> HashMap<String, String> {
    let files = files();
    let c = compile(&files);
    let options = site::Options { name: "Acme", svgs, viz, playground: true, language: cadastre_core::Language::En };
    site::build(&c, &files, &options).into_iter().map(|f| (f.path, f.content)).collect()
}

#[test]
fn the_pages_and_the_api() {
    let mut svgs = HashMap::new();
    svgs.insert(
        "apps".to_owned(),
        "<?xml version=\"1.0\"?>\n<svg width=\"10pt\" height=\"5pt\" viewBox=\"0 0 10 5\"><g/></svg>".to_owned(),
    );
    let site = built(&svgs, None);
    for path in [
        "index.html",
        "browse.html",
        "checks.html",
        "api.html",
        "site.css",
        "site.js",
        "views/apps.html",
        "resources/sales.shop.html",
        "resources/dc1.lan.apps.web01.html",
        "api/project.json",
        "api/resources.json",
        "api/resources/sales.shop.json",
        "api/views/apps.json",
        "api/checks.json",
        "api/search.json",
        "api/links.json",
    ] {
        assert!(site.contains_key(path), "{path} missing: {:?}", site.keys().collect::<Vec<_>>());
    }
    let home = &site["index.html"];
    assert!(home.contains("Applications") && home.contains("The apps and their flows."), "{home}");
    // The thumbnail is the file of the diagram, loaded lazily: a light home page.
    assert!(home.contains("<img src=\"views/apps.svg\" alt=\"\" loading=\"lazy\">"), "{home}");
    assert!(site["views/apps.svg"].contains("viewBox=\"0 0 10 5\""));
    assert!(home.contains("href=\"playground/\""), "{home}");
    // The check of severity info that fails is to know, not failing.
    assert!(home.contains("2 checks · 1 to know</a>"), "{home}");
    let checks = &site["checks.html"];
    assert!(checks.contains("<section class=\"check info\">") && checks.contains("to know</span>"), "{checks}");
    // What an incomplete resource misses, on its page; the site of an app through `on`.
    let spare = &site["resources/spare.html"];
    assert!(spare.contains("<p class=\"tag\">incomplete</p>"), "{spare}");
    assert!(spare.contains("<li>no owner, neither written nor inherited</li>"), "{spare}");
    let shop_record: serde_json::Value = serde_json::from_str(&site["api/resources/sales.shop.json"]).unwrap();
    assert_eq!(shop_record["site"], "dc1");
    // The sites by hosting, when there are several kinds of them.
    assert!(home.contains("<b>2</b> sites <span class=\"muted\">(1 cloud, 1 on-prem)</span>"), "{home}");
    // What runs in a site: the apps on its servers, at any depth.
    let dc1: serde_json::Value = serde_json::from_str(&site["api/resources/dc1.json"]).unwrap();
    assert_eq!(dc1["runs_here"], serde_json::json!(["sales.orders", "sales.shop"]));
    assert!(site["resources/dc1.html"].contains("Runs here"));
    // The API described, with live examples.
    let api = &site["api.html"];
    assert!(api.contains("href=\"api/resources/") && api.contains("href=\"api/views/apps.json\""), "{api}");
    assert!(api.contains("api/resources/&lt;id&gt;.json") && !api.contains("&amp;lt;"), "{api}");
    assert_eq!(shop_record["outgoing"][0]["through_steps"], serde_json::json!([]));
    // The interface is on the page of its server, not in the index.
    let browse = &site["browse.html"];
    assert!(!browse.contains("id=\"interface\""), "{browse}");
    assert!(site["resources/dc1.lan.apps.web01.html"].contains("eth0"));
    // A view rendered as a traversal shows it first, the graph after.
    let trail = &site["views/trail.html"];
    let own = trail.find("<figure class=\"diagram\"><svg").expect("the traversal");
    assert!(trail[own..].contains("<h2>As a graph</h2>"), "{trail}");
    // Its boxes open the pages of their resources.
    assert!(trail[own..].contains("<a xlink:href=\"../resources/sales.shop.html\""), "{trail}");
    assert!(site.contains_key("views/trail.traversal.svg"));
    let shop = &site["resources/sales.shop.html"];
    assert!(shop.contains("<h1>") && shop.contains("Web shop"), "{shop}");
    assert!(shop.contains("href=\"../resources/sales.orders.html\""), "{shop}");
    assert!(shop.contains("https/443"), "{shop}");
    assert!(shop.contains("href=\"../views/apps.html\""), "{shop}");
    let view = &site["views/apps.html"];
    assert!(view.contains("<figure class=\"diagram\"><svg"), "{view}");
    assert!(view.contains("playground/#view=apps"), "{view}");
    assert!(checks.contains("owned") && checks.contains("passed"), "{checks}");
    let project: serde_json::Value = serde_json::from_str(&site["api/project.json"]).unwrap();
    assert_eq!(project["name"], "Acme");
    assert_eq!(project["views"][0]["title"], "Applications");
}

#[test]
fn the_site_speaks_french_when_asked() {
    let files = files();
    let c = compile(&files);
    let svgs = HashMap::new();
    let options =
        site::Options { name: "Acme", svgs: &svgs, viz: None, playground: true, language: cadastre_core::Language::Fr };
    let site: HashMap<String, String> =
        site::build(&c, &files, &options).into_iter().map(|f| (f.path, f.content)).collect();
    let home = &site["index.html"];
    assert!(
        home.contains("<html lang=\"fr\">") && home.contains(">Vues</a>") && home.contains(">Vérifications</a>"),
        "{home}"
    );
    assert!(home.contains("2 vérifications · 1 à connaître"), "{home}");
    assert!(home.contains("<li><b>2</b> apps</li>"), "{home}");
    let shop = &site["resources/sales.shop.html"];
    assert!(shop.contains("<h2>Liens sortants") && shop.contains("<th>Vers</th>"), "{shop}");
    assert!(site["resources/dc1.html"].contains("Tourne ici"));
    assert!(site["checks.html"].contains("réussie"));
}

#[test]
fn a_view_without_svg_is_drawn_in_the_browser() {
    let site = built(&HashMap::new(), Some("playground/viz-standalone.js"));
    let view = &site["views/apps.html"];
    assert!(view.contains("data-viz=\"playground/viz-standalone.js\""), "{view}");
    assert!(view.contains("text/vnd.graphviz") && view.contains("digraph"), "{view}");
    // The nodes of the DOT link to the pages of their resources.
    assert!(view.contains("URL=\"../resources/sales.shop.html\""), "{view}");
    let none = built(&HashMap::new(), None);
    assert!(none["views/apps.html"].contains("<pre class=\"dot\">"));
}
