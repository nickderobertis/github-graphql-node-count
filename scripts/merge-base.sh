#!/usr/bin/env bash
# Print the merge base affected detection keys off, or nothing.
#
# Printing nothing is the **fail-closed** answer: the caller runs every project
# rather than reporting a scoped pass as a full one. Affected selection is a speed
# optimisation, and a speed optimisation that can silently skip a check is a
# correctness hole — so a shallow clone, a missing base branch or a detached build
# buys the full sweep and says so on stderr.
#
# llmlint: ignore-file[tool_output_is_signal] the one line each fail-closed path prints
# is exactly that correctness hole, reported: nothing else tells the reader the scope
# that ran was not the scope asked for.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT" || {
  echo "merge-base: cannot enter the repository root $ROOT" >&2
  exit 1
}

# The base branch as GitHub names it on a pull request, or the local default.
#
# `GITHUB_BASE_REF` is workflow-controlled rather than attacker-controlled, but it
# reaches `git fetch` as a refspec, so its shape is validated at the boundary
# instead of trusted: a branch name is what a branch name may look like.
#
# In CI its absence means a push build, which is *on* the base branch: there is no
# merge base, but there is a well-defined one-commit diff — the commit's first
# parent. Scoping against that keeps merge-to-main on the affected tier, so the
# broader sweep stays at exactly one lifecycle point (the release PR). A commit
# with no parent has no diff to scope by, so that falls closed to the full sweep.
branch="${NODE_COUNT_NX_BASE_REF:-${GITHUB_BASE_REF:-}}"
if [ -z "$branch" ]; then
  if [ -n "${CI:-}" ]; then
    if parent="$(git rev-parse --verify --quiet HEAD^ 2>/dev/null)"; then
      printf '%s' "$parent"
      exit 0
    fi
    echo "merge-base: a push build whose commit has no parent, so every project runs" >&2
    exit 0
  fi
  branch="main"
fi

if ! printf '%s' "$branch" | grep -Eq '^[A-Za-z0-9][A-Za-z0-9._/-]*$'; then
  echo "merge-base: '$branch' is not a usable branch name — set NODE_COUNT_NX_BASE_REF (or GITHUB_BASE_REF) to a plain one such as 'main'" >&2
  exit 0
fi

# A pull-request runner's checkout has the base branch only as a remote-tracking
# ref if it was fetched; fetch it first, so detection does not depend on how deep
# the checkout happened to be.
if [ -n "${CI:-}" ]; then
  git fetch --no-tags --quiet origin \
    "+refs/heads/$branch:refs/remotes/origin/$branch" 2>/dev/null || true
fi

if ! base="$(git merge-base "origin/$branch" HEAD 2>/dev/null)"; then
  echo "merge-base: no merge base against origin/$branch, so every project runs (git fetch --unshallow to scope it)" >&2
  exit 0
fi
printf '%s' "$base"
