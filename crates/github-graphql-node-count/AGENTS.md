# The library crate

The published deliverable. Its public surface is a contract two repositories hold
to — `onetaskgraph`'s GitHub Projects source calls exactly this — so
`NODE_LIMIT`, `Variables` and `node_count(&str, &Variables) -> Result<u64,
NodeCountError>` are not renamed, retyped or narrowed. Adding a public item is
fine; `NodeCountError` is `#[non_exhaustive]`, so adding a variant is too.

Keep the crate offline: no network, no credential, no schema, and no dependency
that would need one. `graphql-parser` is the only dependency and it should stay
the only one.

The rules this implements are GitHub's, and `src/count.rs` links the page that
publishes them. Check a change against that page rather than against the tests
alone — but if the two disagree, say so rather than quietly moving a fixture.
