# The tooling suite

Drives this repository's shell scripts as subprocesses against real temporary
directories, a real `git`, and a real HTTP server — no stubbed `git`, no mocked
`curl`, and no reading a script and asserting on its text. The only thing local
rather than real is the *registry* on the far side of the socket, which is the one
third party these tests cannot run; `tests/support/tiny_http.rs` is it.

What each script owes, and why it is covered here rather than by inspection:

- `merge-base.sh` decides **what the gate runs**. A bug in it does not fail
  loudly; it silently narrows the scope of every check. Its fail-closed paths must
  print nothing, so the caller falls back to the full sweep.
- `gate-tier.sh` decides **which tier CI runs**. Getting it wrong under-gates the
  one commit that most needs the full sweep — the release PR's.
- `publish-crate.sh` decides **whether a version reaches crates.io**. A publish is
  not reversible, so every refusal it makes is driven for real.
- `release-probe.sh` decides **what a waiting consumer is told**. Reporting
  `absent` when the registry was merely unreachable would tell that consumer
  nothing shipped when something did.
- `nx.sh` is **the gate's own output**: one line on success, the preserved log on
  failure, and Nx's stdout untouched for the callers that parse it.
- `install/smoke.sh` is covered here only for its refusals; its happy path is what
  the `install` CI job runs for real on every change.
