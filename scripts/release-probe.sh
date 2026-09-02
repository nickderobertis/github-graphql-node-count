#!/usr/bin/env bash
# What a public registry currently serves for each `id` in release-targets.toml.
#
# Reads the ids back out of that document, so there is one list rather than two
# to drift apart, and prints one `<id> <version|absent>` line per target. It
# reaches crates.io, so it is not part of `just check`; it is what a consumer (or
# a person) runs to ask whether a release has landed yet.
#
# Quiet on success in the sense that matters here: the lines it prints *are* the
# answer, and nothing else goes to stdout.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
declaration="$ROOT/release-targets.toml"

if [ ! -f "$declaration" ]; then
  echo "release-probe: $declaration is missing" >&2
  echo "ACTION: run this from a checkout of the repository" >&2
  exit 1
fi

# `id = "crate:<name>"` is the only registry this repository publishes to. A
# target naming another registry is a change this script must grow to answer, so
# it refuses rather than reporting a wrong answer.
status=0
while read -r id; do
  case "$id" in
  crate:*) ;;
  *)
    echo "release-probe: no probe for '$id' — this script only answers for crates.io" >&2
    echo "ACTION: teach it that registry, or drop the target from release-targets.toml" >&2
    status=1
    continue
    ;;
  esac
  name="${id#crate:}"
  version="$(
    curl --silent --show-error --fail --max-time 30 \
      -H 'User-Agent: github-graphql-node-count release-probe' \
      "https://crates.io/api/v1/crates/${name}" |
      sed -n 's/.*"max_stable_version":"\([^"]*\)".*/\1/p'
  )" || version=""
  printf '%s %s\n' "$id" "${version:-absent}"
done < <(sed -n 's/^id *= *"\([^"]*\)".*/\1/p' "$declaration")

exit "$status"
