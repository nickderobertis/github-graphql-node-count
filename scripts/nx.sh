#!/usr/bin/env bash
# The one entry point to this workspace's Nx.
#
# Nx lives in `node_modules/.bin`, which a fresh clone does not have, so every
# invocation heals through a locked install first. That is what lets `just check`
# work from a clean clone with no separate "install the orchestrator" step, and
# what keeps one recipe from failing with `nx: command not found` while another
# quietly repaired it.
#
# Quiet on success, specific on failure: a green run owes a line rather than Nx's
# whole task log, and the log is written to a file both messages name so a reader
# can follow it live and still read it afterwards. `NODE_COUNT_NX_SHOW_OUTPUT=1`
# streams instead, for the callers that parse Nx's stdout.
#
# Nx orchestrates targets; it is never a runtime dependency of what they run.
# Each target shells out to the project's own language-native tool.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT" || {
  echo "nx: cannot enter the repository root $ROOT" >&2
  echo "ACTION: run this from a checkout whose directories are readable" >&2
  exit 1
}

# The daemon is a long-lived background process per workspace root, buying about
# a tenth of a second here; it is not worth a resident process the gate never
# reaps. `NX_DAEMON=true` turns it back on for anyone who wants it.
export NX_DAEMON="${NX_DAEMON-false}"
# Keep a daemon that *is* turned back on from fetching its own `nx@latest` for
# housekeeping: this workspace's pinned Nx is the only one that may run.
export NX_USE_LOCAL=true

if [ ! -e node_modules/.bin/nx ] && [ ! -e node_modules/.bin/nx.cmd ]; then
  if ! command -v npm >/dev/null 2>&1; then
    echo "nx: npm not found; cannot install the pinned Nx the project graph needs" >&2
    echo "ACTION: install Node.js 20+ (https://nodejs.org/) and re-run 'just bootstrap'" >&2
    exit 1
  fi
  # Installer chatter is not this command's output: in show-output mode stdout is
  # read for Nx's answer, so anything that is not that answer goes to stderr.
  if ! npm ci --silent --no-audit --no-fund >&2; then
    echo "nx: 'npm ci' failed in $ROOT" >&2
    echo "ACTION: check network access to the npm registry, then re-run 'just bootstrap'" >&2
    exit 1
  fi
fi

# The npm-written shim rather than a path inside the package: Nx has moved its
# bin entry between releases, and the shim is the one name that cannot.
NX_BIN="node_modules/.bin/nx"
[ -e "$NX_BIN" ] || NX_BIN="node_modules/.bin/nx.cmd"

# Streaming mode: hand Nx's own streams through untouched. The callers that ask
# for this parse stdout, so nothing may be added to it — not even a summary line.
if [ "${NODE_COUNT_NX_SHOW_OUTPUT:-}" = "1" ]; then
  exec "$NX_BIN" "$@"
fi

mkdir -p .logs
log="$ROOT/.logs/nx.log"
if "$NX_BIN" "$@" >"$log" 2>&1; then
  echo "nx: requested targets succeeded (full output: $log)"
  exit 0
fi
cat "$log" >&2
echo "nx: targets failed; fix the findings above and rerun the same 'just' recipe (full output: $log)" >&2
exit 1
