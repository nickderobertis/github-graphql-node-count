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
  `onetaskgraph`'s GitHub Projects source is written against exactly them.
  `point_cost` was agreed the same way and later, with the same signature so a
  consumer reaches for it identically. A consumer's build is what reconciles the
  two sides, so changing one of them breaks a repository this one cannot see.
  `NodeCountError` is `#[non_exhaustive]` for the same reason: adding a variant
  must not be a breaking change.

## Points are computed here, and out of the same traversal

The user asked for a second answer beside the node count: what one call of a
document spends against GitHub's hourly rate limit. Two constraints came with the
request, and neither is a maintainer's to trade away.

- **One traversal, not two.** The number of requests a connection needs is the
  number of times it is resolved, which is the product of the page sizes strictly
  above it — the `multiplier` this crate's walk already holds on the line that
  computes the node count. So the two answers are one descent accumulating a
  pair, differing on exactly one line. The reason to put this arithmetic in this
  crate at all is that the walk already exists here; a second parser and a second
  walk beside the first would be the copy that drifts, and would give the change
  no reason to be here rather than in the consumer.
- **One error type, one variable map.** `point_cost` fails on exactly the inputs
  `node_count` fails on and means the same things by each failure, because there
  is one parse behind both. A second error type would make a consumer hold two
  vocabularies for one document.

The rule was verified rather than inferred, and the user did the verifying:
eleven documents were predicted by hand from their text and then priced by GitHub
with `rateLimit(dryRun: true)`, and the prediction matched all eleven. GitHub's
own published worked example — 5,101 requests scoring 51 points — is what the
suite anchors on, and its document text and both figures are written into the
fixture rather than fetched, so a later edit to that page cannot move them under
us.

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

The point answer arrived from the second half of that same story. The user was
told that repository's GitHub suite was eating about 30% of the account's hourly
GraphQL budget, found that very high for what it does, and two changes to the
Projects source cut it substantially. Nothing anywhere could *check* that they
had, or that a later change did not quietly give it back: node count was computed
offline and gated on, while points — the number that actually runs out — were
observable only by asking GitHub, which a required check on a fork pull request
cannot do. That is the whole difference this crate's second answer makes: a cost
reduction that was measured once, versus one that stays reduced.
