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
fn a_bare_version_with_no_v_prefix_is_refused() {
    // `${tag#v}` leaves a bare `1.2.3` unchanged, so validating the *stripped*
    // string would accept a tag release-plz never cuts. The whole tag is matched.
    let output = run(&workspace_version(), &[("CARGO_REGISTRY_TOKEN", "x")]);
    assert!(
        !output.status.success(),
        "a tag without the `v` prefix must not publish"
    );
    assert!(
        stderr_of(&output).contains("is not vX.Y.Z"),
        "{}",
        stderr_of(&output)
    );
}

#[test]
fn a_registry_base_that_is_not_an_http_url_is_refused() {
    // It reaches curl as a URL, so its shape is checked before the script would
    // otherwise hand curl something that could be read as another argument.
    let output = run(
        &format!("v{}", workspace_version()),
        &[
            ("CARGO_REGISTRY_TOKEN", "x"),
            ("CRATES_API", "file:///etc/passwd"),
        ],
    );
    assert!(
        !output.status.success(),
        "a non-http registry base must not be used"
    );
    let stderr = stderr_of(&output);
    assert!(
        stderr.contains("CRATES_API is not an http(s) URL"),
        "{stderr}"
    );
    assert!(stderr.contains("ACTION:"), "{stderr}");
}

#[test]
fn an_unreadable_manifest_is_reported_rather_than_dying_on_a_raw_read_error() {
    // The read happens under `set -e`, so without an explicit guard the script
    // would exit through the command substitution before saying anything useful.
    let scratch = Scratch::new("no-manifest");
    let output = scratch.run(
        &format!("v{}", workspace_version()),
        &[("CARGO_REGISTRY_TOKEN", "x")],
    );
    assert!(!output.status.success());
    let stderr = stderr_of(&output);
    assert!(stderr.contains("cannot read"), "{stderr}");
    assert!(
        stderr.contains("ACTION:"),
        "a refusal says what to do next: {stderr}"
    );
}

/// A checkout holding only the script, so the manifest it wants is absent.
struct Scratch {
    root: PathBuf,
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

impl Scratch {
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "publish-crate-{label}-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        std::fs::create_dir_all(root.join("scripts")).expect("create the scratch tree");
        std::fs::copy(
            repo_root().join("scripts/publish-crate.sh"),
            root.join("scripts/publish-crate.sh"),
        )
        .expect("copy the script");
        Self { root }
    }

    /// Give the scratch tree a crate manifest saying exactly this, so what the
    /// script reads out of it is chosen by the test rather than by the repository.
    fn with_crate_manifest(self, body: &str) -> Self {
        let manifest = self
            .root
            .join("crates/github-graphql-node-count/Cargo.toml");
        std::fs::create_dir_all(manifest.parent().expect("manifest directory"))
            .expect("create the crate directory");
        std::fs::write(&manifest, body).expect("write the crate manifest");
        self
    }

    fn run(&self, tag: &str, env: &[(&str, &str)]) -> Output {
        let mut command = Command::new("bash");
        command
            .arg("scripts/publish-crate.sh")
            .arg(tag)
            .current_dir(&self.root)
            .env("CRATES_API", UNROUTABLE_REGISTRY)
            .env_remove("CARGO_REGISTRY_TOKEN");
        for (key, value) in env {
            command.env(key, value);
        }
        command.output().expect("run publish-crate.sh")
    }
}

#[test]
fn a_manifest_version_that_is_not_a_version_is_refused_as_itself() {
    // What `sed` lifts out of the manifest is a line that looked like a version.
    // Held only against the tag, a mangled manifest would be reported as a
    // tag/manifest disagreement — pointing the reader at the tag, which is the one
    // thing here that is well-formed. It is refused for what it actually is.
    let scratch = Scratch::new("bad-manifest-version").with_crate_manifest(
        "[package]\nname = \"github-graphql-node-count\"\nversion = \"1.2\"\n",
    );
    let output = scratch.run("v1.2.3", &[("CARGO_REGISTRY_TOKEN", "x")]);

    assert!(
        !output.status.success(),
        "a manifest version that is not X.Y.Z must not publish"
    );
    let stderr = stderr_of(&output);
    assert!(
        stderr.contains("the manifest's version '1.2' is not X.Y.Z"),
        "the refusal names what the manifest said: {stderr}"
    );
    assert!(
        !stderr.contains("but the manifest says"),
        "a malformed manifest is not a tag disagreement: {stderr}"
    );
    assert!(stderr.contains("ACTION:"), "{stderr}");
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
