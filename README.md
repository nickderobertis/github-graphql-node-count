# github-graphql-node-count

Compute, offline, the two numbers GitHub's GraphQL API charges a query — the
worst-case **node count** it may return and the **rate-limit points** one call of
it spends — from the query text and its page-size variables, before you send it.

```toml
[dependencies]
github-graphql-node-count = "0"
```

```rust
use github_graphql_node_count::{node_count, point_cost, Variables, NODE_LIMIT};

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
let nodes = node_count(document, &variables).expect("a well-formed document counts");
assert_eq!(nodes, 550);
assert!(nodes < NODE_LIMIT);

// The other limit: `repositories` is resolved once and `issues` fifty times,
// so 51 requests round to one rate-limit point.
let points = point_cost(document, &variables).expect("a well-formed document prices");
assert_eq!(points, 1);
```

## What it is for

GitHub rejects a query whose node count reaches 500,000, and it charges every
call against an hourly budget one credential shares across everything it does. A
query that grew past either is usually one nobody measured — and the second is
worse, because it is observable only by asking GitHub, which a required check on
a fork pull request cannot do. This crate is the measurement, for both: put it in
a test, and a query that would be rejected, or one that quietly gave back a cost
reduction somebody made on purpose, fails your build instead of your production
call.

## Two numbers, two limits

GitHub meters two different numbers against two different limits. Both are
computed here, from one traversal of the document.

| | what it counts | what it is limited against | computed by |
| --- | --- | --- | --- |
| **`nodeCount`** | the maximum number of nodes **one query may return** | 500,000, **per query** | `node_count` |
| **`cost`** | the rate-limit **points** one call spends | an hourly budget, **per credential**, across everything that credential does | `point_cost` |

They are not two views of one quantity. A cheap query run in a loop exhausts the
hourly budget without ever approaching the node limit; a single enormous query is
rejected outright while costing a handful of points. So gating on one is not
gating on the other, and this crate never renames one into the other.

> Individual calls cannot request more than 500,000 total nodes.
> — [GitHub: Rate limits and node limits for the GraphQL API](https://docs.github.com/en/graphql/overview/rate-limits-and-node-limits-for-the-graphql-api)

The point rule from the same page: add up the requests needed to fulfil each
unique connection in the call, assuming every request reaches its page-size
limit; divide that aggregate by 100 and round to the nearest whole number; and
never answer below GitHub's minimum of 1. A connection is resolved once per
parent node, so what it costs is the product of the page sizes **strictly above**
it — its own page size changes how many nodes it returns and not how often it is
resolved. `point_aggregate` returns the raw pre-rounding aggregate, for a gate
that wants to see a change smaller than a whole point.

## No schema, and that is the design

Every function here is handed document text and nothing else — no network, no
credential, no GraphQL schema. So the crate cannot know which fields are
connections by type. Instead, **a field carrying a `first:` or a `last:` argument
is a connection**: it multiplies the count beneath it and it is one of the
connections whose requests are aggregated. Every other field contributes no
multiplier, no nodes and no requests.

That is what makes this crate runnable in a fork pull request with no secret,
which is the whole reason you can put it in a gate. The price is one blind spot,
stated plainly and applying to both answers: **a connection supplying neither
`first` nor `last` is invalid to GitHub and invisible here**, so it is treated as
an ordinary field and both answers are an undercount. A connection supplying
*both* is charged at the larger of the two, so both stay a worst case.

## What it answers with

`node_count`, `point_cost` and `point_aggregate` return an error rather than a
panic, a zero, or a wrong number when the text is not GraphQL, when the document
holds no operation or more than one, when a `first:`/`last:` names an unbound
variable or falls outside `1..=100`, and when a spread names a fragment the
document does not define. There is one error type across all three, because there
is one parse and one walk: a document either yields both numbers or fails
identically for either. Every message names the field or the `line:column` at
fault. See the [API
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
