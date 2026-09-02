# The tooling suite

Drives this repository's shell scripts as subprocesses against real temporary git
repositories — no stubbed `git`, no reading the script and asserting on its text.

`scripts/merge-base.sh` decides **what the gate runs**, so a bug in it does not
fail loudly; it silently narrows the scope of every check. That is why its
fail-closed paths are tested as carefully as its happy path: each one must print
nothing, so the caller falls back to the full sweep.
