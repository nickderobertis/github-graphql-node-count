#!/usr/bin/env bash
# Which gate tier a CI run is for: `all` or `affected`.
#
# The broader tier runs at exactly one lifecycle point — release-prep, which for
# this repository is the release-plz release PR, because the commit that PR ships
# is one no merge job swept. Everything else, including a push to `main`, runs the
# affected tier.
#
# It lives here rather than inline in the workflow so the decision is testable:
# picking the wrong tier does not fail loudly, it silently under-gates the one
# commit that most needs the full sweep.
#
# Usage: scripts/gate-tier.sh "$GITHUB_HEAD_REF"   (empty on a push build)
set -euo pipefail

head_ref="${1-}"

# The branch release-plz opens its release PR from. Matching a prefix rather than
# an exact name because the bot appends the branch it targets.
case "$head_ref" in
release-plz-*) printf 'all\n' ;;
*) printf 'affected\n' ;;
esac
