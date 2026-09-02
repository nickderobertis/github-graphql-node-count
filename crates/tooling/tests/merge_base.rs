//! `scripts/merge-base.sh`, driven as a subprocess against real git repositories.
//!
//! This script decides what the gate runs. Its happy paths choose a scope; its
//! failure paths must choose **no** scope, so the caller runs everything. A bug
//! here does not fail loudly — it silently narrows every check — which is why
//! each fail-closed path is asserted to print nothing.

use std::path::{Path, PathBuf};
use std::process::Command;

/// A throwaway git repository with the script copied in, at the path the script
/// expects: a `scripts/` directory under the repository root.
struct Checkout {
    root: PathBuf,
}

impl Drop for Checkout {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

impl Checkout {
    /// Create an initialised repository on `main` with the script in place.
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "merge-base-{label}-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        std::fs::create_dir_all(root.join("scripts")).expect("create the checkout");
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scripts/merge-base.sh")
            .canonicalize()
            .expect("the script under test exists");
        std::fs::copy(&source, root.join("scripts/merge-base.sh")).expect("copy the script");

        let checkout = Self { root };
        checkout.git(&["init", "--quiet", "--initial-branch=main"]);
        checkout.git(&["config", "user.email", "tooling@example.invalid"]);
        checkout.git(&["config", "user.name", "tooling"]);
        checkout.commit("first");
        checkout
    }

    fn git(&self, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.root)
            .output()
            .unwrap_or_else(|error| panic!("git {args:?}: {error}"));
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr),
        );
        String::from_utf8(output.stdout)
            .expect("git speaks utf-8")
            .trim()
            .to_string()
    }

    /// Commit an empty change with `message`, and return the new commit's SHA.
    fn commit(&self, message: &str) -> String {
        self.git(&["commit", "--quiet", "--allow-empty", "-m", message]);
        self.git(&["rev-parse", "HEAD"])
    }

    /// Point `origin/<branch>` at the current `HEAD`, the way a fetched clone
    /// would.
    fn publish(&self, branch: &str) {
        let head = self.git(&["rev-parse", "HEAD"]);
        self.git(&[
            "update-ref",
            &format!("refs/remotes/origin/{branch}"),
            &head,
        ]);
    }

    fn publish_main(&self) {
        self.publish("main");
    }

    fn publish_release(&self) {
        self.publish("release");
    }

    /// Run the script with `env` set, returning `(stdout, stderr)`.
    fn run(&self, env: &[(&str, &str)]) -> (String, String) {
        let mut command = Command::new("bash");
        command.arg("scripts/merge-base.sh").current_dir(&self.root);
        // A real local run has neither of these set; each test opts in.
        command
            .env_remove("CI")
            .env_remove("GITHUB_BASE_REF")
            .env_remove("NODE_COUNT_NX_BASE_REF");
        for (key, value) in env {
            command.env(key, value);
        }
        let output = command.output().expect("run merge-base.sh");
        assert!(output.status.success(), "merge-base.sh must always exit 0");
        (
            String::from_utf8(output.stdout).expect("utf-8"),
            String::from_utf8(output.stderr).expect("utf-8"),
        )
    }
}

#[test]
fn a_local_branch_scopes_against_its_fork_point_from_main() {
    let checkout = Checkout::new("local");
    checkout.publish_main();
    let fork_point = checkout.git(&["rev-parse", "HEAD"]);
    checkout.git(&["checkout", "--quiet", "-b", "feature"]);
    checkout.commit("second");
    checkout.commit("third");

    let (stdout, stderr) = checkout.run(&[]);
    assert_eq!(stdout, fork_point, "stderr was: {stderr}");
}

#[test]
fn a_pull_request_build_scopes_against_the_base_branch_it_names() {
    let checkout = Checkout::new("pr");
    checkout.publish_main();
    let fork_point = checkout.git(&["rev-parse", "HEAD"]);
    checkout.git(&["checkout", "--quiet", "-b", "feature"]);
    checkout.commit("second");

    let (stdout, _) = checkout.run(&[("CI", "1"), ("GITHUB_BASE_REF", "main")]);
    assert_eq!(stdout, fork_point);
}

#[test]
fn an_explicit_base_ref_overrides_the_branch_the_environment_names() {
    let checkout = Checkout::new("override");
    checkout.publish_main();
    let fork_point = checkout.git(&["rev-parse", "HEAD"]);
    checkout.git(&["checkout", "--quiet", "-b", "release"]);
    checkout.publish_release();
    checkout.commit("second");

    // GITHUB_BASE_REF names `release`, but the explicit override wins.
    let (stdout, _) = checkout.run(&[
        ("CI", "1"),
        ("GITHUB_BASE_REF", "release"),
        ("NODE_COUNT_NX_BASE_REF", "main"),
    ]);
    assert_eq!(stdout, fork_point);
}

#[test]
fn a_push_build_scopes_against_the_commits_own_parent() {
    // A push build is *on* the base branch, so there is no merge base — but there
    // is a one-commit diff. Scoping against the parent is what keeps
    // merge-to-main on the affected tier instead of falling open to a full sweep.
    let checkout = Checkout::new("push");
    let parent = checkout.git(&["rev-parse", "HEAD"]);
    checkout.commit("second");

    let (stdout, stderr) = checkout.run(&[("CI", "1")]);
    assert_eq!(stdout, parent, "stderr was: {stderr}");
}

#[test]
fn a_push_build_of_a_root_commit_falls_closed_to_the_full_sweep() {
    let checkout = Checkout::new("root");

    let (stdout, stderr) = checkout.run(&[("CI", "1")]);
    assert!(
        stdout.is_empty(),
        "printed {stdout:?}; a base nobody can derive must scope nothing"
    );
    assert!(stderr.contains("every project runs"), "{stderr}");
}

#[test]
fn a_branch_name_that_is_not_one_falls_closed_to_the_full_sweep() {
    let checkout = Checkout::new("bad-ref");
    checkout.publish_main();

    let (stdout, stderr) = checkout.run(&[("CI", "1"), ("GITHUB_BASE_REF", "main; rm -rf /")]);
    assert!(
        stdout.is_empty(),
        "printed {stdout:?}; an unusable ref must scope nothing"
    );
    assert!(stderr.contains("is not a usable branch name"), "{stderr}");
}

#[test]
fn an_unfetched_base_branch_falls_closed_to_the_full_sweep() {
    // The base branch exists by name but the checkout has no ref for it — the
    // shallow-clone case. Nothing can be derived, so nothing is scoped.
    let checkout = Checkout::new("unfetched");
    checkout.git(&["checkout", "--quiet", "-b", "feature"]);
    checkout.commit("second");

    let (stdout, stderr) = checkout.run(&[("CI", "1"), ("GITHUB_BASE_REF", "main")]);
    assert!(
        stdout.is_empty(),
        "printed {stdout:?}; an underivable base must scope nothing"
    );
    assert!(
        stderr.contains("no merge base against origin/main"),
        "{stderr}"
    );
}
