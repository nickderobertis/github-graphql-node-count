#!/usr/bin/env bash
# Prove the path a consumer actually takes: `cargo add github-graphql-node-count`.
#
# `just bootstrap` sets up the *dev* environment; it says nothing about whether
# the crate a consumer downloads builds and works. This packages the crate
# exactly as `cargo publish` would, then compiles and runs the README's example
# against that package in a throwaway project of its own — so a file missing from
# the package, a `readme`/`include` path that does not survive packaging, or a
# public item that only resolves inside this workspace fails here rather than in
# somebody else's build.
#
# It reaches crates.io to resolve the dependency, so it is not part of `just
# check`; CI runs it as its own job.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT" || {
  echo "install-smoke: cannot enter the repository root $ROOT" >&2
  echo "ACTION: run this from a checkout whose directories are readable" >&2
  exit 1
}

readonly CRATE="github-graphql-node-count"

version="$(
  cargo metadata --no-deps --format-version 1 --manifest-path Cargo.toml |
    python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"] == "'"$CRATE"'"))'
)" || {
  echo "install-smoke: could not read $CRATE's version from cargo metadata" >&2
  echo "ACTION: run 'cargo metadata --no-deps' and fix what it reports" >&2
  exit 1
}

# `--allow-dirty` so the recipe is usable mid-change; CI's checkout is clean, so
# there it packages exactly the committed tree.
if ! cargo package --locked --allow-dirty -p "$CRATE" >/dev/null; then
  echo "install-smoke: 'cargo package' failed — the crate a consumer would download does not build" >&2
  echo "ACTION: re-run 'cargo package -p $CRATE' and fix what it reports" >&2
  exit 1
fi

packaged="$ROOT/target/package/$CRATE-$version"
[ -d "$packaged" ] || {
  echo "install-smoke: cargo packaged no directory at $packaged" >&2
  echo "ACTION: check 'cargo package -p $CRATE' output for the path it wrote" >&2
  exit 1
}

consumer="$(mktemp -d)"
trap 'rm -rf "$consumer"' EXIT

mkdir -p "$consumer/src"
cat >"$consumer/Cargo.toml" <<TOML
[package]
name = "install-smoke"
version = "0.0.0"
edition = "2021"

# An empty table so this throwaway project is its own workspace.
[workspace]

[dependencies]
$CRATE = { path = "$packaged" }
TOML

# The README's example, verbatim in spirit: what a consumer writes on day one.
cat >"$consumer/src/main.rs" <<'RUST'
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

if ! (cd "$consumer" && cargo run --quiet); then
  echo "install-smoke: a fresh project depending on the packaged $CRATE failed to build or run" >&2
  echo "ACTION: reproduce in $packaged — the package is what a consumer downloads" >&2
  exit 1
fi
