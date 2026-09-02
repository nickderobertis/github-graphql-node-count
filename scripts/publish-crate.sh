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
# trusted before any part of it reaches a URL or a comparison.
version="${tag#v}"
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] ||
  fail "tag '$tag' is not vX.Y.Z; refusing to publish" \
    "tag releases as vX.Y.Z — release-plz does this, so an odd tag means a hand-made one"

manifest_version="$(
  sed -n 's/^version *= *"\([^"]*\)".*/\1/p' "$MANIFEST" | head -1
)"
if [ -z "$manifest_version" ]; then
  # The crate inherits its version from [workspace.package], which is the shape
  # release-plz writes.
  manifest_version="$(
    sed -n '/^\[workspace\.package\]/,/^\[/ s/^version *= *"\([^"]*\)".*/\1/p' Cargo.toml | head -1
  )"
fi
[ -n "$manifest_version" ] ||
  fail "could not read $CRATE's version from $MANIFEST or [workspace.package]" \
    "check that the manifest still declares a version release-plz can write"

[ "$version" = "$manifest_version" ] ||
  fail "tag '$tag' names $version but the manifest says $manifest_version" \
    "delete the tag and let release-plz cut it, rather than publishing a version nobody reviewed"

# Idempotent: a version already live is skipped, so re-running after a partial
# failure is safe.
api="${CRATES_API:-https://crates.io/api/v1/crates}"
if curl --silent --fail --max-time 30 "$api/$CRATE/$version" >/dev/null; then
  echo "$CRATE $version is already on crates.io; nothing to do"
  exit 0
fi

if [ -n "${PUBLISH_DRY_RUN:-}" ]; then
  echo "would publish $CRATE $version from $MANIFEST"
  exit 0
fi
cargo publish --locked --manifest-path "$MANIFEST"
