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

/// The manifests this repository actually publishes, asked of the script that
/// does the publishing rather than read out of a restatement of it.
///
/// `scripts/publish-crate.sh` is run in dry-run mode with the registry pointed at
/// an unreachable address, so it reaches its "would publish" line without a
/// credential and without contacting crates.io, and names the manifest it would
/// upload. Anything that changes what the release publishes changes this answer.
fn published_manifests() -> BTreeSet<String> {
    let root = repo_root();

    // The workflow must actually run the script, or the script's answer would be
    // about something nothing invokes.
    let workflow = read(&root.join(".github/workflows/release.yml"));
    assert!(
        workflow.contains("./scripts/publish-crate.sh"),
        "release.yml runs scripts/publish-crate.sh",
    );

    let version = workspace_version();
    let output = std::process::Command::new("bash")
        .arg("scripts/publish-crate.sh")
        .arg(format!("v{version}"))
        .current_dir(&root)
        .env("CARGO_REGISTRY_TOKEN", "not-a-real-token")
        .env("PUBLISH_DRY_RUN", "1")
        // Unroutable, so the "is it already published?" probe fails fast and the
        // script proceeds to say what it would publish.
        .env("CRATES_API", "http://127.0.0.1:1/crates")
        .output()
        .expect("run scripts/publish-crate.sh");
    assert!(
        output.status.success(),
        "publish-crate.sh refused a well-formed tag: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    String::from_utf8(output.stdout)
        .expect("utf-8")
        .lines()
        .filter_map(|line| line.strip_prefix("would publish "))
        .map(|rest| {
            rest.split(" from ")
                .nth(1)
                .expect("the manifest it would publish")
                .to_string()
        })
        .collect()
}

/// The one version every manifest inherits, from `[workspace.package]`.
fn workspace_version() -> String {
    let workspace: toml::Table =
        toml::from_str(&read(&repo_root().join("Cargo.toml"))).expect("root Cargo.toml");
    workspace["workspace"]["package"]["version"]
        .as_str()
        .expect("a version")
        .to_string()
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

/// An Nx project definition, enough of it to read the tags and the graph edges.
#[derive(Debug, Deserialize)]
struct Project {
    name: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default, rename = "implicitDependencies")]
    implicit_dependencies: Vec<String>,
}

/// The module-boundary policy: which scope each project may depend on.
///
/// Nx's own boundary rule is an ESLint rule and there is no JavaScript here, so
/// the tags are held to their meaning by reading the definitions and manifests
/// that draw the real graph edges. Every project must carry exactly one scope in
/// this table, so a new project has to declare where it sits rather than
/// inheriting whatever the graph happens to allow.
const SCOPE_POLICY: &[(&str, &[&str])] = &[
    // The published deliverable: a leaf, so nothing in this repository can make
    // it heavier for a consumer.
    ("scope:library", &[]),
    // The consumer-shaped suite drives the published surface and nothing else.
    ("scope:e2e", &["scope:library"]),
    // The install-path suite exercises the packaged crate, and nothing else.
    ("scope:install", &["scope:library"]),
    // A contract project owns an agreement its consumers hold to, so it must not
    // depend on them: one convenient import would re-attach it to every change in
    // the repository, and nothing would fail to say so.
    ("scope:contract", &[]),
    // The shell-tooling suite drives scripts, not crates.
    ("scope:tooling", &[]),
    // The repository-level aggregate, which exists to depend on everything.
    (
        "scope:repo",
        &[
            "scope:library",
            "scope:e2e",
            "scope:install",
            "scope:contract",
            "scope:tooling",
        ],
    ),
];

/// Every project definition in the graph, with the projects it depends on.
///
/// An edge is either a real Cargo dependency on a workspace member or an Nx
/// `implicitDependencies` entry — the two ways this repository draws one.
fn graph() -> Vec<(Project, BTreeSet<String>)> {
    let root = repo_root();
    let members: BTreeSet<String> = workspace_members()
        .into_iter()
        .map(|(_, manifest)| manifest.package.name)
        .collect();
    let mut projects = Vec::new();
    for definition in project_definitions(&root) {
        let project: Project = serde_json::from_str(&read(&definition))
            .unwrap_or_else(|error| panic!("{}: {error}", definition.display()));
        let mut edges: BTreeSet<String> = project.implicit_dependencies.iter().cloned().collect();
        let manifest_path = definition.with_file_name("Cargo.toml");
        if manifest_path.is_file() {
            let manifest: toml::Table =
                toml::from_str(&read(&manifest_path)).expect("member manifest");
            for table in ["dependencies", "dev-dependencies", "build-dependencies"] {
                if let Some(toml::Value::Table(declared)) = manifest.get(table) {
                    edges.extend(declared.keys().filter(|k| members.contains(*k)).cloned());
                }
            }
        }
        projects.push((project, edges));
    }
    projects
}

/// Every `project.json` in the repository, excluding build and vendor trees.
fn project_definitions(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, found: &mut Vec<PathBuf>) {
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if path.is_dir() {
                if !matches!(name.as_ref(), "node_modules" | "target" | ".git" | ".nx") {
                    walk(&path, found);
                }
            } else if name == "project.json" {
                found.push(path);
            }
        }
    }
    let mut found = Vec::new();
    walk(root, &mut found);
    found.sort();
    assert!(found.len() > 1, "the repository has a project graph");
    found
}

/// The one scope tag a project carries, panicking if it carries none or several.
fn scope_of(project: &Project) -> &str {
    let scopes: Vec<&str> = project
        .tags
        .iter()
        .map(String::as_str)
        .filter(|tag| tag.starts_with("scope:"))
        .collect();
    assert_eq!(
        scopes.len(),
        1,
        "{} must carry exactly one scope: tag, got {:?}",
        project.name,
        project.tags,
    );
    let scope = scopes[0];
    assert!(
        SCOPE_POLICY.iter().any(|(known, _)| *known == scope),
        "{} carries {scope}, which SCOPE_POLICY does not know; add it and say what it may depend on",
        project.name,
    );
    scope
}

/// The module-boundary rule, in the form a Cargo workspace can enforce.
#[test]
fn every_project_depends_only_on_the_scopes_its_own_scope_allows() {
    let projects = graph();
    let scope_by_project: std::collections::BTreeMap<String, String> = projects
        .iter()
        .map(|(project, _)| (project.name.clone(), scope_of(project).to_string()))
        .collect();

    for (project, edges) in &projects {
        let scope = scope_of(project);
        let allowed = SCOPE_POLICY
            .iter()
            .find(|(known, _)| *known == scope)
            .map(|(_, allowed)| *allowed)
            .expect("scope_of checked the scope is known");
        for edge in edges {
            let target_scope = scope_by_project.get(edge).unwrap_or_else(|| {
                panic!("{} depends on {edge}, which is not a project", project.name)
            });
            assert!(
                allowed.contains(&target_scope.as_str()),
                "{} ({scope}) depends on {edge} ({target_scope}), which {scope} may not reach; \
                 allowed: {allowed:?}",
                project.name,
            );
        }
    }
}

#[test]
fn every_scope_the_policy_declares_is_carried_by_a_project() {
    // A policy entry for a scope nothing uses is a rule nobody is held to, and it
    // would quietly permit an edge the day somebody adds that tag.
    let carried: BTreeSet<String> = graph()
        .iter()
        .map(|(project, _)| scope_of(project).to_string())
        .collect();
    for (scope, _) in SCOPE_POLICY {
        assert!(
            carried.contains(*scope),
            "SCOPE_POLICY declares {scope}, which no project carries"
        );
    }
}
