# AGENTS.md

Durable instructions for humans and agents working in this repo. Write for a
future maintainer, not as a session log. Put deterministic steps in scripts and
keep this file for constraints, tradeoffs, and judgment.

> `CLAUDE.md` is a symlink to this file, so the two never drift. Edit
> `AGENTS.md` only.

## What this repo is

One published Rust library, `github-graphql-node-count`. It answers the
worst-case **node count** GitHub attributes to a GraphQL document, from the
document's text and its page-size variable bindings alone — no network, no
credential, no schema. Its consumers are other repositories' query gates.

`nodeCount` is the maximum number of nodes **one query may return**, limited per
query. It is not `cost`, the rate-limit **points** a call spends, metered per
hour across everything a credential does. This repo computes the first and says
nothing about the second; keep the two apart by name everywhere.

## Two standing goals on every task

The user drives product features and their request is the priority — but carry
two goals into *every* task. When either is the lowest-error path to what the
user asked, fold it into the same task without asking first; surface the rest as
follow-ups.

1. **Engineer the context for next time.** Realistic end-to-end tests that
   exercise what a consumer sees — especially when a bug the existing tests
   missed is reported — scripts that automate repetitive steps and shrink their
   output to signal, and terse `AGENTS.md` notes capturing what the code doesn't
   make obvious.
2. **Engineer the codebase and environment.** Keep the tree clean, the gate
   strict, and setup automated (`just bootstrap` from a clean clone), so a
   feature ships with a low error rate.

## Stack and composition

How this repo was built up from the create-repo reference pieces.

- **Product shape:** library
- **Language(s):** rust
- **References composed:** base.md, project-graph.md, shapes/library.md,
  languages/rust.md, ci.md, llmlint.md, releasing.md
  (`compose_repo_plan.py --shape library --language rust --releasing`)
- **Projects in the graph:**
  - `github-graphql-node-count` — the published crate: its source and its own
    fast `tests/` tier.
  - `github-graphql-node-count-e2e` — the consumer-shaped suite. A
    `publish = false` member with no `src/`, taking the crate as an ordinary
    dependency and driving only its public surface. Depends on the crate, so a
    change to the crate reaches it.
  - `release-contract` — reconciles `release-targets.toml` against what the
    release configuration actually publishes. Tagged `type:contract` and
    deliberately depends on **nothing** in this repo, so a change to the library
    cannot reach it and it cannot reach back.
- **Excluded, and why:**
  - *OS matrix* — the crate is pure computation over `&str`: no I/O, no
    platform-conditional code, no binary. One Linux runner proves it; a matrix
    would triple CI for identical arithmetic.
  - *End-user install-path CI job* — the install path is `cargo add`, which is
    dependency resolution rather than an installer to smoke-test. The
    `github-graphql-node-count-e2e` project already consumes the crate the way a
    dependent does, and `cargo publish --dry-run` in the publish workflow proves
    the packaged shape.
  - *Live GitHub oracle tier* — GitHub's `rateLimit(dryRun: true) { nodeCount }`
    would corroborate the arithmetic, but it needs a credential and a share of an
    account-wide rate limit, and it would have to hold its own copy of every
    fixture to stay off the library's affected path. The arithmetic is GitHub's
    published, stable rule set and the suite anchors on GitHub's own worked
    examples, so the corroboration does not pay for that second fixture source.
    Revisit if a fixture's expected total is ever disputed.
  - *MSRV* — no minimum is promised, so there is no `rust-version` to gate. The
    toolchain is pinned in `rust-toolchain.toml`.
  - *Benchmark tier* — the computation is one pass over a parsed document; there
    is no hot path to defend.
  - *`bash` as a composed language* — the shell scripts are build tooling, not a
    deliverable, so the language references stay `rust` alone (the shape the
    sibling repositories on this account also compose). The consequence is that
    `robust_shell` is not a configured rule here, so the two directives inherited
    from the skill's own script templates name only the rules this composition
    does have.

## Command surface

Use the `just` recipes; do not hand-roll equivalent commands. The repo-wide verbs
delegate to Nx, which fans a uniformly-named target across projects; what a
target *does* stays with the project that declares it. `just --list` is the
inventory.

`just check` is the deterministic gate: format, lint, doc, tests, and the
coverage floor. It is offline and credential-free — keep it that way. `just gate`
adds the diff-scoped llmlint tier, which needs a harness.

## Coverage

**The floor is 100% line coverage of the library's own source**
(`crates/github-graphql-node-count/src/`), enforced by the repo-level `coverage`
target over the union of every project's test run. Test sources are excluded from
the measurement; the number is about the library, not about the suite.

