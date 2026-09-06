# AGENTS.md

Durable instructions for this repo. Write for a future maintainer, not as a
session log. Deterministic steps belong in scripts; this file is for constraints,
tradeoffs, and decisions the code cannot show.

> `CLAUDE.md` is a symlink to this file, so the two never drift. Edit
> `AGENTS.md` only.

## What this repo is

One published Rust library, `github-graphql-node-count`. It answers the two
numbers GitHub charges a GraphQL document — the worst-case **node count** and the
**rate-limit points** one call spends — from the document's text and its
page-size variable bindings alone: no network, no credential, no schema. Its
consumers are other repositories' query gates.

`nodeCount` is the maximum number of nodes **one query may return**, limited per
query. It is not `cost`, the rate-limit **points** a call spends, metered per
hour across everything a credential does. Both are computed here; keep the two
apart by name everywhere, because they get confused. The crate name predates the
second answer and does not change: it is what a registry serves and what
dependents write down.

## Two standing goals on every task

The user's request is the priority, but carry two goals into *every* task. Fold
either in when it is the lowest-error path to what was asked; otherwise offer it
as a follow-up.

1. **Engineer the context for next time** — realistic tests exercising what a
   consumer sees, scripts that shrink their output to signal, and terse notes
   here for what the code does not make obvious.
2. **Engineer the codebase and environment** — a strict gate, local/CI parity,
   and `just bootstrap` working from a clean clone.

## Stack and composition

- **Product shape:** library
- **Language(s):** rust
- **References composed:** base.md, project-graph.md, shapes/library.md,
  languages/rust.md, ci.md, llmlint.md, releasing.md
  (`compose_repo_plan.py --shape library --language rust --releasing`)
