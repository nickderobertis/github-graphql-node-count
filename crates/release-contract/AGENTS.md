# The release contract

Reconciles `release-targets.toml` against what the release configuration actually
publishes, in both directions, so a name this repository starts or stops
publishing fails here rather than going undeclared.

It depends on **nothing** in this workspace, deliberately: a change to the
library must not reach this suite, and this suite must not reach back into what
depends on it. Keep it that way — one convenient import would re-attach it to
every change in the repository, and nothing would fail to say so.
