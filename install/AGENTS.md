# The install-path suite

Proves the path a consumer takes — `cargo add` — rather than the path a
contributor takes. It packages the crate exactly as `cargo publish` would and
builds the README's own dependency declaration against that package, in a project
of its own, so a file missing from the package, a `readme`/`include` path that
does not survive packaging, a public item that only resolves inside this
workspace, or a README version requirement the crate has outgrown fails here
rather than in somebody else's build.

It resolves dependencies from crates.io, so it is **not** in `just check` and not
in the affected tier's target list: it is its own CI job (`install`), and its only
graph edge is to the published crate.
