#!/usr/bin/env bash
# Publish the tagged version of the library to crates.io, idempotently.
#
# Run by the tag-triggered release workflow, which passes the tag it was fired
# for. It lives here rather than inline in that workflow because everything it
# does before `cargo publish` is a refusal: a tag that is not `vX.Y.Z`, or that
# names a version the manifest does not, would publish something nobody reviewed,
# and a publish is not reversible. Those refusals are tested.
#
# Usage: scripts/publish-crate.sh <tag>
#
# Environment:
#   CARGO_REGISTRY_TOKEN  required — the registry credential.
#   CRATES_API            optional — the registry API base, for tests.
#   PUBLISH_DRY_RUN       optional — when set, print the publish rather than run
#                         it, so the refusals above can be exercised end to end.
set -euo pipefail

readonly CRATE="github-graphql-node-count"
readonly MANIFEST="crates/github-graphql-node-count/Cargo.toml"

fail() {
  echo "::error::$1" >&2
  echo "ACTION: $2" >&2
  exit 1
}

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT" ||
  fail "cannot enter the repository root $ROOT" \
    "run this from a checkout whose directories are readable"

tag="${1-}"
[ -n "$tag" ] ||
  fail "no tag given, so there is no version to publish" \
    "call this as 'scripts/publish-crate.sh vX.Y.Z'"

[ -n "${CARGO_REGISTRY_TOKEN:-}" ] ||
  fail "CARGO_REGISTRY_TOKEN is not set, so the tagged version cannot be published" \
    "add it under Settings -> Secrets and variables -> Actions"

# The tag arrives from the push event, so its shape is validated rather than
# trusted before any part of it reaches a URL or a comparison. The whole tag is
# matched, `v` included: stripping the prefix first would let a bare `1.2.3`
# through, and that is not a tag release-plz cuts.
[[ "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] ||
  fail "tag '$tag' is not vX.Y.Z; refusing to publish" \
    "tag releases as vX.Y.Z — release-plz does this, so an odd tag means a hand-made one"
version="${tag#v}"

[ -r "$MANIFEST" ] ||
  fail "cannot read $MANIFEST" \
    "run this from a checkout whose files are readable"
manifest_version="$(
  sed -n 's/^version *= *"\([^"]*\)".*/\1/p' "$MANIFEST" | head -1
)" || fail "could not read a version from $MANIFEST" \
  "check that the manifest is well-formed TOML"
if [ -z "$manifest_version" ]; then
  # The crate inherits its version from [workspace.package], which is the shape
  # release-plz writes.
  [ -r Cargo.toml ] ||
    fail "cannot read the workspace manifest Cargo.toml" \
      "run this from a checkout whose files are readable"
  manifest_version="$(
    sed -n '/^\[workspace\.package\]/,/^\[/ s/^version *= *"\([^"]*\)".*/\1/p' Cargo.toml | head -1
  )" || fail "could not read a version from [workspace.package]" \
    "check that the workspace manifest is well-formed TOML"
fi
[ -n "$manifest_version" ] ||
  fail "could not read $CRATE's version from $MANIFEST or [workspace.package]" \
    "check that the manifest still declares a version release-plz can write"

# What `sed` pulled out of the manifest is a line that looked like a version, not
# a version: the extraction is a pattern over text, so its result is held to the
# same shape the tag was before it decides anything. A manifest mangled into
# saying something else refuses here, naming what it said, rather than reaching
# the comparison below and being reported as a tag/manifest disagreement — two
# different faults that want two different fixes.
[[ "$manifest_version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] ||
  fail "the manifest's version '$manifest_version' is not X.Y.Z; refusing to publish" \
    "fix the version in $MANIFEST (or [workspace.package]) — release-plz writes a plain X.Y.Z"

[ "$version" = "$manifest_version" ] ||
  fail "tag '$tag' names $version but the manifest says $manifest_version" \
    "delete the tag and let release-plz cut it, rather than publishing a version nobody reviewed"

# Idempotent: a version already live is skipped, so re-running after a partial
# failure is safe.
api="${CRATES_API:-https://crates.io/api/v1/crates}"
# It reaches curl as a URL, so its shape is checked rather than trusted.
printf '%s' "$api" | grep -Eq '^https?://[A-Za-z0-9._~:/?#@!$&()*+,;=%-]+$' ||
  fail "CRATES_API is not an http(s) URL: $api" \
    "unset CRATES_API to use crates.io, or set it to an http(s) registry base"
if curl --silent --fail --max-time 30 "$api/$CRATE/$version" >/dev/null; then
  echo "$CRATE $version is already on crates.io; nothing to do"
  exit 0
fi

if [ -n "${PUBLISH_DRY_RUN:-}" ]; then
  echo "would publish $CRATE $version from $MANIFEST"
  exit 0
fi

# The upload itself is the one step no test can drive: publishing is irreversible,
# so there is no way to exercise it that does not push a real version to a real
# registry. Everything it can refuse on is checked above and covered by
# crates/tooling/tests/publish_crate.rs.
# llmlint: ignore[changed_behavior_has_e2e] a real `cargo publish` cannot be exercised in a
# test without irreversibly publishing to crates.io; every refusal that precedes it is driven
# for real in crates/tooling/tests/publish_crate.rs, and PUBLISH_DRY_RUN covers the path up to
# this line.
cargo publish --locked --quiet --manifest-path "$MANIFEST" ||
  fail "publishing $CRATE $version to crates.io failed" \
    "read the cargo output above; if the upload partially succeeded, re-running this workflow is safe — an already-live version is skipped"
echo "published $CRATE $version"
