# Gate tiers: the measurement behind the decision

The decision is that nothing is promoted out of the affected tier and nothing is
split for speed. This is the measurement that decision rests on, and how to retake
it — a dated observation from one host, which is why it is a document you open
when you are about to move a target rather than context every session carries.

## The two tiers

- **Affected tier** — `just check`. The projects a change's diff can reach, keyed
  off an explicitly derived merge base (`scripts/merge-base.sh`). What development
  and review run.
- **Broader tier** — `just check all`. One full sweep over every project. It runs
  at exactly one lifecycle point: **release-prep**, on the release-plz release PR,
  because this repository batches releases and that PR ships a commit no merge job
  swept.

Both are the same recipe and the same CI job, so the tier is a flag rather than a
second gate.

## What was measured, and when

Taken on the development host on 2026-09-02, on the tree at that date:

| Run | Wall clock | Nx cache |
| --- | --- | --- |
| `just check all`, cold cache | 16 s | 0/14 |
| `just check all`, warm cache | 4 s | 13/14 replayed |

The starting budget the create-repo guidance suggests is **10 minutes p95 for the
whole affected tier** and **5 minutes p95 for its lint and unit targets**. Both
measurements are two orders of magnitude inside it, so the promotion rule — move
the target with the largest expected cost per change out of the tier until the
tier is back under budget — never fires, and there is nothing here worth
splitting for wall-clock.

## What sits outside the gate, and why

Not for speed. Each of these leaves because of what it *touches*, which is
unconditional:

| Recipe | Why it is out |
| --- | --- |
| `just deps-check` | fetches an advisory database |
| `just install-smoke` | resolves dependencies from crates.io |
| `just msrv` | installs a second toolchain |
| `just lint-llm*` | drives a real LLM harness with a credential |

The gate itself stays offline and credential-free, which is what lets it run in a
fork pull request.

## Retaking the measurement

```bash
just nx reset                       # drop the Nx cache
time just check all                 # cold
time just check all                 # warm
```

Collect p50 and p95 over several runs rather than one, and record what you found
here with its date. A promotion made without a measurement is a guess; if the
numbers move far enough that the affected tier stops returning inside the budget,
split the expensive project or raise its cache hit rate before promoting
anything, and never promote the target that covers the change in front of you.