- **Projects in the graph:** `github-graphql-node-count` (the published crate and
  its fast tier); `github-graphql-node-count-e2e` (a `publish = false` member with
  no `src/`, taking the crate as an ordinary dependency and driving only its
  public surface); `install-smoke` (the crates.io-reaching install-path suite, its
  only edge to the published crate, out of the gate's target list); `tooling`
  (drives the shell scripts under `scripts/` and `install/` as subprocesses against
  real temporary directories, a real `git`, and a real local HTTP server); and
  `release-contract` (tagged `scope:contract`,
  depending on nothing here so a library change cannot reach it).
  Every project carries exactly one `scope:` tag, and `SCOPE_POLICY` in
  `release-contract`'s suite says what each scope may depend on — Nx's own
  module-boundary rule is an ESLint rule and there is no JavaScript here, so the
  boundary is enforced by reading the definitions and manifests that draw the real
  edges. A new project declares its scope there or fails.
- **Excluded, and why:**
  - *OS matrix* — pure computation over a `&str`: no I/O, no platform-conditional
    code. One Linux runner proves it; a matrix would triple CI for identical
    arithmetic.
  - *Live GitHub oracle tier* — `rateLimit(dryRun: true) { nodeCount }` would
    corroborate the arithmetic, but it needs a credential and a share of an
    account-wide rate limit, and it would have to hold its own copy of every
    fixture to stay off the library's affected path. The suite anchors on
    GitHub's published worked examples instead. Revisit if a fixture's expected
    total is ever disputed.
  - *Benchmark tier* — one pass over a parsed document; no hot path to defend.
  - *Release archives* — the crate ships no binary, so there is nothing to build
    per platform.
  - *`bash` as a composed language* — the shell scripts are build tooling, not a
    deliverable, so the language references stay `rust` alone (as the sibling
    repositories on this account also compose). One consequence: `robust_shell`
    is not a configured rule here, so the two directives inherited from the
    skill's script templates name only the rules this composition does have.

## Command surface

Use the `just` recipes; `just --list` is the inventory. Two rules it does not
show:

- **`just check` is offline and credential-free.** Keep it that way. Anything
  needing a network or a token is its own recipe and its own CI job —
  `deps-check`, `install-smoke`, `msrv`, and the `lint-llm*` tier.
- **`just toolchain` is the one place the pinned toolchain is provisioned**, so a
  CI job never hand-rolls rustup and cannot compile against a different compiler
  than the gate.
- **`just gate`** is the pre-push bar: `check` plus the diff-scoped llmlint tier.

## Coverage

**The floor is 100% line coverage of the library's own source**
(`crates/github-graphql-node-count/src/`), enforced by the repo-level `coverage`
target over the union of every project's run. Test sources are excluded from the
measurement; the number is about the library, not the suite.

100 rather than the 95 default because this library is pure computation with no
I/O, no platform branches and no error path a test cannot reach: every line is
coverable, so a floor tolerating uncovered lines would be declaring the suite
incomplete.

Every project's `test` target writes raw profiles into one shared directory and
declares it as an Nx output, so a cache replay restores them and the aggregate
never reports on a partial set. `just check` clears that directory first: a stale
profile can only make the number look better, which is the one direction a floor
must not be wrong in.

## Gate tiers and their measured budget

`just check` is the **affected tier**; `just check all` is the **broader tier**.
One command, one CI job, so the tier is a flag rather than a second gate.

Nothing is promoted out of the affected tier for speed, and nothing is split for
it: both tiers finish far inside the budget a contributor will wait through. What
*is* outside the gate — `deps-check`, `install-smoke`, `msrv`, the `lint-llm*`
tier — left because of what it touches, not how long it takes. Promote only from a
fresh measurement; the numbers behind this decision, and how to retake them, are
in [docs/gate-tiers.md](docs/gate-tiers.md).

The `workspace` project, which owns the aggregate `coverage` target, is always in
the affected set — coverage is a property of the union rather than of any one
project. At these timings that costs seconds; it would not be free in a repo with
a slow suite.

## Commits, releases, and merging

- **Squash-merge only, via PR, with auto-merge.** The default branch is
  protected: merge commits and rebase-merging are off, so one PR is one squash
  commit whose subject is the PR title. Queue with
  `gh pr merge --auto --squash`. Merged head branches auto-delete. Linear history
  required; no force-pushes, no branch deletion. Admins may break glass.
- **Required checks: `check`, `deps`, `msrv`, `install`, `commitlint`,
  `llmlint`** — exactly the jobs `.github/workflows/ci.yml` defines to verify a
  change. An empty or partial required set is vacuously green and auto-merge
  lands straight through it, so renaming a job moves those contexts, the
  `setup_github_governance.py` invocation, and this list together.
- **Releases: release-plz, a release-PR gate.** Conventional Commits drive the
  version; release-plz opens a PR writing the version, the manifests and
  `CHANGELOG.md`, and merging it tags `vX.Y.Z` **with a PAT** — the default
  `GITHUB_TOKEN` would create a tag that triggers nothing and ship no crate. The
  tag fires `release.yml`, which runs `scripts/publish-crate.sh`: it refuses a tag
  that is not `vX.Y.Z` or that names a version the manifest does not, and skips a
  version already live. Nobody hand-edits a version, hand-tags, or hand-dispatches
  a publish.
- **Bump policy (pre-1.0):** `feat` → minor; `feat!` / `BREAKING CHANGE` → minor
  (a break before 1.0 is not a major); `fix` / `perf` / `refactor` / `build` →
  patch; `chore` / `docs` / `ci` / `test` / `style` → no release.
- **The broader tier runs at release-prep, and only there**, because this repo
  batches: the release PR ships a commit no merge job swept. The `check` job runs
  the full sweep on a `release-plz-*` head branch and the affected tier
  everywhere else — including on a push to `main`, where there is no merge base
  but there is a well-defined one-commit diff, so `scripts/merge-base.sh` scopes
  against the commit's first parent rather than falling open to a second sweep.
  The tag-triggered publish re-gates nothing.
- **What this repo publishes** is declared in `release-targets.toml`, and the
  `release-contract` project holds that document to the real release
  configuration in both directions.

## Invariants (non-negotiable)

- The gate is strict: no warnings-only mode. A diagnostic is an error, or a
  suppression carrying a reason at its site.
- The library reaches no network, reads no credential, and consults no schema.
  Anything that would need one belongs outside the crate.
- Its public surface is a contract with the repositories that call it, so
  `NODE_LIMIT`, `Variables` and `node_count` are not renamed, retyped or
  narrowed. Adding a public item is fine.
- `node_count` takes untrusted document text and returns an error rather than
  panicking.
- **Security is gate-level.** No secrets in the tree (they live in the platform
  store, named by `gh-secrets.json`); every grant least-privilege.

## Tests are context engineering

This is an agent-driven repo: the suite is the only QA loop. Nothing is mocked:
the library's whole input is a `&str` and a map, and the shell scripts are driven
as subprocesses against real directories, a real `git`, and a real local HTTP
server. The one thing stood in for is the *registry* on the far side of that
socket — the single third party these tests cannot run. Each project's nested
`AGENTS.md` carries the rules for its own fixtures.

## Keeping the allowlist current

The allowlist lives in `.claude/settings.json` and the tool enforces it. Keep it
current and narrow: when a command becomes routine, add it rather than
re-approving it every session.

Three absences are deliberate. `git add` is not allowlisted — an allowlist cannot
bound it to the paths a change actually touches, and staging the whole tree is how
a stray file lands in a commit. `git commit` is allowlisted only in its
message-carrying forms, so `--amend` and the other history-rewriting spellings are
approved case by case. Neither is `just nx <anything>`: the escape hatch exists,
but an arbitrary orchestrator invocation is not routine.

## Suppressions

Fix a finding in the code it names, or suppress it at that site with a stated
reason. Never widen a suppression's scope to clear one site.
`.github/workflows/notignored.yml` surfaces every suppression a PR adds, so what
you leave is what a reviewer reads.
