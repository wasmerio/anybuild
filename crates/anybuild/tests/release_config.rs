use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use serde_json::Value;

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("anybuild crate should be inside the workspace")
}

fn read_json(path: &Path) -> Value {
    let contents = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    serde_json::from_str(&contents)
        .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()))
}

#[test]
fn release_please_tracks_one_workspace_release() {
    let root = workspace_root();
    let config = read_json(&root.join("release-please-config.json"));
    let manifest = read_json(&root.join(".release-please-manifest.json"));

    assert_eq!(
        config["include-component-in-tag"], false,
        "the public release tag must remain vX.Y.Z"
    );
    assert!(
        config.get("plugins").is_none(),
        "the root Rust strategy already updates every Cargo workspace member"
    );

    let package_paths = config["packages"]
        .as_object()
        .expect("release-please packages should be an object")
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        package_paths,
        BTreeSet::from(["."]),
        "multiple release paths cannot share the component-less vX.Y.Z tag"
    );
    assert!(
        config["packages"]["."].get("component").is_none(),
        "the v tag prefix is not a Release Please component"
    );

    let manifest_paths = manifest
        .as_object()
        .expect("release-please manifest should be an object")
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    assert_eq!(manifest_paths, BTreeSet::from(["."]));
}
