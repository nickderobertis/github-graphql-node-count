//! Hold the release pull request to merging itself behind its required checks.
//!
//! `.github/workflows/release-plz.yml` opens or updates the release pull request
//! and then arms GitHub auto-merge on it with the release PAT, so a release has no
//! human step and still cannot merge past a check. Running that workflow for real
//! needs the PAT and the live repository, so its wiring is read here instead: one
//! arming step, after `release-plz release-pr` in the same job, `--auto` on every
//! `gh pr merge` and `--admin` on none. The arming step's own script needs no
//! secret, so it is also run, under `bash`, with only `gh` doubled.

#![cfg(unix)]

mod support;

use std::path::{Path, PathBuf};

use serde_norway::Value;

use support::{read, repo_root};

const WORKFLOW: &str = ".github/workflows/release-plz.yml";
const RELEASE_TOKEN: &str = "${{ secrets.RELEASE_PLZ_TOKEN }}";
const ARMING_STEP: &str = "Auto-merge the release PR when the gate is green";

fn workflow_text() -> String {
    read(&repo_root().join(WORKFLOW))
}

fn parse(text: &str) -> Value {
    serde_norway::from_str(text).unwrap_or_else(|error| panic!("{WORKFLOW}: {error}"))
}

fn run_of(step: &Value) -> &str {
    step["run"].as_str().unwrap_or_default()
}

/// Every job's name and steps, in the order the workflow declares them.
fn jobs(workflow: &Value) -> Vec<(&str, &[Value])> {
    workflow["jobs"]
        .as_mapping()
        .expect("the workflow declares jobs")
        .iter()
        .map(|(name, job)| {
            (
                name.as_str().expect("a job name"),
                job["steps"].as_sequence().map_or(&[][..], Vec::as_slice),
            )
        })
        .collect()
}

/// Whether a step's command runs `release-plz release-pr`.
fn runs_release_pr(step: &Value) -> bool {
    run_of(step)
        .split_whitespace()
        .collect::<Vec<_>>()
        .windows(2)
        .any(|pair| pair == ["release-plz", "release-pr"])
}

/// Each `gh pr merge` command in a script, a fallback chained on the same line
/// included, as its words: continuation lines joined, cut at the operator that
/// ends the command.
fn gh_pr_merges(run: &str) -> Vec<Vec<String>> {
    run.replace("\\\n", " ")
        .lines()
        .flat_map(|line| line.split("gh pr merge").skip(1))
        .map(|rest| {
            rest.split_whitespace()
                .take_while(|word| !matches!(*word, "||" | "&&" | ";" | "|"))
                .map(str::to_owned)
                .collect()
        })
        .collect()
}

