# The release contract

Reconciles `release-targets.toml` against what the release configuration actually
publishes, in both directions, so a name this repository starts or stops
publishing fails here rather than going undeclared.

It also holds the project graph to `SCOPE_POLICY` — the module-boundary rule in
the form a Cargo workspace can enforce, since Nx's own is an ESLint rule and there
is no JavaScript here. A new project declares its scope in that table, and what
that scope may depend on, or the suite fails.

That check lives here, in `tests/scope_policy.rs`, but **runs as
`workspace:test`**, not as this project's own `test` (which runs only
`release_targets`). An edge is drawn in a `project.json` or member `Cargo.toml`
that some *other* project owns, so with the check on this project neither the
affected set nor the cache key moved with it: a pull request could add a
forbidden edge and replay a stale pass until the release PR's cold sweep. The
root `workspace` project lists every project in `implicitDependencies` — which
`scope:repo` is allowed and the suite requires — so it is affected by any
project's change, and its `test` target is keyed on every `project.json`, every
`Cargo.toml` and this crate's own files. The alternative, giving this project
those edges, would break the rule below.

It depends on **nothing** in this workspace, deliberately: a change to the library
must not reach this suite, and this suite must not reach back into what depends on
it. Its own tests are what keep that true.

`tests/release_workflow.rs` holds `.github/workflows/release-plz.yml` to arming
auto-merge on the release pull request behind its required checks: one step,
after `release-plz release-pr` in the same job, with the release PAT, `--auto` on
every `gh pr merge` and `--admin` on none. It also runs that step's own script
under `bash`. `gh` is the one thing doubled, and the double needs the real `jq` on
`PATH`. The double is checked in under `tests/gh-double/` instead of written by
the test, because nextest runs each test in its own process, and one process
executing a file another is still writing fails with "Text file busy".
