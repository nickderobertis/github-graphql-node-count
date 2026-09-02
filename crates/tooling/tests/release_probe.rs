//! `scripts/release-probe.sh`, driven against a real HTTP server.
//!
//! The probe is what a consumer asks whether a release has landed. Answering
//! `absent` when the registry was merely unreachable would tell that consumer to
//! keep waiting for something already published — or, worse, that nothing shipped
//! when something did. So each outcome is driven for real: the script runs
//! `curl` against a listening socket, and only the registry on the far side is
//! local.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[path = "support/tiny_http.rs"]
mod tiny_http;

/// A throwaway checkout holding the script and a `release-targets.toml`.
struct Checkout {
    root: PathBuf,
}

impl Drop for Checkout {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

impl Checkout {
    /// Lay down the script beside a declaration naming `ids`.
    fn with_ids(label: &str, ids: &[&str]) -> Self {
        let root = std::env::temp_dir().join(format!(
            "release-probe-{label}-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        std::fs::create_dir_all(root.join("scripts")).expect("create the checkout");
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scripts/release-probe.sh")
            .canonicalize()
            .expect("the script under test exists");
        std::fs::copy(&source, root.join("scripts/release-probe.sh")).expect("copy the script");
        let declaration: String = ids
            .iter()
            .map(|id| format!("[[target]]\nid = \"{id}\"\nname = \"crate\"\n\n"))
            .collect();
        std::fs::write(root.join("release-targets.toml"), declaration).expect("write the toml");
        Self { root }
    }

    fn run(&self, api: &str) -> Output {
        Command::new("bash")
            .arg("scripts/release-probe.sh")
            .current_dir(&self.root)
            .env("CRATES_API", api)
            .output()
            .expect("run release-probe.sh")
    }
}

fn stdout_of(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("utf-8")
}

fn stderr_of(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("utf-8")
}

#[test]
fn a_published_crate_is_reported_at_its_current_version() {
    let server = tiny_http::Server::answering(200, br#"{"crate":{"max_stable_version":"1.2.3"}}"#);
    let checkout = Checkout::with_ids("published", &["crate:github-graphql-node-count"]);

    let output = checkout.run(&server.base_url());
    assert!(output.status.success(), "{}", stderr_of(&output));
    assert_eq!(
        stdout_of(&output).trim(),
        "crate:github-graphql-node-count 1.2.3"
    );
}

#[test]
fn a_crate_the_registry_does_not_know_is_reported_absent() {
    let server = tiny_http::Server::answering(404, br#"{"errors":[{"detail":"Not Found"}]}"#);
    let checkout = Checkout::with_ids("absent", &["crate:github-graphql-node-count"]);

    let output = checkout.run(&server.base_url());
    assert!(output.status.success(), "a 404 is an answer, not a failure");
    assert_eq!(
        stdout_of(&output).trim(),
        "crate:github-graphql-node-count absent"
    );
}

#[test]
fn an_unreachable_registry_is_a_failure_rather_than_absent() {
    // The distinction the whole script exists for: not knowing is not the same as
    // knowing nothing was published.
    let checkout = Checkout::with_ids("unreachable", &["crate:github-graphql-node-count"]);

    let output = checkout.run("http://127.0.0.1:1/crates");
    assert!(
        !output.status.success(),
        "an unreachable registry must not report success"
    );
    assert!(
        !stdout_of(&output).contains("absent"),
        "{}",
        stdout_of(&output)
    );
    let stderr = stderr_of(&output);
    assert!(stderr.contains("could not reach"), "{stderr}");
    assert!(
        stderr.contains("ACTION:"),
        "a failure says what to do next: {stderr}"
    );
}

#[test]
fn a_server_error_is_a_failure_rather_than_absent() {
    let server = tiny_http::Server::answering(503, b"upstream is having a moment");
    let checkout = Checkout::with_ids("server-error", &["crate:github-graphql-node-count"]);

    let output = checkout.run(&server.base_url());
    assert!(!output.status.success(), "a 503 must not report success");
    assert!(
        !stdout_of(&output).contains("absent"),
        "{}",
        stdout_of(&output)
    );
    assert!(
        stderr_of(&output).contains("answered HTTP 503"),
        "{}",
        stderr_of(&output)
    );
}

#[test]
fn a_response_without_a_usable_version_is_a_failure() {
    let server = tiny_http::Server::answering(200, br#"{"crate":{"name":"whatever"}}"#);
    let checkout = Checkout::with_ids("no-version", &["crate:github-graphql-node-count"]);

    let output = checkout.run(&server.base_url());
    assert!(
        !output.status.success(),
        "an unrecognised response shape must not report success"
    );
    assert!(
        stderr_of(&output).contains("no usable version"),
        "{}",
        stderr_of(&output)
    );
}

#[test]
fn a_registry_this_script_cannot_answer_for_is_refused() {
    let server = tiny_http::Server::answering(200, br#"{"crate":{"max_stable_version":"1.0.0"}}"#);
    let checkout = Checkout::with_ids("other-registry", &["npm:github-graphql-node-count"]);

    let output = checkout.run(&server.base_url());
    assert!(
        !output.status.success(),
        "a registry it cannot answer for must not be guessed at"
    );
    assert!(
        stderr_of(&output).contains("only answers for crates.io"),
        "{}",
        stderr_of(&output)
    );
}

#[test]
fn an_identifier_that_does_not_name_a_package_is_refused_before_it_reaches_a_url() {
    let server = tiny_http::Server::answering(200, br#"{"crate":{"max_stable_version":"1.0.0"}}"#);
    let checkout = Checkout::with_ids("empty-name", &["crate:"]);

    let output = checkout.run(&server.base_url());
    assert!(
        !output.status.success(),
        "an empty package name must not reach a URL"
    );
    assert!(
        stderr_of(&output).contains("does not name a crates.io package"),
        "{}",
        stderr_of(&output)
    );
}

#[test]
fn a_missing_declaration_says_what_to_do() {
    let checkout = Checkout::with_ids("missing", &[]);
    std::fs::remove_file(checkout.root.join("release-targets.toml")).expect("remove the toml");

    let output = checkout.run("http://127.0.0.1:1/crates");
    assert!(!output.status.success());
    assert!(
        stderr_of(&output).contains("is missing"),
        "{}",
        stderr_of(&output)
    );
}