/// What is wrong with a workflow's auto-merge wiring; empty when nothing is.
///
/// A function over the text rather than assertions inline, so the same checks can
/// be shown to refuse each way the wiring can break.
fn arming_violations(text: &str) -> Vec<String> {
    let workflow = parse(text);
    let mut violations = Vec::new();

    for (job, steps) in jobs(&workflow) {
        for step in steps {
            let run = run_of(step);
            for words in gh_pr_merges(run) {
                if !words.iter().any(|word| word == "--auto") {
                    violations.push(format!(
                        "job `{job}` runs a gh pr merge without --auto, which merges past \
                         the checks: gh pr merge {words:?}"
                    ));
                }
            }
            if run.contains("--admin") {
                violations.push(format!(
                    "job `{job}` passes --admin, which forces past the checks: {run}"
                ));
            }
        }
    }

    let arming: Vec<(&str, usize, &Value)> = jobs(&workflow)
        .into_iter()
        .flat_map(|(job, steps)| {
            steps
                .iter()
                .enumerate()
                .filter(|(_, step)| !gh_pr_merges(run_of(step)).is_empty())
                .map(move |(index, step)| (job, index, step))
        })
        .collect();
    let [(job, index, step)] = arming[..] else {
        violations.push(format!(
            "exactly one step arms auto-merge on the release PR, found {}",
            arming.len()
        ));
        return violations;
    };

    if step["name"].as_str() != Some(ARMING_STEP) {
        violations.push(format!("the arming step is named `{ARMING_STEP}`"));
    }

    let opening: Vec<(&str, usize)> = jobs(&workflow)
        .into_iter()
        .flat_map(|(job, steps)| {
            steps
                .iter()
                .enumerate()
                .filter(|(_, step)| runs_release_pr(step))
                .map(move |(index, _)| (job, index))
        })
        .collect();
    match opening[..] {
        [(opening_job, _)] if opening_job != job => violations.push(format!(
            "the arming step runs in job `{job}`, not `{opening_job}` where release-plz \
             opens the release PR"
        )),
        [(_, opens_at)] if opens_at > index => violations.push(
            "the arming step runs before `release-plz release-pr` has opened or updated \
             the release PR"
                .to_owned(),
        ),
        [_] => {}
        _ => violations.push(format!(
            "exactly one step runs `release-plz release-pr`, found {}",
            opening.len()
        )),
    }

    // The PAT, and nothing else: a merge the default token makes pushes a commit
    // that triggers no workflow, so the release would never be tagged.
    let env = step["env"].as_mapping();
    if step["env"]["GH_TOKEN"].as_str() != Some(RELEASE_TOKEN)
        || env.map(|env| env.len()) != Some(1)
    {
        violations.push(format!(
            "the arming step hands gh only GH_TOKEN: {RELEASE_TOKEN}, got {:?}",
            step["env"]
        ));
    }

    let run = run_of(step);
    if !(run.contains("gh pr list --repo \"$GITHUB_REPOSITORY\" --state open")
        && run.contains("--json number,headRefName")
        && run.contains("startswith(\"release-plz-\")"))
    {
        violations.push(format!(
            "the arming step selects the open release-plz-* pull request: {run}"
        ));
    }
    for words in gh_pr_merges(run) {
        for flag in ["--auto", "--squash"] {
            if !words.iter().any(|word| word == flag) {
                violations.push(format!(
                    "the release PR's merge passes {flag}: gh pr merge {words:?}"
                ));
            }
        }
    }

    violations
}

#[test]
fn the_release_pull_request_arms_auto_merge_behind_its_checks() {
    let violations = arming_violations(&workflow_text());
    assert!(
        violations.is_empty(),
        "{WORKFLOW}:\n{}",
        violations.join("\n")
    );
}

/// The arming step's text, from its name to the end of its `run` block.
fn arming_block(text: &str) -> &str {
    let start = text
        .find(&format!("      - name: {ARMING_STEP}"))
        .expect("the arming step");
    let end = text[start..]
        .find("\n\n")
        .map_or(text.len(), |offset| start + offset + 1);
    &text[start..end]
}

