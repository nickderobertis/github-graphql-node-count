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
#
# `CRATES_API` overrides the registry base so this script can be driven against a
# local server in a test; it defaults to crates.io.
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
api="${CRATES_API:-https://crates.io/api/v1/crates}"
# It reaches curl as a URL, so its shape is checked rather than trusted: an
# http(s) origin and path, and nothing that could be read as another argument.
if ! printf '%s' "$api" | grep -Eq '^https?://[A-Za-z0-9._~:/?#@!$&()*+,;=%-]+$'; then
  echo "release-probe: CRATES_API is not an http(s) URL: $api" >&2
  echo "ACTION: unset CRATES_API to use crates.io, or set it to an http(s) registry base" >&2
  exit 1
fi
ids="$(sed -n 's/^id *= *"\([^"]*\)".*/\1/p' "$declaration")" || {
  echo "release-probe: could not read $declaration" >&2
  echo "ACTION: check that it is readable and well-formed TOML" >&2
  exit 1
}
if [ -z "$ids" ]; then
  echo "release-probe: $declaration declares no release targets" >&2
  echo "ACTION: add a [[target]] with an id, or stop calling the probe" >&2
  exit 1
fi

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
  # The name reaches a URL, so its shape is checked rather than trusted — a
  # crates.io package name is letters, digits, `-` and `_`, and never empty.
  if ! printf '%s' "$name" | grep -Eq '^[A-Za-z0-9][A-Za-z0-9_-]*$'; then
    echo "release-probe: '$id' does not name a crates.io package" >&2
    echo "ACTION: fix the id in release-targets.toml — it is 'crate:<name>'" >&2
    status=1
    continue
  fi
  # A 404 means the crate is genuinely not published yet; anything else — a
  # network failure, a 5xx, a rate limit — is this script failing to find out,
  # which must not be reported as `absent`. Ask curl for the status separately so
  # the two are told apart.
  body="$(
    curl --silent --max-time 30 --write-out '\n%{http_code}' \
      -H 'User-Agent: github-graphql-node-count release-probe' \
      "${api}/${name}"
  )" || {
    echo "release-probe: could not reach crates.io for '$id'" >&2
    echo "ACTION: check network access to ${api}, then re-run" >&2
    status=1
    continue
  }
  code="${body##*$'\n'}"
  case "$code" in
  200) ;;
  404)
    printf '%s absent\n' "$id"
    continue
    ;;
  *)
    echo "release-probe: the registry answered HTTP $code for '$id'" >&2
    echo "ACTION: retry in a minute; if it persists, check https://status.crates.io/" >&2
    status=1
    continue
    ;;
  esac
  version="$(printf '%s' "${body%$'\n'*}" |
    sed -n 's/.*"max_stable_version":"\([^"]*\)".*/\1/p')"
  # The registry's answer is third-party input, so its shape is checked before it
  # is printed as this repository's answer: a semver version is digits, letters,
  # `.`, `-` and `+`.
  if ! printf '%s' "$version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+[0-9A-Za-z.+-]*$'; then
    echo "release-probe: the registry returned no usable version for '$id'" >&2
    echo "ACTION: inspect ${api}/${name} — the response shape may have changed" >&2
    status=1
    continue
  fi
  printf '%s %s\n' "$id" "$version"
done <<<"$ids"

exit "$status"
