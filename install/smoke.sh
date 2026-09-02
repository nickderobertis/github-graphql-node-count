#!/usr/bin/env bash
# Prove the path a consumer actually takes: `cargo add github-graphql-node-count`.
#
# `just bootstrap` sets up the *dev* environment; it says nothing about whether
# the crate a consumer downloads builds and works. This packages the crate exactly
# as `cargo publish` would, then builds the README's own dependency declaration
# and its own example against that package, in a throwaway project of its own — so
# a file missing from the package, a `readme`/`include` path that does not survive
# packaging, a public item that only resolves inside this workspace, or a README
# version requirement the crate has outgrown fails here rather than in somebody
# else's build.
#
# The dependency is declared with the **version requirement the README tells
# consumers to write**, read out of README.md rather than repeated here, alongside
# a path to the packaged crate. Cargo resolves through the path and still checks
# the requirement, so the documented declaration is what is proven. A bare
# registry declaration cannot be: cargo would have to resolve the name from
# crates.io, which is exactly what this job exists to run *before*.
#
# It reaches crates.io to resolve the dependency, so it is not part of `just
# check`; CI runs it as its own job.
# Every step that can fail says what to do next: this script's output is the whole
# of what a failing `install` job tells the next reader.
set -euo pipefail

fail() {
  echo "install-smoke: $1" >&2
  echo "ACTION: $2" >&2
  exit 1
}

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT" ||
  fail "cannot enter the repository root $ROOT" \
    "run this from a checkout whose directories are readable"

readonly CRATE="github-graphql-node-count"

# The version requirement README.md tells a consumer to put in their manifest.
# Read rather than restated, so the two cannot drift.
requirement="$(
  sed -n 's/^'"$CRATE"' *= *"\([^"]*\)".*/\1/p' README.md | head -1
)" || fail "could not read README.md" "run this from a checkout whose files are readable"
[ -n "$requirement" ] ||
  fail "README.md declares no '$CRATE = \"...\"' dependency line" \
    "keep the README's install snippet in the documented form, e.g. $CRATE = \"0\""
# It is interpolated into a generated manifest, so its shape is checked here: a
# cargo version requirement is digits, `.`, and the comparison characters.
printf '%s' "$requirement" | grep -Eq '^[0-9^~=<>* .,+-]+$' ||
  fail "README.md declares $CRATE = \"$requirement\", which is not a version requirement" \
    "write a cargo version requirement, e.g. $CRATE = \"0\""

version="$(
  cargo metadata --no-deps --format-version 1 --manifest-path Cargo.toml |
    python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"] == "'"$CRATE"'"))'
)" || fail "could not read $CRATE's version from cargo metadata" \
  "run 'cargo metadata --no-deps' and fix what it reports"

# `--allow-dirty` so the recipe is usable mid-change; CI's checkout is clean, so
# there it packages exactly the committed tree.
cargo package --locked --allow-dirty -p "$CRATE" >/dev/null ||
  fail "'cargo package' failed — the crate a consumer would download does not build" \
    "re-run 'cargo package -p $CRATE' and fix what it reports"

packaged="$ROOT/target/package/$CRATE-$version"
[ -d "$packaged" ] ||
  fail "cargo packaged no directory at $packaged" \
    "check 'cargo package -p $CRATE' output for the path it wrote"

consumer="$(mktemp -d)" ||
  fail "could not create a temporary directory for the consumer project" \
    "check that TMPDIR is writable and has free space (df -h)"
trap 'rm -rf "$consumer"' EXIT

mkdir -p "$consumer/src" ||
  fail "could not create $consumer/src" \
    "check that TMPDIR is writable and has free space (df -h)"
cat >"$consumer/Cargo.toml" <<TOML ||
  fail "could not write the consumer manifest under $consumer" \
    "check that TMPDIR is writable and has free space (df -h)"
[package]
name = "install-smoke"
version = "0.0.0"
edition = "2021"

# An empty table so this throwaway project is its own workspace.
[workspace]

[dependencies]
$CRATE = { version = "$requirement", path = "$packaged" }
TOML

# The README's example, verbatim in spirit: what a consumer writes on day one.
cat >"$consumer/src/main.rs" <<'RUST' ||
  fail "could not write the consumer program under $consumer" \
    "check that TMPDIR is writable and has free space (df -h)"
use github_graphql_node_count::{node_count, Variables, NODE_LIMIT};

fn main() {
    let document = r#"
        query($repos: Int!) {
          viewer {
            repositories(first: $repos) {
              edges { node { name issues(first: 10) { edges { node { title } } } } }
            }
          }
        }
    "#;
    let variables = Variables::from([("repos".to_string(), 50)]);

    let nodes = node_count(document, &variables).expect("a well-formed document counts");
    assert_eq!(nodes, 550, "GitHub's own worked example is 550 nodes");
    assert!(nodes < NODE_LIMIT);
    println!("install-smoke: {nodes} nodes, under the {NODE_LIMIT} limit");
}
RUST

(cd "$consumer" && cargo run --quiet) ||
  fail "a fresh project depending on the packaged $CRATE failed to build or run" \
    "reproduce against $packaged — that directory is what a consumer downloads"
