# The release contract

Reconciles `release-targets.toml` against what the release configuration actually
publishes, in both directions, so a name this repository starts or stops
publishing fails here rather than going undeclared. It also holds the
`scope:contract` tag to its meaning — Nx's own module-boundary rule is an ESLint
rule and there is no JavaScript here, so the boundary is enforced by reading the
manifests that draw the real graph edges.

It depends on **nothing** in this workspace, deliberately: a change to the library
must not reach this suite, and this suite must not reach back into what depends on
it. Its own test is what keeps that true — one convenient dependency would
silently re-attach it to every change in the repository.
