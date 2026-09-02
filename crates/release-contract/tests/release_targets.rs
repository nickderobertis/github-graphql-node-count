//! Hold `release-targets.toml` to what this repository actually publishes.
//!
//! The declaration is a contract with the repositories that wait on this one, so
//! it cannot be allowed to drift from the release configuration. These tests
//! reconcile the two **in both directions**: a name this repository starts
//! publishing but does not declare fails here, and so does a target declared for
//! something it does not publish.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// The declaration's shape, as the canonical schema its siblings write.
#[derive(Debug, Deserialize)]
struct Declaration {
    schema_version: u32,
    probe: String,
    #[serde(default, rename = "target")]
    targets: Vec<Target>,
    #[serde(default, rename = "retired")]
    retired: Vec<Target>,
}

/// One consumable artifact a dependent can name in order to wait on it.
#[derive(Debug, Deserialize)]
struct Target {
    /// `<registry>:<name>`, where the name is what that registry serves.
    id: String,
    /// The short name a dependent uses to wait on this target.
    name: String,
    /// What the artifact is, for a reader who does not know.
    what: String,
    /// Where in the release configuration it is published from.
    published_by: String,
    /// The manifest whose version and name this target takes.
    manifest: String,
}

/// A workspace member's manifest, enough of it to reconcile against.
#[derive(Debug, Deserialize)]
struct Manifest {
    package: Package,
}

#[derive(Debug, Deserialize)]
struct Package {
    name: String,
    /// Absent means "publishable"; `false` means this member reaches no registry.
    #[serde(default)]
    publish: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct WorkspaceManifest {
    workspace: Workspace,
}

#[derive(Debug, Deserialize)]
struct Workspace {
    members: Vec<String>,
}

/// The repository root, from this crate's own manifest directory.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("repo root")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn declaration() -> Declaration {
    let path = repo_root().join("release-targets.toml");
    toml::from_str(&read(&path)).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// Every workspace member's `[package]` table, in declaration order.
fn workspace_members() -> Vec<(String, Manifest)> {
    let root = repo_root();
    let workspace: WorkspaceManifest =
        toml::from_str(&read(&root.join("Cargo.toml"))).expect("root Cargo.toml");
    workspace
        .workspace
        .members
        .iter()
        .map(|member| {
            let relative = format!("{member}/Cargo.toml");
            let manifest: Manifest =
                toml::from_str(&read(&root.join(&relative))).expect("member Cargo.toml");
            (relative, manifest)
        })
        .collect()
}

/// The manifests `.github/workflows/release.yml` runs `cargo publish` against.
///
/// This is what the repository *actually* publishes, read from the one file that
/// does the publishing rather than restated anywhere.
fn published_manifests() -> BTreeSet<String> {
    let workflow = read(&repo_root().join(".github/workflows/release.yml"));
    workflow
        .lines()
        .filter(|line| line.contains("cargo publish"))
        .map(|line| {
            let after = line
                .split("--manifest-path")
                .nth(1)
                .unwrap_or_else(|| panic!("`cargo publish` without --manifest-path: {line}"));
            after
                .split_whitespace()
                .next()
                .expect("a manifest path")
                .to_string()
        })
        .collect()
}

#[test]
fn the_declaration_is_the_schema_its_siblings_write() {
    let declaration = declaration();
    assert_eq!(
        declaration.schema_version, 3,
        "the shape a producer writes today"
    );
    assert!(
        !declaration.targets.is_empty(),
        "this repository publishes something"
    );
    assert!(
        declaration.retired.is_empty(),
        "nothing published here has been withdrawn"
    );

    let probe = repo_root().join(&declaration.probe);
    assert!(
        probe.is_file(),
        "the declared probe {} exists",
        declaration.probe
    );

    let mut ids = BTreeSet::new();
    let mut names = BTreeSet::new();
    for target in &declaration.targets {
        let (registry, artifact) = target
            .id
            .split_once(':')
            .unwrap_or_else(|| panic!("`{}` is not `<registry>:<name>`", target.id));
        assert!(
            !registry.is_empty() && !artifact.is_empty(),
            "`{}` names both parts",
            target.id
        );
        assert!(
            ids.insert(target.id.clone()),
            "`{}` is declared twice",
            target.id
        );
        assert!(
            names.insert(target.name.clone()),
            "`{}` is a short name twice",
            target.name
        );
        assert!(
            !target.what.trim().is_empty(),
            "`{}` says what it is",
            target.id
        );
        assert!(
            target
                .published_by
                .contains(".github/workflows/release.yml"),
            "`{}` names the workflow that publishes it, got: {}",
            target.id,
            target.published_by,
        );
    }
}

#[test]
fn the_short_name_dependents_wait_on_is_the_one_this_repository_promised() {
    // A dependent's build names this string in order to wait on the crate's
    // release; changing it breaks that repository, so it is asserted literally.
    let declaration = declaration();
    let crate_target = declaration
        .targets
        .iter()
        .find(|target| target.id == "crate:github-graphql-node-count")
        .expect("the crate target is declared");
    assert_eq!(crate_target.name, "crate");
}

#[test]
fn every_declared_target_names_a_manifest_this_repository_publishes() {
    let root = repo_root();
    let published = published_manifests();
    for target in declaration().targets {
        let registry = target.id.split_once(':').expect("qualified id").0;
        assert_eq!(
            registry, "crate",
            "the only registry this repository publishes to"
        );

        let manifest_path = root.join(&target.manifest);
        assert!(manifest_path.is_file(), "{} exists", target.manifest);
        let manifest: Manifest = toml::from_str(&read(&manifest_path)).expect("member manifest");
        assert_eq!(
            format!("crate:{}", manifest.package.name),
            target.id,
            "{} declares the package name the id promises",
            target.manifest,
        );
        assert_ne!(
            manifest.package.publish,
            Some(false),
            "{} is declared as published but carries publish = false",
            target.manifest,
        );
        assert!(
            published.contains(&target.manifest),
            "release.yml publishes {} — declared targets: {published:?}",
            target.manifest,
        );
    }
}

#[test]
fn every_crate_this_repository_publishes_is_declared() {
    let declared: BTreeSet<String> = declaration()
        .targets
        .iter()
        .map(|target| target.manifest.clone())
        .collect();

    // Direction one: a workspace member that reaches a registry must be declared.
    for (manifest_path, manifest) in workspace_members() {
        if manifest.package.publish == Some(false) {
            assert!(
                !declared.contains(&manifest_path),
                "{manifest_path} carries publish = false but is declared as a release target",
            );
            continue;
        }
        assert!(
            declared.contains(&manifest_path),
            "{manifest_path} is publishable but undeclared in release-targets.toml",
        );
    }

    // Direction two: every `cargo publish` in the release workflow is declared.
    assert_eq!(
        published_manifests(),
        declared,
        "release.yml and release-targets.toml disagree about what is published",
    );
}
