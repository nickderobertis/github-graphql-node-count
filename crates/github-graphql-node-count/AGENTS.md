# The library crate

Its public surface is a contract two repositories hold to, so `NODE_LIMIT`,
`Variables` and `node_count(&str, &Variables) -> Result<u64, NodeCountError>` are
not renamed, retyped or narrowed. Adding a public item is fine; `NodeCountError`
is `#[non_exhaustive]`, so adding a variant is too. `DESIGN.md` records who
decided that and why.

Keep the crate offline: no network, no credential, no schema, and no dependency
that would need one. `graphql-parser` is the only dependency and should stay so.

The rules this implements are GitHub's, and `src/count.rs` links the page that
publishes them. Check a change against that page rather than against the tests
alone — and if the two disagree, say so rather than quietly moving a fixture.

## Fixtures in `tests/`

- **The fixture is the document.** Every expected total is a named constant beside
  the query text it belongs to, so a different document reaching the same number
  does not pass. Do not simplify a fixture toward its number.
- **Every documented error is driven to an error** — not a panic, a zero, or a
  wrong count — and its `Display` is asserted to name the field or position at
  fault, because that message is what a consumer shows its own user.
- The overflow fixtures are generated rather than transcribed: they exist to push
  the arithmetic past `u64`, not to reproduce a published rule, and an
  eleven-level query written out by hand would be less legible.
