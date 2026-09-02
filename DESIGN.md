# DESIGN.md

Decisions the user made, kept because the code shows *what* and never the
constraint that drove it. Only what a future reader needs, and only what came
from the user.

## This calculation is its own crate, not a private helper

The user asked for the node-count calculation to be extracted rather than kept
inside the caller that needed it, and gave the reason: it can be *"written once
with comprehensive tests and then almost never touched"*. GitHub's rules are
published, stable, and the same for every consumer.

Two things follow, and neither is negotiable on a maintainer's judgment alone:

- **The test bar is the deliverable, not an afterthought.** Somebody else should
  be able to trust this crate without reading it. That is why the suite anchors
  on GitHub's own worked examples rather than on numbers we computed, and why the
  coverage floor is 100 rather than the 95 the tooling defaults to.
- **The public surface is frozen by agreement, not by preference.**
  `NODE_LIMIT`, `Variables` and `node_count(&str, &Variables) -> Result<u64,
  NodeCountError>` were specified before this repository existed, because
  `onetaskgraph`'s GitHub Projects source is written against exactly them. A
  consumer's build is what reconciles the two sides, so changing one of the three
  breaks a repository this one cannot see. `NodeCountError` is
  `#[non_exhaustive]` for the same reason: adding a variant must not be a
  breaking change.

## It stays offline, credential-free and schema-free

The reason this crate can sit in a consumer's gate at all is that it needs
nothing to run — no token, no network, no GraphQL schema — so it works in a fork
pull request. That is why a field carrying `first:`/`last:` *is* the definition of
a connection here, and why the resulting blind spot (a connection supplying
neither, which GitHub rejects anyway) is documented rather than engineered away.

## Why it exists at all

The GitHub Projects source in `onetaskgraph` was shipping queries GitHub's own
published node limit lets it reject, and nobody noticed because nobody could
compute the number. A change meant to *reduce* what that source read went the
wrong way by a factor of ten and was argued rather than measured. The rate
limiting is shared across everything the account does, so the cost landed on
plan-board reads, settlement projections, and a manager reading the state of its
own work, all at once.
