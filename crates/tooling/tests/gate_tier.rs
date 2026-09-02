//! `scripts/gate-tier.sh`, which decides whether a CI run is the affected tier or
//! the full sweep.
//!
//! Picking the wrong tier does not fail loudly: it silently under-gates the one
//! commit that most needs the full sweep — the release PR's — or pays for a full
//! sweep on every ordinary change.

use std::path::Path;
use std::process::Command;

/// Run the script with `head_ref` as its argument and return its stdout, trimmed.
fn tier_for(head_ref: &str) -> String {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/gate-tier.sh");
    let output = Command::new("bash")
        .arg(&script)
        .arg(head_ref)
        .output()
        .expect("run gate-tier.sh");
    assert!(
        output.status.success(),
        "gate-tier.sh must answer, not refuse"
    );
    String::from_utf8(output.stdout)
        .expect("utf-8")
        .trim()
        .to_string()
}

#[test]
fn a_release_plz_branch_buys_the_full_sweep() {
    // release-plz names the branch after the one it targets, so the match is a
    // prefix rather than an exact name.
    assert_eq!(tier_for("release-plz-main"), "all");
    assert_eq!(tier_for("release-plz-2026-09-02"), "all");
}

#[test]
fn an_ordinary_branch_buys_the_affected_tier() {
    assert_eq!(tier_for("feat/count-inline-fragments"), "affected");
    assert_eq!(tier_for("main"), "affected");
    // A branch merely *mentioning* the bot's name is not its release PR.
    assert_eq!(tier_for("fix-release-plz-config"), "affected");
}

#[test]
fn a_push_build_with_no_head_ref_buys_the_affected_tier() {
    // `github.head_ref` is empty on a push, and merge-to-main stays on the
    // affected tier — the broader sweep belongs at release-prep and nowhere else.
    assert_eq!(tier_for(""), "affected");
}
