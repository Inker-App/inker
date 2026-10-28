#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::Read;
use std::os::unix::fs::PermissionsExt;

use camino::Utf8PathBuf;
use inker_agents::mcpb;
use serde_json::{Value, json};

fn manifest() -> Value {
    mcpb::manifest("1.2.3", "inker", "${HOME}/Private")
}

#[test]
fn manifest_has_the_required_fields() {
    let m = manifest();
    for key in [
        "manifest_version",
        "name",
        "display_name",
        "version",
        "description",
        "author",
        "server",
    ] {
        assert!(m.get(key).is_some(), "missing {key}: {m}");
    }
    assert_eq!(m["name"], json!("inker"));
    assert_eq!(m["display_name"], json!("INKER"));
    assert_eq!(m["version"], json!("1.2.3"));
    assert!(m["author"]["name"].is_string());
}

#[test]
fn manifest_runs_the_bundled_binary_as_a_stdio_shim() {
    let m = manifest();
    assert_eq!(m["server"]["type"], json!("binary"));
    assert_eq!(m["server"]["entry_point"], json!("inker"));
    assert_eq!(
        m["server"]["mcp_config"]["command"],
        json!("${__dirname}/inker")
    );
    assert_eq!(m["server"]["mcp_config"]["args"], json!(["mcp"]));
    assert_eq!(m["compatibility"]["platforms"], json!(["darwin"]));
}

#[test]
fn manifest_offers_an_optional_private_folder_picker() {
    let m = manifest();
    let pf = &m["user_config"]["private_folder"];
    assert_eq!(pf["type"], json!("directory"));
    assert_eq!(pf["default"], json!("${HOME}/Private"));
    assert_eq!(pf["required"], json!(false));
    assert!(pf["title"].is_string() && pf["description"].is_string());
}

#[test]
fn manifest_follows_the_binary_name() {
    let m = mcpb::manifest("0.1.0", "inker-shim", "/x");
    assert_eq!(m["server"]["entry_point"], json!("inker-shim"));
    assert_eq!(
        m["server"]["mcp_config"]["command"],
        json!("${__dirname}/inker-shim")
    );
}

#[test]
fn description_never_says_redact() {
    let text = manifest().to_string().to_lowercase();
    assert!(!text.contains("redact"), "{text}");
    assert!(manifest()["description"].as_str().unwrap().len() > 20);
}

struct Built {
    _dir: tempfile::TempDir,
    out: Utf8PathBuf,
    bin: Utf8PathBuf,
}

fn build() -> Built {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    let bin = root.join("some-inker-build");
    std::fs::write(&bin, b"#!/bin/sh\necho fake inker\n").unwrap();
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o644)).unwrap();
    let out = root.join("dist/INKER.mcpb");
    let got = mcpb::build_bundle(&out, &bin, "1.2.3").unwrap();
    assert_eq!(got, out);
    Built {
        _dir: dir,
        out,
        bin,
    }
}

fn archive(p: &Utf8PathBuf) -> zip::ZipArchive<std::fs::File> {
    zip::ZipArchive::new(std::fs::File::open(p).unwrap()).unwrap()
}

#[test]
fn bundle_holds_exactly_manifest_binary_and_readme() {
    let b = build();
    let mut z = archive(&b.out);
    let mut names: Vec<String> = z.file_names().map(|n| n.unwrap().into_owned()).collect();
    names.sort();
    assert_eq!(names, ["README.md", "inker", "manifest.json"]);

    let mut m = String::new();
    z.by_name("manifest.json")
        .unwrap()
        .read_to_string(&mut m)
        .unwrap();
    let m: Value = serde_json::from_str(&m).unwrap();
    assert_eq!(m, manifest_for("1.2.3"));

    let mut bin = Vec::new();
    z.by_name("inker").unwrap().read_to_end(&mut bin).unwrap();
    assert_eq!(bin, std::fs::read(&b.bin).unwrap());

    let mut readme = String::new();
    z.by_name("README.md")
        .unwrap()
        .read_to_string(&mut readme)
        .unwrap();
    assert!(readme.contains("INKER"));
    assert!(!readme.to_lowercase().contains("redact"));
}

fn manifest_for(version: &str) -> Value {
    mcpb::manifest(version, "inker", mcpb::DEFAULT_PRIVATE_FOLDER)
}

#[test]
fn binary_entry_is_executable_and_deflated() {
    let b = build();
    let mut z = archive(&b.out);
    let f = z.by_name("inker").unwrap();
    let mode = f.unix_mode().unwrap();
    assert_eq!(mode & 0o111, 0o111, "mode {mode:o}");
    assert_eq!(f.compression(), zip::CompressionMethod::Deflated);
}

#[test]
fn building_twice_gives_identical_bytes() {
    let b = build();
    let first = std::fs::read(&b.out).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1100));
    mcpb::build_bundle(&b.out, &b.bin, "1.2.3").unwrap();
    assert_eq!(first, std::fs::read(&b.out).unwrap());
}

#[test]
fn missing_binary_is_an_error_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    let out = root.join("x.mcpb");
    assert!(mcpb::build_bundle(&out, &root.join("absent"), "1.0.0").is_err());
    assert!(!out.exists());
}
