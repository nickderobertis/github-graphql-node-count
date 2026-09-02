//! `install/smoke.sh`, the install-path suite's own refusals.
//!
//! The happy path — package the crate, build the README's declaration against it
//! — is what the `install` CI job runs for real on every change, so it is not
//! repeated here. What is covered here is what that job *cannot* show: the
//! refusals it takes when the README stops declaring the dependency the way the
//! documentation says to, which is the drift this suite exists to catch.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A throwaway tree holding the script and a README to read.
struct Checkout {
    root: PathBuf,
}

impl Drop for Checkout {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

impl Checkout {
    fn with_readme(label: &str, readme: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "install-smoke-{label}-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        std::fs::create_dir_all(root.join("install")).expect("create the checkout");
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../install/smoke.sh")
            .canonicalize()
            .expect("the script under test exists");
        std::fs::copy(&source, root.join("install/smoke.sh")).expect("copy the script");
        std::fs::write(root.join("README.md"), readme).expect("write the README");
        Self { root }
    }

    fn run(&self) -> Output {
        Command::new("bash")
            .arg("install/smoke.sh")
            .current_dir(&self.root)
            .output()
            .expect("run smoke.sh")
    }
}

fn stderr_of(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("utf-8")
}

#[test]
fn a_readme_without_the_documented_declaration_is_refused() {
    // If the README stops telling consumers how to depend on the crate, this
    // suite has nothing to prove and says so, rather than inventing a
    // requirement and passing.
    let checkout = Checkout::with_readme(
        "no-declaration",
        "# github-graphql-node-count\n\nJust prose.\n",
    );

    let output = checkout.run();
    assert!(
        !output.status.success(),
        "a README with no declaration must not pass"
    );
    let stderr = stderr_of(&output);
    assert!(stderr.contains("declares no"), "{stderr}");
    assert!(
        stderr.contains("ACTION:"),
        "a refusal says what to do next: {stderr}"
    );
}

#[test]
fn a_declaration_that_is_not_a_version_requirement_is_refused() {
    // The value is interpolated into a generated manifest, so its shape is
    // checked at that boundary rather than trusted.
    let checkout = Checkout::with_readme(
        "not-a-requirement",
        "# x\n\n```toml\ngithub-graphql-node-count = \"latest, or whatever\"\n```\n",
    );

    let output = checkout.run();
    assert!(
        !output.status.success(),
        "a non-requirement must not reach a generated manifest"
    );
    assert!(
        stderr_of(&output).contains("is not a version requirement"),
        "{}",
        stderr_of(&output)
    );
}

#[test]
fn the_repositorys_own_readme_declares_a_usable_requirement() {
    // The other side of the same gate, asserted against the real README: the
    // documented install snippet is in the form the install job can prove.
    let readme =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../README.md"))
            .expect("the repository README");
    let declared = readme
        .lines()
        .find_map(|line| {
            line.strip_prefix("github-graphql-node-count = \"")?
                .strip_suffix('"')
        })
        .expect("README.md declares the dependency the documented way");
    assert!(
        declared
            .chars()
            .all(|c| c.is_ascii_digit() || "^~=<>* .,+-".contains(c)),
        "README declares {declared:?}, which is not a cargo version requirement",
    );
}