100 rather than the 95 default because this library is pure computation with no
I/O, no platform branches, and no error path a test cannot reach: every line is
coverable, so a floor that tolerated uncovered lines would be declaring the suite
incomplete.

Two consequences worth knowing before you change the graph. The `test` target is
**not** Nx-cached: every project's run writes into one shared profile directory,
so a replayed run would leave the aggregate report measuring a partial set — a
cache that changed the answer rather than the speed. And the `workspace` project,
which owns the aggregate, is always in the affected set, because coverage is a
property of the union rather than of any one project. Both cost seconds here;
neither is a pattern to copy into a repository with a slow suite without first
solving the profile-directory sharing.

## Commits, releases, and merging

- **Squash-merge only, via PR, with auto-merge.** The default branch is
  protected; merge commits and rebase-merging are off, so one PR is one squash
  commit whose subject is the PR title. Queue with
  `gh pr merge --auto --squash`. Merged head branches auto-delete. Admins may
  break glass.
- **Required checks:** `check`, `deps`, `commitlint`, `llmlint`. Those are
  exactly the jobs `.github/workflows/ci.yml` defines to verify a change, and the
  set is not empty: an empty required set is vacuously green, and auto-merge
  lands through it. Rename a job and the contexts, the
  `setup_github_governance.py` invocation, and this list move together.
- **Releases: release-plz, a release-PR gate.** Conventional Commits drive the
  version; release-plz opens a PR that writes the version, the manifests and
  `CHANGELOG.md`, and merging it tags `vX.Y.Z` with a PAT. The tag triggers
  `release.yml`, which publishes to crates.io. Nobody hand-edits a version,
  hand-tags, or hand-dispatches a publish.
- **Bump policy (pre-1.0):** `feat` → minor; `feat!` / `BREAKING CHANGE` → minor
  (a break before 1.0 is not a major); `fix` / `perf` / `refactor` / `build` →
  patch; `chore` / `docs` / `ci` / `test` / `style` → no release.
- **The broader tier runs at release-prep**, because this repo batches: the
  release PR ships a commit no merge job swept. `check` runs the full sweep on a
  `release-plz-*` head branch and the affected tier everywhere else — one job,
  one context, so the tier is a flag on the gate rather than a second gate. The
  tag-triggered publish re-gates nothing.
- **What this repo publishes** is declared in `release-targets.toml`, and the
  `release-contract` project holds that document to the real release
  configuration in both directions.

## Invariants (non-negotiable)

- The gate is strict: no warnings-only mode. A diagnostic is an error or a
  suppression carrying a documented reason at its site.
- **Tests are realistic and complete.** The e2e project drives the published
  surface as a dependent does, with no access to private items. A behaviour is
  not done until a real journey covers it.
- The library reaches no network, reads no credential, and consults no schema.
  Anything that would need one belongs outside the crate.
- Validate external input at the public surface: `node_count` takes untrusted
  document text and returns an error rather than panicking.
- **Security is gate-level.** No secrets in the tree (they live in the platform
  store, named by `gh-secrets.json`); every grant least-privilege.

## Scripts and output are context

Quiet on success — a line or nothing. On failure, the exact error and a concrete
next action. Command output is context the next agent must read.

## Tests are context engineering

This is an agent-driven repo: the suite is the only QA loop.

- Nothing is mocked, because there is nothing to mock: the library's whole input
  is a `&str` and a map.
- **The fixture is the document.** Every expected total is a named constant
  beside the query text it belongs to, so a different document reaching the same
  number does not pass.
- GitHub's two worked examples are transcribed as they stand, from the
  documentation page named in `src/lib.rs`. Do not "simplify" a fixture to reach
  its number.
- Every documented error is driven to an error — not a panic, a zero, or a wrong
  count.

## Keeping the allowlist current

The agent command allowlist lives in `.claude/settings.json` and the tool
enforces it. Keep it current and narrow: when a command becomes part of the
routine workflow, add it rather than re-approving it every session.

## Suppressions

A finding is fixed in the code it names, or suppressed at that site with a stated
reason for why the rule does not apply there. Never widen a suppression's scope
to clear one site. `.github/workflows/notignored.yml` surfaces every suppression
a PR adds, so what you leave is what a reviewer reads.

## After the main task: refine and hand off

Act on the two standing goals: propose materially-helpful follow-ups (scripts,
`AGENTS.md` notes, tests, fixtures) and note each one's likely impact. Skip
busywork.
