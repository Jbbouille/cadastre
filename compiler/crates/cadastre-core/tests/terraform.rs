//! The import of a Terraform state into `.ca` files.

use cadastre_core::terraform::import;
use cadastre_core::{Severity, SourceFile, compile};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

/// The imported files, compiled: no error, whatever the state holds.
fn imported(name: &str) -> Vec<(String, String)> {
    let files = import(&fixture(name)).unwrap();
    let sources: Vec<SourceFile> =
        files.iter().map(|f| SourceFile { path: f.path.clone(), text: f.text.clone() }).collect();
    let c = compile(&sources);
    let errors: Vec<String> =
        c.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.render(&sources)).collect();
    assert!(errors.is_empty(), "{}\n{}", errors.join(""), files.iter().map(|f| f.text.as_str()).collect::<String>());
    files.into_iter().map(|f| (f.path, f.text)).collect()
}

#[test]
fn an_aws_state_file() {
    let files = imported("aws.tfstate");
    let paths: Vec<&str> = files.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(paths, ["aws-eu-west-3.ca", "not-imported.ca"]);
    let site = &files[0].1;
    for expected in [
        "site aws-eu-west-3 {",
        "region: eu-west-3",
        "vpc prod {",
        "subnet private-a {",
        "cidr: 10.0.2.0/24",
        "// aws_instance.web[0]",
        "server web-1 { ip: 10.0.2.10, instance_type: t3.medium, cloud_id: i-01 }",
        "router nat-a {",
        "load_balancer front {",
        "cluster k8s-prod {",
        "database orders-db {",
        "on: aws-eu-west-3.prod",
    ] {
        assert!(site.contains(expected), "{expected}\n{site}");
    }
    // Resources not imported are listed; data sources are not resources.
    assert!(files[1].1.contains("aws_s3_bucket.assets") && !files[1].1.contains("aws_ami"), "{}", files[1].1);
}

#[test]
fn azure_and_google_cloud_from_terraform_show() {
    let files = imported("azure-gcp.json");
    let paths: Vec<&str> = files.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(paths, ["azure-westeurope.ca", "gcp-shop.ca"]);
    assert!(files[0].1.contains("server app-vm-01 { ip: 10.10.1.4, size: Standard_B2s }"), "{}", files[0].1);
    assert!(files[1].1.contains("server api-1 { ip: 10.20.1.5, machine_type: e2-medium }"), "{}", files[1].1);
    assert!(files[1].1.contains("// module.gcp.google_compute_instance.api"), "{}", files[1].1);
}

#[test]
fn not_a_state() {
    assert!(import("{}").unwrap_err().starts_with("not a Terraform state"));
    assert!(import("nope").unwrap_err().starts_with("not JSON"));
}
