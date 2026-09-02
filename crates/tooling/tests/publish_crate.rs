//! `scripts/publish-crate.sh`, whose whole job before `cargo publish` is to
//! refuse.
//!
//! A publish is not reversible, so each refusal is driven for real: the script
//! runs, and what it does with a bad tag, a missing credential, or a version the
//! manifest does not claim is asserted from its exit status and its message. The
//! registry is pointed at an unroutable address so no test contacts crates.io,
//! and `PUBLISH_DRY_RUN` stops the one accepted case short of uploading.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// An address nothing answers on, so the "already published?" probe fails fast.
const UNROUTABLE_REGISTRY: &str = "http://127.0.0.1:1/crates";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

/// The version every manifest inherits, which is the only tag the script accepts.
fn workspace_version() -> String {
    let manifest = std::fs::read_to_string(repo_root().join("Cargo.toml")).expect("root manifest");
    manifest
        .lines()
        .skip_while(|line| !line.starts_with("[workspace.package]"))
        .find_map(|line| line.strip_prefix("version = \"")?.strip_suffix('"'))
        .expect("[workspace.package] declares a version")
        .to_string()
}

/// Run the script with `tag`, and the given extra environment.
fn run(tag: &str, env: &[(&str, &str)]) -> Output {
    let mut command = Command::new("bash");
    command
        .arg("scripts/publish-crate.sh")
        .arg(tag)
        .current_dir(repo_root())
        .env("CRATES_API", UNROUTABLE_REGISTRY)
        .env_remove("CARGO_REGISTRY_TOKEN")
        .env_remove("PUBLISH_DRY_RUN");
    for (key, value) in env {
        command.env(key, value);
    }
    command.output().expect("run publish-crate.sh")
}

fn stderr_of(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("utf-8")
}

#[test]
fn a_tag_that_is_not_a_version_is_refused() {
    let output = run("not-a-tag", &[("CARGO_REGISTRY_TOKEN", "x")]);
    assert!(
        !output.status.success(),
        "a tag that is not vX.Y.Z must not publish"
    );
    let stderr = stderr_of(&output);
    assert!(stderr.contains("is not vX.Y.Z"), "{stderr}");
    assert!(
        stderr.contains("ACTION:"),
        "a refusal says what to do next: {stderr}"
    );
}

#[test]
fn a_tag_naming_a_version_the_manifest_does_not_is_refused() {
    // The case that matters: a hand-made tag would publish a version nobody
    // reviewed, under a number the changelog does not describe.
    let output = run("v99.98.97", &[("CARGO_REGISTRY_TOKEN", "x")]);
    assert!(
        !output.status.success(),
        "a tag disagreeing with the manifest must not publish"
    );
    let stderr = stderr_of(&output);
    assert!(stderr.contains("but the manifest says"), "{stderr}");
}

#[test]
fn a_missing_registry_credential_is_refused_rather_than_skipped() {
    let output = run(&format!("v{}", workspace_version()), &[]);
    assert!(
        !output.status.success(),
        "a missing credential must fail, not no-op to green"
    );
    let stderr = stderr_of(&output);
    assert!(
        stderr.contains("CARGO_REGISTRY_TOKEN is not set"),
        "{stderr}"
    );
}

#[test]
fn no_tag_at_all_is_refused() {
    let output = Command::new("bash")
        .arg("scripts/publish-crate.sh")
        .current_dir(repo_root())
        .env("CRATES_API", UNROUTABLE_REGISTRY)
        .env("CARGO_REGISTRY_TOKEN", "x")
        .output()
        .expect("run publish-crate.sh");
    assert!(!output.status.success());
    assert!(
        stderr_of(&output).contains("no tag given"),
        "{}",
        stderr_of(&output)
    );
}

#[test]
fn the_tag_release_plz_would_cut_is_accepted() {
    let version = workspace_version();
    let output = run(
        &format!("v{version}"),
        &[("CARGO_REGISTRY_TOKEN", "x"), ("PUBLISH_DRY_RUN", "1")],
    );
    assert!(output.status.success(), "{}", stderr_of(&output));
    let stdout = String::from_utf8(output.stdout).expect("utf-8");
    assert!(
        stdout.contains(&format!(
            "would publish github-graphql-node-count {version}"
        )),
        "{stdout}",
    );
    assert!(
        stdout.contains("crates/github-graphql-node-count/Cargo.toml"),
        "{stdout}"
    );
}

#[test]
fn a_version_already_on_the_registry_is_skipped_rather_than_republished() {
    // Idempotence, driven against a real HTTP server that answers 200 for the
    // version — so re-running after a partial failure is safe.
    let server = tiny_http::Server::answering(200, br#"{"version":{"num":"9.9.9"}}"#);
    let version = workspace_version();
    let output = run(
        &format!("v{version}"),
        &[
            ("CARGO_REGISTRY_TOKEN", "x"),
            ("CRATES_API", &server.base_url()),
        ],
    );
    assert!(output.status.success(), "{}", stderr_of(&output));
    let stdout = String::from_utf8(output.stdout).expect("utf-8");
    assert!(
        stdout.contains("is already on crates.io; nothing to do"),
        "{stdout}"
    );
}

#[path = "support/tiny_http.rs"]
mod tiny_http;
