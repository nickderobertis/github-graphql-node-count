# github-graphql-node-count

Compute the worst-case **node count** GitHub's GraphQL API attributes to a query
— offline, from the query text and its page-size variables, before you send it.

```toml
[dependencies]
github-graphql-node-count = "0"
```

```rust
use github_graphql_node_count::{node_count, Variables, NODE_LIMIT};

let document = r#"
    query($repos: Int!) {
      viewer {
        repositories(first: $repos) {
          edges { node { name issues(first: 10) { edges { node { title } } } } }
        }
      }
    }
"#;
let variables = Variables::from([("repos".to_string(), 50)]);

// 50 repositories + 50 x 10 issues.
assert_eq!(node_count(document, &variables)?, 550);
assert!(node_count(document, &variables)? < NODE_LIMIT);
# Ok::<(), github_graphql_node_count::NodeCountError>(())
```

## What it is for

GitHub rejects a query whose node count reaches 500,000, and a query that grew
past the limit is usually one nobody measured. This crate is the measurement:
put it in a test, and a query that would be rejected fails your build instead of
your production call.

## `nodeCount`, not `cost`

GitHub meters two different numbers against two different limits.

| | what it counts | what it is limited against |
| --- | --- | --- |
| **`nodeCount`** | the maximum number of nodes **one query may return** | 500,000, **per query** |
| `cost` | the rate-limit **points** a call spends | an hourly budget, **per credential** |

This crate computes the first. It says nothing about the second.

> Individual calls cannot request more than 500,000 total nodes.
> — [GitHub: Rate limits and node limits for the GraphQL API](https://docs.github.com/en/graphql/overview/rate-limits-and-node-limits-for-the-graphql-api)

## No schema, and that is the design

`node_count` is handed document text and nothing else — no network, no
credential, no GraphQL schema. So it cannot know which fields are connections by
type. Instead, **a field carrying a `first:` or a `last:` argument is a
connection** and multiplies the count beneath it; every other field contributes
no multiplier.

That is what makes this crate runnable in a fork pull request with no secret,
which is the whole reason you can put it in a gate. The cost is one blind spot,
stated plainly: **a connection supplying neither `first` nor `last` is invalid to
GitHub and invisible here**, so it is counted as an ordinary field and the answer
is an undercount.

## What it answers with

`node_count` returns an error rather than a panic, a zero, or a wrong count when
the text is not GraphQL, when the document holds no operation or more than one,
when a `first:`/`last:` names an unbound variable or falls outside `1..=100`, and
when a spread names a fragment the document does not define. Every message names
the field or the `line:column` at fault. See the [API
documentation](https://docs.rs/github-graphql-node-count) for the full list.

## Versioning

Semver, pre-1.0: a breaking change bumps the minor version. `NodeCountError` is
`#[non_exhaustive]`, so a new variant is not a breaking change — match it with a
`_ =>` arm.

Releases and their notes are in [CHANGELOG.md](CHANGELOG.md), written from
Conventional Commits with no manual step.

## Contributing

`just bootstrap` from a clean clone, then `just check` — the whole gate, offline
and credential-free. [AGENTS.md](AGENTS.md) is the durable instruction layer.

## License

MIT
