//! Hold the project graph to `SCOPE_POLICY`, the module-boundary rule.
//!
//! This binary is not run by `release-contract:test`, whose inputs are the
//! release configuration: an edge is drawn in a `project.json` or a member
//! `Cargo.toml` that another project owns, so neither the affected set nor that
//! target's cache key would move with it. It runs as `workspace:test`, which
//! depends on every project and is keyed on every definition and manifest — see
//! this crate's AGENTS.md.

mod support;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use support::{read, repo_root};

/// The package name of every workspace member, from the root manifest's list.
fn workspace_member_names() -> BTreeSet<String> {
    let root = repo_root();
    let workspace: toml::Table =
        toml::from_str(&read(&root.join("Cargo.toml"))).expect("root Cargo.toml");
    workspace["workspace"]["members"]
        .as_array()
        .expect("a members list")
        .iter()
        .map(|member| {
            let path = root
                .join(member.as_str().expect("a member path"))
                .join("Cargo.toml");
            let manifest: toml::Table = toml::from_str(&read(&path)).expect("member Cargo.toml");
            manifest["package"]["name"]
                .as_str()
                .expect("a package name")
                .to_string()
        })
        .collect()
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
    let members = workspace_member_names();
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

#[test]
fn the_repository_aggregate_depends_on_every_other_project() {
    // `workspace:test` runs this suite, and it is only in the affected set when a
    // project it depends on is. A project it does not depend on could draw a
    // forbidden edge in its own files and never be checked until a full sweep.
    let projects = graph();
    let others: BTreeSet<String> = projects
        .iter()
        .filter(|(project, _)| scope_of(project) != "scope:repo")
        .map(|(project, _)| project.name.clone())
        .collect();
    let aggregates: Vec<_> = projects
        .iter()
        .filter(|(project, _)| scope_of(project) == "scope:repo")
        .collect();
    assert_eq!(
        aggregates.len(),
        1,
        "exactly one project carries scope:repo"
    );
    let (aggregate, edges) = aggregates[0];
    let missing: Vec<&String> = others.difference(edges).collect();
    assert!(
        missing.is_empty(),
        "{} must list {missing:?} in its implicitDependencies, or a change to them never runs \
         the boundary check",
        aggregate.name,
    );
}