/// Each way the wiring can break, applied to the real workflow, is refused — so
/// the check above passing means something.
#[test]
fn every_broken_wiring_is_refused() {
    let text = workflow_text();
    let block = arming_block(&text);
    let without_step = text.replacen(block, "", 1);
    let opener = "      - name: Open or update the release PR";
    let replace = |from: &str, to: &str| {
        assert!(text.contains(from), "the workflow contains {from:?}");
        text.replacen(from, to, 1)
    };

    // Each case, and the refusal it must draw: a case refused for some other
    // reason would prove nothing about the one it names.
    let broken: Vec<(&str, &str, String)> = vec![
        (
            "the step is missing",
            "exactly one step arms auto-merge",
            without_step.clone(),
        ),
        (
            "it precedes release-plz release-pr",
            "runs before `release-plz release-pr`",
            without_step.replacen(opener, &format!("{block}\n{opener}"), 1),
        ),
        (
            "it sits in another job",
            "runs in job `release-merge`",
            format!(
                "{without_step}\n  release-merge:\n    runs-on: ubuntu-latest\n    steps:\n{block}"
            ),
        ),
        (
            "it takes the default token",
            "hands gh only GH_TOKEN",
            replace(
                &format!("GH_TOKEN: {RELEASE_TOKEN}"),
                "GH_TOKEN: ${{ github.token }}",
            ),
        ),
        (
            "it drops --auto",
            "without --auto",
            replace(" --auto --squash", " --squash"),
        ),
        (
            "it drops --squash",
            "passes --squash",
            replace(" --auto --squash", " --auto"),
        ),
        (
            "it carries --admin",
            "passes --admin",
            replace("--delete-branch", "--delete-branch --admin"),
        ),
        (
            "it falls back to a merge without --auto",
            "without --auto",
            replace(
                "|| echo \"::warning::",
                "|| gh pr merge \"$pr\" --squash || echo \"::warning::",
            ),
        ),
        (
            "another step merges without --auto",
            "without --auto",
            replace(
                opener,
                &format!("      - run: gh pr merge 1 --squash\n\n{opener}"),
            ),
        ),
        (
            "another step passes --admin",
            "passes --admin",
            replace(
                opener,
                &format!("      - run: gh pr merge 1 --auto --squash --admin\n\n{opener}"),
            ),
        ),
    ];
    for (how, refusal, text) in broken {
        let violations = arming_violations(&text);
        assert!(
            violations
                .iter()
                .any(|violation| violation.contains(refusal)),
            "a workflow where {how} must be refused with `{refusal}`, got {violations:?}:\n{text}"
        );
    }
}

const REPOSITORY: &str = "nickderobertis/github-graphql-node-count";
const TOKEN: &str = "release-pat-for-the-double";

/// What `gh pr list --state open --json number,headRefName` prints with the
/// release PR open among a feature branch and a dependency bump.
const OPEN_WITH_RELEASE: &str = r#"[
  {"headRefName":"nick/count/fragment-spreads","number":12},
  {"headRefName":"release-plz-2026-10-15T09-30-00Z","number":13},
  {"headRefName":"dependabot/cargo/serde-1.0.228","number":11}
]"#;

struct Armed {
    status: std::process::ExitStatus,
    stdout: String,
    stderr: String,
    /// Each `gh` call: the `GH_TOKEN=` it saw, then its arguments.
    calls: Vec<Vec<String>>,
}

impl Armed {
    fn calls_to(&self, subcommand: &str) -> Vec<&[String]> {
        self.calls
            .iter()
            .filter(|call| call[1..].starts_with(&["pr".to_owned(), subcommand.to_owned()]))
            .map(Vec::as_slice)
            .collect()
    }
}

