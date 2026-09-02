# Canonical command surface for github-graphql-node-count.
#
# `just bootstrap` works from a clean clone; `just check` is the deterministic
# quality gate and `just gate` is the complete pre-push bar (`check` plus the
# diff-scoped llmlint tier). Recipes are quiet on success and specific on failure.
#
# The repo-wide verbs delegate to Nx, which fans a uniformly-named target across
# every project rather than looping over projects by hand. What a target *does*
# stays with its project — the `_crate-*` recipes below are the Rust crates' own
# tools, named by each crate's project.json.

set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

# llmlint: ignore-file[tool_output_is_signal] recipes that hand straight to cargo,
# clippy, rustdoc or cargo-deny inherit those tools' diagnostics, which already name the
# exact problem and its fix; a wrapper message would bury them. The recipes whose failure
# needs project-level context (_cargo-bootstrap, _crate-fmt-check, _crate-test,
# _coverage-report, deps-check, the lint-llm tier) add one explicitly.

# The line-coverage floor, over the library's own source. 100 rather than the 95
# default because this library is pure computation with no I/O, no platform
# branches and no unreachable error path — see "Coverage" in AGENTS.md.
coverage-floor := "100"

# Test sources are excluded from the measurement: the floor is about the library,
# not about the suite that drives it.
coverage-exclude := '(^|/)tests/'

# Keep the gate's own output to signal: successes are silent, failures are not.
export CARGO_TERM_QUIET := "true"

# List available recipes.
default:
    @just --list

# Set up the project from a clean clone.
bootstrap:
    @just nx run-many -t bootstrap --parallel=1

# The Rust toolchain and cargo dev tools every crate project needs.
_cargo-bootstrap:
    @rustup show active-toolchain >/dev/null 2>&1 || rustup toolchain install
    @rustup component add rustfmt clippy llvm-tools >/dev/null \
      || { echo "cannot add toolchain components — install rustup (https://rustup.rs/) and re-run" >&2; exit 1; }
    @just _ensure-tool cargo-nextest
    @just _ensure-tool cargo-llvm-cov
    @cargo fetch --locked --quiet

# These are test runners, not rules: their version cannot change the gate's
# verdict, so this and CI both take the latest rather than keeping two pins that
# drift apart.
_ensure-tool tool:
    @command -v {{tool}} >/dev/null 2>&1 || cargo install {{tool}} --locked --quiet

# The deterministic quality gate: formatting, lint, docs, tests and the coverage
# floor. It is offline and credential-free — keep it that way.
#
# `just check` runs the AFFECTED tier (the projects this branch's diff can reach,
# keyed off an explicitly derived merge base); `just check all` runs the BROADER
# tier, one full sweep over every project. The tier is a flag on this one command
# rather than a second gate, so local and CI cannot drift. CI runs the broader
# tier at release-prep — on the release PR — because this repo batches releases;
# see "Commits, releases, and merging" in AGENTS.md.
#
# Coverage is measured over the union of every project's run, so the profile
# directory is cleared first: a stale profile can only make the number look
# better, which is the one direction a floor must not be wrong in.
#
# just resolves the tier rather than a shell `if`, so `just -n check` prints the
# one command that will run and a mistyped tier aborts instead of quietly buying
# the weaker tier.
check tier="affected":
    @cargo llvm-cov clean --workspace
    @tier={{ if tier == "all" { "all" } else if tier == "affected" { "affected" } else { error("unknown tier '" + tier + "' — use 'affected' (the default) or 'all'") } }}; \
      base="$(bash scripts/merge-base.sh)"; \
      if [ "$tier" = "all" ] || [ -z "$base" ]; then \
        just nx run-many -t format-check lint doc test coverage; \
      else \
        just nx affected --base="$base" --head=HEAD -t format-check lint doc test coverage; \
      fi
    @echo "check: ok"

# The complete pre-push bar: the deterministic gate plus the LLM-judge tier scoped
# to this branch's diff. `check` stays offline and credential-free; this is the one
# that needs a harness.
gate base="origin/main": check (lint-llm-diff base)
    @echo "gate: ok"

# Every project's tests, scoped to what this branch can reach. Iterating recipe:
# it measures coverage but does not enforce the floor — `just check` does that,
# over the union.
test:
    @base="$(bash scripts/merge-base.sh)"; \
      if [ -z "$base" ]; then just nx run-many -t test; \
      else just nx affected --base="$base" --head=HEAD -t test; fi

# The end-to-end tier on its own, for iterating. It is a project of its own that
# takes the crate as an ordinary dependency, so it drives only the published
# surface; `just check` runs it at the same tier as everything else, so it is
# gated on every change rather than opt-in.
test-e2e:
    @just nx run github-graphql-node-count-e2e:test

# Lint every project this change can reach; any warning is an error.
lint:
    @base="$(bash scripts/merge-base.sh)"; \
      if [ -z "$base" ]; then just nx run-many -t lint; \
      else just nx affected --base="$base" --head=HEAD -t lint; fi

