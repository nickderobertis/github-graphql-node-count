# github-graphql-node-count

Compute the **node count** GitHub's GraphQL API charges a query against its
500,000-node limit — offline, from the query text, before you send it.

> Individual calls cannot request more than 500,000 total nodes.
> — [GitHub: Rate limits and node limits for the GraphQL API](https://docs.github.com/en/graphql/overview/rate-limits-and-node-limits-for-the-graphql-api)

Not to be confused with GitHub's *other* number: `rateLimit.cost` is **points**,
metered per hour across everything a credential does. This crate computes
`rateLimit.nodeCount` — the maximum number of **nodes one query may return**,
limited per query.

Status: scaffolding. Nothing is published yet.