/// A scratch directory of this test's own, emptied first.
fn scratch(case: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("release_workflow")
        .join(format!("{case}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap_or_else(|error| panic!("{}: {error}", dir.display()));
    dir
}

/// Runs the arming step's own script, read from the real workflow, under the
/// shell GitHub runs a `run:` block with, against a `gh pr list` answering
/// `pulls` and a `gh pr merge` exiting `merge_exit`.
fn run_arming_step(case: &str, pulls: &str, merge_exit: u8) -> Armed {
    let jq = std::process::Command::new("jq").arg("--version").output();
    assert!(
        jq.is_ok_and(|output| output.status.success()),
        "jq must be on PATH: the gh double answers --jq with the real jq, as gh does"
    );
    let text = workflow_text();
    let workflow = parse(&text);
    let script = jobs(&workflow)
        .into_iter()
        .flat_map(|(_, steps)| steps)
        .find(|step| step["name"].as_str() == Some(ARMING_STEP))
        .map(run_of)
        .expect("the arming step");

    let dir = scratch(case);
    let script_path = dir.join("step.sh");
    std::fs::write(&script_path, script).expect("the step's script");
    let pulls_path = dir.join("pulls.json");
    std::fs::write(&pulls_path, pulls).expect("the canned pull requests");
    let calls_path = dir.join("calls");

    let double = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/gh-double");
    let path = std::env::join_paths(std::iter::once(double).chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .expect("a PATH");
    let output = std::process::Command::new("bash")
        .args(["--noprofile", "--norc", "-eo", "pipefail"])
        .arg(&script_path)
        .env("PATH", path)
        .env("GITHUB_REPOSITORY", REPOSITORY)
        .env("GH_TOKEN", TOKEN)
        .env_remove("GITHUB_TOKEN")
        .env("GH_DOUBLE_PULLS", &pulls_path)
        .env("GH_DOUBLE_CALLS", &calls_path)
        .env("GH_DOUBLE_MERGE_EXIT", merge_exit.to_string())
        .output()
        .expect("bash runs the arming step");
    let calls = std::fs::read_to_string(&calls_path)
        .unwrap_or_default()
        .lines()
        .map(|line| line.split('\x1f').map(str::to_owned).collect())
        .collect();
    Armed {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        calls,
    }
}

fn assert_listed_open_pulls(armed: &Armed) {
    let lists = armed.calls_to("list");
    assert_eq!(lists.len(), 1, "one gh pr list: {:?}", armed.calls);
    assert_eq!(
        lists[0][0],
        format!("GH_TOKEN={TOKEN}"),
        "gh is handed the step's token"
    );
    let args = lists[0][1..].join(" ");
    assert!(
        args.contains(&format!("--repo {REPOSITORY}")) && args.contains("--state open"),
        "the step lists this repository's open pull requests: gh {args}"
    );
}

#[test]
fn the_arming_step_arms_auto_merge_on_the_open_release_pull_request() {
    let armed = run_arming_step("arms", OPEN_WITH_RELEASE, 0);
    assert!(
        armed.status.success(),
        "the step succeeds: {}\n{}",
        armed.stdout,
        armed.stderr
    );
    assert_listed_open_pulls(&armed);
    let merges = armed.calls_to("merge");
    assert_eq!(
        merges.len(),
        1,
        "exactly one gh pr merge: {:?}",
        armed.calls
    );
    assert_eq!(
        merges[0],
        [
            &format!("GH_TOKEN={TOKEN}"),
            "pr",
            "merge",
            "13",
            "--repo",
            REPOSITORY,
            "--auto",
            "--squash",
            "--delete-branch",
        ],
        "auto-merge is armed on the release-plz-* pull request alone, with the PAT"
    );
    assert!(
        armed
            .stdout
            .contains("Enabling auto-merge on release PR #13"),
        "{}",
        armed.stdout
    );
}

#[test]
fn the_arming_step_merges_nothing_when_no_release_pull_request_is_open() {
    let open_without_release = r#"[
  {"headRefName":"nick/count/fragment-spreads","number":12},
  {"headRefName":"dependabot/cargo/serde-1.0.228","number":11}
]"#;
    for (case, pulls) in [
        ("none-among-others", open_without_release),
        ("none-open", "[]"),
    ] {
        let armed = run_arming_step(case, pulls, 0);
        assert!(
            armed.status.success(),
            "with no release PR open the step still succeeds: {}\n{}",
            armed.stdout,
            armed.stderr
        );
        assert_listed_open_pulls(&armed);
        assert!(
            armed.calls_to("merge").is_empty(),
            "no gh pr merge without an open release PR: {:?}",
            armed.calls
        );
        assert!(
            armed.stdout.contains("No open release PR to auto-merge."),
            "{}",
            armed.stdout
        );
    }
}

/// `gh pr merge --auto` refuses when auto-merge is already armed or the PR is
/// already merging; the step reports that as a warning and leaves the run green.
#[test]
fn a_refused_arming_warns_and_leaves_the_run_green() {
    let armed = run_arming_step("refused", OPEN_WITH_RELEASE, 1);
    assert!(
        armed.status.success(),
        "a refused gh pr merge must not fail the run: {}\n{}",
        armed.stdout,
        armed.stderr
    );
    assert_eq!(armed.calls_to("merge").len(), 1, "{:?}", armed.calls);
    assert!(
        armed.stdout.lines().any(|line| {
            line
            == "::warning::could not enable auto-merge on #13 (already enabled, or already merging)"
        }),
        "the step warns that arming failed: {}",
        armed.stdout
    );
}