# Format the whole tree in place — formatting is not a "what changed" question.
format:
    @just nx run-many -t format

# Verify formatting without modifying files.
fmt-check:
    @just nx run-many -t format-check

# Build every crate's docs; warnings and failing doc examples are errors.
doc:
    @just nx run-many -t doc

# The coverage floor over the union of every project's test run.
coverage:
    @cargo llvm-cov clean --workspace
    @just nx run workspace:coverage

# Upgrade dependencies, then re-run the gate as a full sweep: an upgrade can
# reach any project, so the affected set would understate it.
upgrade:
    @cargo update --quiet
    @npm update --silent --no-audit --no-fund
    @just check all

# Separate from `check`: `cargo deny` fetches an advisory database, and the gate
# stays offline. CI runs this as its own job.
deps-check:
    @command -v cargo-deny >/dev/null || { echo "cargo-deny not installed: cargo install cargo-deny --locked" >&2; exit 1; }
    @command -v cargo-machete >/dev/null || { echo "cargo-machete not installed: cargo install cargo-machete --locked" >&2; exit 1; }
    @cargo deny --log-level error check
    @# machete prints the unused dependencies it finds on stdout, so keep it:
    @# hiding them would leave a failing gate with no actionable detail.
    @cargo machete

# Escape hatch for Nx itself, e.g. `just nx show projects` or `just nx graph`.
nx *ARGS:
    @bash scripts/nx.sh {{ARGS}}

# --- per-project bodies, named by each crate's project.json ----------------

# Format one crate in place.
_crate-format crate:
    @cargo fmt -p {{crate}}

# Verify one crate's formatting without modifying files.
_crate-fmt-check crate:
    @cargo fmt -p {{crate}} -- --check \
      || { echo "formatting drift above — run 'just format'" >&2; exit 1; }

# Lint one crate with clippy; any warning is an error.
_crate-lint crate:
    @cargo clippy -p {{crate}} --all-targets --locked --quiet -- -D warnings

# Build one crate's docs with warnings denied, then run its doc examples: a
# documented example that no longer compiles is a broken promise to a consumer.
_crate-doc crate:
    @RUSTDOCFLAGS="-D warnings" cargo doc -p {{crate}} --no-deps --locked --quiet
    @cargo test -p {{crate}} --doc --locked --quiet

# Run one crate's tests under coverage instrumentation, writing raw profile data
# into the shared directory instead of reporting. The floor is enforced once, by
# `_coverage-report`, over the union — a per-crate report would fail the moment a
# suite moved into a sibling project.
_crate-test crate:
    @cargo llvm-cov --no-report nextest -p {{crate}} --locked --status-level fail \
      || { echo "tests failed in {{crate}} — see the failures above" >&2; exit 1; }

# The one aggregate coverage report, and the gate's floor.
_coverage-report:
    @cargo llvm-cov report --fail-under-lines {{coverage-floor}} \
      --ignore-filename-regex '{{coverage-exclude}}' \
      || { echo "line coverage of the library's own source fell below {{coverage-floor}}% — cover the lines the table above counts as missed" >&2; exit 1; }

# --- session and LLM-judge tiers -------------------------------------------

# Ensures `just`, verifies the rest, then runs setup-llmlint. Runs automatically
# via the Claude Code SessionStart hook; this is the manual entry point.
session-setup:
    ./scripts/session-setup.sh

# Install/refresh the llmlint toolchain (oneharness + llmlint). Idempotent.
setup-llmlint:
    ./scripts/setup-llmlint.sh

# LLM-judge lint — the non-deterministic, harness-backed tier. Kept OUT of `check`
# on purpose: the deterministic gate stays offline and credential-free. Config is
# the composed `llmlint.yml`.
lint-llm *paths:
    @command -v llmlint >/dev/null 2>&1 || { echo "llmlint not installed — run 'just setup-llmlint'" >&2; exit 1; }
    llmlint {{paths}}

# Fast, deterministic llmlint gate — no model calls, no harness credential. CI
# runs it before the model tier so a broken config fails in milliseconds instead
# of spending a harness call.
lint-llm-validate *args:
    @command -v llmlint >/dev/null 2>&1 || { echo "llmlint not installed — run 'just setup-llmlint'" >&2; exit 1; }
    llmlint validate {{args}}

# llmlint scoped to the files this branch changed since it forked from main. A
# plain `--diff-base <ref>` is three-dot/merge-base, so the judge sees only what
# the branch introduced. This is the blocking `llmlint` PR check.
lint-llm-diff base="origin/main" *args:
    @command -v llmlint >/dev/null 2>&1 || { echo "llmlint not installed — run 'just setup-llmlint'" >&2; exit 1; }
    llmlint --diff --diff-base "{{base}}" {{args}}
