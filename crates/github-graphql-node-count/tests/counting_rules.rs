//! The counting rules GitHub publishes, one fixture and one asserted total each.
//!
//! Every expected total is a named constant beside the document it belongs to,
//! so a different document reaching the same number does not pass. Both of the
//! crate's answers are held to their own rules here: the node count that a query
//! may return, and the rate-limit points one call of it spends.

use github_graphql_node_count::{node_count, point_aggregate, point_cost, Variables};

/// No page-size variables: the document binds every page size as a literal.
fn no_variables() -> Variables {
    Variables::new()
}

/// One page-size variable, bound to `value`.
fn page(value: u32) -> Variables {
    Variables::from([("page".to_string(), value)])
}

/// A connection nested inside a connection: the inner page size is charged once
/// per outer node, not once overall.
const NESTED_PATH: &str = r#"
query {
  viewer {
    repositories(first: 10) {
      edges {
        node {
          issues(first: 5) {
            edges { node { title } }
          }
        }
      }
    }
  }
}
"#;

/// 10 repositories + 10 x 5 = 50 repository issues.
const NESTED_PATH_NODES: u64 = 60;

#[test]
fn a_nested_connection_multiplies_by_its_parents_page_size() {
    assert_eq!(
        node_count(NESTED_PATH, &no_variables()),
        Ok(NESTED_PATH_NODES)
    );
}

/// Three connections under one parent. An implementation that took the maximum
/// across siblings instead of the sum would answer 10 here.
const SIBLING_PATHS: &str = r#"
query {
  viewer {
    repositories(first: 10) { edges { node { name } } }
    followers(first: 7) { edges { node { login } } }
    following(first: 3) { edges { node { login } } }
  }
}
"#;

/// 10 repositories + 7 followers + 3 following.
const SIBLING_PATHS_NODES: u64 = 20;

#[test]
fn sibling_connections_sum_rather_than_compete() {
    assert_eq!(
        node_count(SIBLING_PATHS, &no_variables()),
        Ok(SIBLING_PATHS_NODES)
    );
    // The distinguishing assertion: the maximum across the siblings is 10.
    assert_ne!(node_count(SIBLING_PATHS, &no_variables()), Ok(10));
}

/// The connection that costs the most sits inside a fragment, so a counter that
/// ignored fragment definitions would answer 10.
const SPREAD_FRAGMENT: &str = r#"
query {
  viewer {
    repositories(first: 10) {
      edges { node { ...RepositoryIssues } }
    }
  }
}

fragment RepositoryIssues on Repository {
  name
  issues(first: 4) {
    edges { node { title } }
  }
}
"#;

/// 10 repositories + 10 x 4 = 40 repository issues.
const SPREAD_FRAGMENT_NODES: u64 = 50;

#[test]
fn a_spread_counts_the_fragment_it_names() {
    assert_eq!(
        node_count(SPREAD_FRAGMENT, &no_variables()),
        Ok(SPREAD_FRAGMENT_NODES)
    );
}

/// `timelineItems.nodes` is a union, so the fields under it are reached through
/// inline fragments — including one nested inside another. `LabeledEvent` costs
/// nothing, which is what makes the total attributable to the other branch.
const INLINE_FRAGMENT_UNION: &str = r#"
query {
  repository(owner: "nickderobertis", name: "onetaskgraph") {
    issue(number: 235) {
      timelineItems(first: 10) {
        nodes {
          ... on CrossReferencedEvent {
            source {
              ... on PullRequest {
                comments(first: 5) { nodes { bodyHTML } }
              }
            }
          }
          ... on LabeledEvent {
            label { name }
          }
        }
      }
    }
  }
}
"#;

/// 10 timeline items + 10 x 5 = 50 pull-request comments.
const INLINE_FRAGMENT_UNION_NODES: u64 = 60;

#[test]
fn an_inline_fragment_on_a_union_counts_against_its_parent() {
    assert_eq!(
        node_count(INLINE_FRAGMENT_UNION, &no_variables()),
        Ok(INLINE_FRAGMENT_UNION_NODES)
    );
}

/// `RepositoryIssues` is reachable only through `ViewerRepositories`, so a
/// counter that resolved one level of spread would answer 6.
const NESTED_SPREADS: &str = r#"
query {
  viewer { ...ViewerRepositories }
}

fragment ViewerRepositories on User {
  repositories(first: 6) {
    edges { node { ...RepositoryIssues } }
  }
}

fragment RepositoryIssues on Repository {
  issues(first: 2) {
    edges { node { title } }
  }
}
"#;

/// 6 repositories + 6 x 2 = 12 repository issues.
const NESTED_SPREADS_NODES: u64 = 18;

#[test]
fn a_spread_reached_through_another_spread_is_counted() {
    assert_eq!(
        node_count(NESTED_SPREADS, &no_variables()),
        Ok(NESTED_SPREADS_NODES)
    );
}

/// The shape that matters most to a consumer: one page-size constant reused down
/// a nested path, so binding it to `n` moves the total by `n` squared.
const VARIABLE_SPENT_TWICE: &str = r#"
query ViewerIssues($page: Int!) {
  viewer {
    repositories(first: $page) {
      edges {
        node {
          issues(first: $page) {
            edges { node { title } }
          }
        }
      }
    }
  }
}
"#;

/// 3 repositories + 3 x 3 = 9 repository issues.
const VARIABLE_SPENT_TWICE_AT_3: u64 = 12;
/// 5 repositories + 5 x 5 = 25 repository issues.
const VARIABLE_SPENT_TWICE_AT_5: u64 = 30;
/// 9 repositories + 9 x 9 = 81 repository issues.
const VARIABLE_SPENT_TWICE_AT_9: u64 = 90;

#[test]
fn one_variable_spent_twice_moves_the_total_by_its_square() {
    assert_eq!(
        node_count(VARIABLE_SPENT_TWICE, &page(3)),
        Ok(VARIABLE_SPENT_TWICE_AT_3)
    );
    assert_eq!(
        node_count(VARIABLE_SPENT_TWICE, &page(5)),
        Ok(VARIABLE_SPENT_TWICE_AT_5)
    );
    assert_eq!(
        node_count(VARIABLE_SPENT_TWICE, &page(9)),
        Ok(VARIABLE_SPENT_TWICE_AT_9)
    );

    // n squared, not 2n: tripling the page size from 3 to 9 multiplies the
    // repository-issue term by nine.
    for (n, total) in [
        (3u64, VARIABLE_SPENT_TWICE_AT_3),
        (5, VARIABLE_SPENT_TWICE_AT_5),
        (9, VARIABLE_SPENT_TWICE_AT_9),
    ] {
        assert_eq!(total, n + n * n);
    }
}

/// The four spellings of one page size. They differ only in the outer argument,
/// so anything but an equal count is the crate honouring one form over another.
const OUTER_FIRST_LITERAL: &str = r#"
query {
  viewer {
    repositories(first: 25) {
      edges { node { issues(first: 4) { edges { node { title } } } } }
    }
  }
}
"#;

const OUTER_LAST_LITERAL: &str = r#"
query {
  viewer {
    repositories(last: 25) {
      edges { node { issues(first: 4) { edges { node { title } } } } }
    }
  }
}
"#;

const OUTER_FIRST_VARIABLE: &str = r#"
query ViewerIssues($page: Int!) {
  viewer {
    repositories(first: $page) {
      edges { node { issues(first: 4) { edges { node { title } } } } }
    }
  }
}
"#;

const OUTER_LAST_VARIABLE: &str = r#"
query ViewerIssues($page: Int!) {
  viewer {
    repositories(last: $page) {
      edges { node { issues(first: 4) { edges { node { title } } } } }
    }
  }
}
"#;

/// 25 repositories + 25 x 4 = 100 repository issues.
const PAGE_ARGUMENT_NODES: u64 = 125;

#[test]
fn first_and_last_and_literal_and_variable_all_count_the_same() {
    assert_eq!(
        node_count(OUTER_FIRST_LITERAL, &no_variables()),
        Ok(PAGE_ARGUMENT_NODES)
    );
    assert_eq!(
        node_count(OUTER_LAST_LITERAL, &no_variables()),
        Ok(PAGE_ARGUMENT_NODES)
    );
    assert_eq!(
        node_count(OUTER_FIRST_VARIABLE, &page(25)),
        Ok(PAGE_ARGUMENT_NODES)
    );
    assert_eq!(
        node_count(OUTER_LAST_VARIABLE, &page(25)),
        Ok(PAGE_ARGUMENT_NODES)
    );
}

/// GitHub rejects a connection supplying both; the crate takes the larger so the
/// answer it does give stays a worst case.
const BOTH_FIRST_AND_LAST: &str = r#"
query {
  viewer {
    repositories(first: 3, last: 30) {
      edges { node { name } }
    }
  }
}
"#;

/// The larger of the two page sizes, 30.
const BOTH_FIRST_AND_LAST_NODES: u64 = 30;

/// The same invalid connection with something nested under it, so the page size
/// the crate chose is visible in what the child costs rather than only in the
/// parent's own nodes.
const BOTH_FIRST_AND_LAST_NESTED: &str = r#"
query {
  viewer {
    repositories(first: 3, last: 30) {
      edges { node { issues(first: 5) { edges { node { title } } } } }
    }
  }
}
"#;

/// 30 repositories + 30 x 5 = 150 repository issues.
const BOTH_FIRST_AND_LAST_NESTED_NODES: u64 = 180;
/// 1 repositories request + 30 issues requests, one per repository — the larger
/// page size, so the price stays a worst case too.
const BOTH_FIRST_AND_LAST_NESTED_AGGREGATE: u64 = 31;

#[test]
fn a_field_supplying_both_first_and_last_takes_the_larger() {
    assert_eq!(
        node_count(BOTH_FIRST_AND_LAST, &no_variables()),
        Ok(BOTH_FIRST_AND_LAST_NODES)
    );
}

/// Working from text alone, this crate can only see a connection that says
/// `first`/`last`. A field with no page size adds no nodes and no multiplier —
/// the documented blind spot, asserted so it stays deliberate.
const NO_PAGE_SIZE_ANYWHERE: &str = r#"
query {
  viewer {
    login
    repository(owner: "nickderobertis", name: "onevcs") {
      name
      issues { edges { node { title } } }
    }
  }
}
"#;

/// Nothing carries `first`/`last`, so nothing is charged.
const NO_PAGE_SIZE_ANYWHERE_NODES: u64 = 0;

#[test]
fn a_document_with_no_page_size_costs_nothing_here() {
    assert_eq!(
        node_count(NO_PAGE_SIZE_ANYWHERE, &no_variables()),
        Ok(NO_PAGE_SIZE_ANYWHERE_NODES)
    );
}

/// A shorthand operation: a bare selection set with no `query` keyword.
const SHORTHAND_OPERATION: &str = r#"
{
  viewer {
    repositories(first: 3) { edges { node { name } } }
  }
}
"#;

/// 3 repositories.
const SHORTHAND_OPERATION_NODES: u64 = 3;

#[test]
fn a_shorthand_operation_is_counted() {
    assert_eq!(
        node_count(SHORTHAND_OPERATION, &no_variables()),
        Ok(SHORTHAND_OPERATION_NODES)
    );
}

/// A mutation whose payload reads a connection. `$body` is declared but no
/// `first:`/`last:` references it, so it need not be bound.
const MUTATION_OPERATION: &str = r#"
mutation AddComment($body: String!) {
  addComment(input: { subjectId: "MDU6SXNzdWUx", body: $body }) {
    commentEdge {
      node {
        reactions(first: 5) { nodes { content } }
      }
    }
  }
}
"#;

/// 5 reactions on the created comment.
const MUTATION_OPERATION_NODES: u64 = 5;

#[test]
fn a_mutation_is_counted_and_an_unreferenced_variable_need_not_be_bound() {
    assert_eq!(
        node_count(MUTATION_OPERATION, &no_variables()),
        Ok(MUTATION_OPERATION_NODES)
    );
}

/// GitHub's schema has no subscription root today, but the GraphQL grammar
/// admits one — so the crate answers rather than panicking if a caller hands it
/// one.
const SUBSCRIPTION_OPERATION: &str = r#"
subscription WatchIssues {
  issueEvents {
    issue {
      comments(first: 8) { nodes { bodyHTML } }
    }
  }
}
"#;

/// 8 comments.
const SUBSCRIPTION_OPERATION_NODES: u64 = 8;

#[test]
fn a_subscription_is_counted() {
    assert_eq!(
        node_count(SUBSCRIPTION_OPERATION, &no_variables()),
        Ok(SUBSCRIPTION_OPERATION_NODES)
    );
}

/// A fragment the document defines but never spreads is dead weight GitHub would
/// reject; it is not counted here either.
const UNUSED_FRAGMENT: &str = r#"
query {
  viewer {
    repositories(first: 4) { edges { node { name } } }
  }
}

fragment Unused on Repository {
  issues(first: 100) { edges { node { title } } }
}
"#;

/// 4 repositories; the unspread fragment's 100 issues are not charged.
const UNUSED_FRAGMENT_NODES: u64 = 4;

#[test]
fn a_fragment_that_is_never_spread_is_not_counted() {
    assert_eq!(
        node_count(UNUSED_FRAGMENT, &no_variables()),
        Ok(UNUSED_FRAGMENT_NODES)
    );
}

// GitHub's point rule, fixture by fixture. A connection is *resolved* once per
// parent node, so what it contributes to the aggregate is the product of the page
// sizes strictly above it — its own page size never appears in that product. The
// call then costs `max(1, round(aggregate / 100))`.

/// Three levels of connection. `repositories` is resolved once, `issues` once per
/// repository, and `comments` once per issue — so the aggregate multiplies down
/// the path exactly as the node count does, one factor short.
const POINT_NESTED_PATH: &str = r#"
query {
  viewer {
    repositories(first: 100) {
      edges {
        node {
          issues(first: 100) {
            edges {
              node {
                comments(first: 10) { nodes { bodyHTML } }
              }
            }
          }
        }
      }
    }
  }
}
"#;

/// 1 repositories request + 100 issues requests + 100 x 100 = 10,000 comments
/// requests.
const POINT_NESTED_PATH_AGGREGATE: u64 = 10_101;
/// 10,101 / 100 = 101.01, which rounds to 101.
const POINT_NESTED_PATH_POINTS: u64 = 101;
/// 100 repositories + 100 x 100 = 10,000 issues + 100 x 100 x 10 = 100,000
/// comments. Stated here because it is the number the aggregate is *not*.
const POINT_NESTED_PATH_NODES: u64 = 110_100;

#[test]
fn requests_multiply_down_a_nested_path() {
    assert_eq!(
        point_aggregate(POINT_NESTED_PATH, &no_variables()),
        Ok(POINT_NESTED_PATH_AGGREGATE)
    );
    assert_eq!(
        point_cost(POINT_NESTED_PATH, &no_variables()),
        Ok(POINT_NESTED_PATH_POINTS)
    );
    // The two answers are different numbers about the same document, and the
    // node count is the larger by the deepest connection's own page size.
    assert_eq!(
        node_count(POINT_NESTED_PATH, &no_variables()),
        Ok(POINT_NESTED_PATH_NODES)
    );
}

/// Two connections under one parent, each resolved once per parent node. A
/// counter that took the maximum across siblings would aggregate 101 here and
/// answer one point.
const POINT_SIBLING_PATHS: &str = r#"
query {
  viewer {
    repositories(first: 100) {
      edges {
        node {
          issues(first: 20) { edges { node { title } } }
          pullRequests(first: 30) { edges { node { title } } }
        }
      }
    }
  }
}
"#;

/// 1 repositories request + 100 issues requests + 100 pullRequests requests.
const POINT_SIBLING_PATHS_AGGREGATE: u64 = 201;
/// 201 / 100 = 2.01, which rounds to 2.
const POINT_SIBLING_PATHS_POINTS: u64 = 2;
/// What a counter taking the maximum across the two siblings would aggregate.
const POINT_MAXIMUM_ACROSS_SIBLINGS: u64 = 101;

#[test]
fn requests_sum_across_sibling_paths() {
    assert_eq!(
        point_aggregate(POINT_SIBLING_PATHS, &no_variables()),
        Ok(POINT_SIBLING_PATHS_AGGREGATE)
    );
    assert_ne!(
        point_aggregate(POINT_SIBLING_PATHS, &no_variables()),
        Ok(POINT_MAXIMUM_ACROSS_SIBLINGS),
        "sibling connections must sum, not compete"
    );
    assert_eq!(
        point_cost(POINT_SIBLING_PATHS, &no_variables()),
        Ok(POINT_SIBLING_PATHS_POINTS)
    );
}

// The property that makes the two answers different, and the one a reader is
// most likely to get wrong: a connection's *own* page size changes how many
// nodes it returns and not how many times it is resolved. These two documents
// differ in one digit — the innermost page size — and nowhere else.

/// The innermost connection asks for a single item.
const LEAF_PAGE_OF_ONE: &str = r#"
query {
  viewer {
    repositories(first: 100) {
      edges {
        node {
          issues(first: 100) {
            edges {
              node {
                comments(first: 1) { nodes { bodyHTML } }
              }
            }
          }
        }
      }
    }
  }
}
"#;

/// The same document with the innermost connection asking for a hundred.
const LEAF_PAGE_OF_A_HUNDRED: &str = r#"
query {
  viewer {
    repositories(first: 100) {
      edges {
        node {
          issues(first: 100) {
            edges {
              node {
                comments(first: 100) { nodes { bodyHTML } }
              }
            }
          }
        }
      }
    }
  }
}
"#;

/// 1 + 100 + 10,000, whichever page the innermost connection asks for: it is
/// resolved 10,000 times either way.
const LEAF_PAGE_AGGREGATE: u64 = 10_101;
/// 10,101 / 100 = 101.01, which rounds to 101 — for both documents.
const LEAF_PAGE_POINTS: u64 = 101;
/// 100 + 10,000 + 10,000 x 1 nodes.
const LEAF_PAGE_OF_ONE_NODES: u64 = 20_100;
/// 100 + 10,000 + 10,000 x 100 nodes: a hundred times the leaf's contribution.
const LEAF_PAGE_OF_A_HUNDRED_NODES: u64 = 1_010_100;

#[test]
fn a_connections_own_page_size_does_not_change_what_it_costs() {
    for document in [LEAF_PAGE_OF_ONE, LEAF_PAGE_OF_A_HUNDRED] {
        assert_eq!(
            point_aggregate(document, &no_variables()),
            Ok(LEAF_PAGE_AGGREGATE)
        );
        assert_eq!(point_cost(document, &no_variables()), Ok(LEAF_PAGE_POINTS));
    }

    // The same two documents answer very different node counts, so the equality
    // above is the point rule holding rather than the documents being the same.
    assert_eq!(
        node_count(LEAF_PAGE_OF_ONE, &no_variables()),
        Ok(LEAF_PAGE_OF_ONE_NODES)
    );
    assert_eq!(
        node_count(LEAF_PAGE_OF_A_HUNDRED, &no_variables()),
        Ok(LEAF_PAGE_OF_A_HUNDRED_NODES)
    );
    assert_ne!(LEAF_PAGE_OF_ONE_NODES, LEAF_PAGE_OF_A_HUNDRED_NODES);
}

/// Two root connections, each resolved once and each carrying one sub-connection
/// resolved 74 times: an aggregate of exactly 150, the tie the rounding has to
/// settle.
const AGGREGATE_OF_EXACTLY_150: &str = r#"
query {
  viewer {
    repositories(first: 74) {
      edges { node { issues(first: 5) { nodes { title } } } }
    }
    followers(first: 74) {
      edges { node { gists(first: 5) { nodes { name } } } }
    }
  }
}
"#;

/// The same document with one page size a single item smaller.
const AGGREGATE_OF_149: &str = r#"
query {
  viewer {
    repositories(first: 74) {
      edges { node { issues(first: 5) { nodes { title } } } }
    }
    followers(first: 73) {
      edges { node { gists(first: 5) { nodes { name } } } }
    }
  }
}
"#;

/// (1 + 74) + (1 + 74).
const EXACTLY_150_AGGREGATE: u64 = 150;
/// 1.5 exactly; the tie goes away from zero, so 2 rather than 1.
const EXACTLY_150_POINTS: u64 = 2;
/// (1 + 74) + (1 + 73).
const JUST_UNDER_AGGREGATE: u64 = 149;
/// 1.49, which rounds down to 1.
const JUST_UNDER_POINTS: u64 = 1;

#[test]
fn the_aggregate_rounds_to_the_nearest_point_with_ties_away_from_zero() {
    assert_eq!(
        point_aggregate(AGGREGATE_OF_EXACTLY_150, &no_variables()),
        Ok(EXACTLY_150_AGGREGATE)
    );
    assert_eq!(
        point_cost(AGGREGATE_OF_EXACTLY_150, &no_variables()),
        Ok(EXACTLY_150_POINTS)
    );

    assert_eq!(
        point_aggregate(AGGREGATE_OF_149, &no_variables()),
        Ok(JUST_UNDER_AGGREGATE)
    );
    assert_eq!(
        point_cost(AGGREGATE_OF_149, &no_variables()),
        Ok(JUST_UNDER_POINTS)
    );
}

#[test]
fn a_document_with_no_connection_still_costs_githubs_minimum_of_one() {
    // Nothing in this document carries `first`/`last`, so it aggregates no
    // requests at all — and GitHub's published minimum is one point, not zero.
    assert_eq!(
        point_aggregate(NO_PAGE_SIZE_ANYWHERE, &no_variables()),
        Ok(0)
    );
    assert_eq!(point_cost(NO_PAGE_SIZE_ANYWHERE, &no_variables()), Ok(1));
}

#[test]
fn a_single_connection_resolved_once_costs_one() {
    // One root connection: one request, so 0.01 points before the minimum
    // applies, and one point after it.
    assert_eq!(point_aggregate(SHORTHAND_OPERATION, &no_variables()), Ok(1));
    assert_eq!(point_cost(SHORTHAND_OPERATION, &no_variables()), Ok(1));

    // Its page size is the largest GitHub allows and it still costs one, because
    // a connection's own page size is not what it is charged for.
    let wide = r#"
{
  viewer {
    repositories(first: 100) { edges { node { name } } }
  }
}
"#;
    assert_eq!(point_aggregate(wide, &no_variables()), Ok(1));
    assert_eq!(point_cost(wide, &no_variables()), Ok(1));
}

#[test]
fn a_spread_fragments_connections_are_aggregated_at_the_spreads_multiplier() {
    // The fragment's `issues` connection is reached under 10 repositories, so it
    // is resolved 10 times: 1 + 10 requests, the same walk the node count takes.
    assert_eq!(point_aggregate(SPREAD_FRAGMENT, &no_variables()), Ok(11));
    assert_eq!(point_cost(SPREAD_FRAGMENT, &no_variables()), Ok(1));

    // Reached through another spread, two levels down, it is still aggregated at
    // the multiplier of the path that reaches it: 1 + 6.
    assert_eq!(point_aggregate(NESTED_SPREADS, &no_variables()), Ok(7));

    // An unspread fragment's connection is never resolved, so it is never
    // charged: 1 request for `repositories` and nothing for the dead weight.
    assert_eq!(point_aggregate(UNUSED_FRAGMENT, &no_variables()), Ok(1));
}

#[test]
fn a_page_size_variable_moves_the_aggregate_the_way_it_moves_the_multiplier() {
    // `repositories(first: $page)` is resolved once whatever `$page` is bound to;
    // the `issues` beneath it is resolved once per repository. So the aggregate
    // is 1 + n, growing linearly where the node count grows as n squared.
    for (size, aggregate, nodes) in [
        (3u32, 4u64, VARIABLE_SPENT_TWICE_AT_3),
        (5, 6, VARIABLE_SPENT_TWICE_AT_5),
        (9, 10, VARIABLE_SPENT_TWICE_AT_9),
    ] {
        let variables = page(size);
        assert_eq!(
            point_aggregate(VARIABLE_SPENT_TWICE, &variables),
            Ok(aggregate)
        );
        assert_eq!(node_count(VARIABLE_SPENT_TWICE, &variables), Ok(nodes));
        // Every one of these is under a point, so all three cost the minimum.
        assert_eq!(point_cost(VARIABLE_SPENT_TWICE, &variables), Ok(1));
    }
}

#[test]
fn an_inline_fragments_connections_are_aggregated_against_its_parent() {
    // An inline fragment adds no level of its own, so the `comments` reached
    // through two of them is resolved once per timeline item: 1 + 10 requests.
    assert_eq!(
        point_aggregate(INLINE_FRAGMENT_UNION, &no_variables()),
        Ok(11)
    );
    assert_eq!(point_cost(INLINE_FRAGMENT_UNION, &no_variables()), Ok(1));
}

#[test]
fn a_mutation_and_a_subscription_are_priced_like_a_query() {
    // The payload of a mutation reads one connection, resolved once.
    assert_eq!(point_aggregate(MUTATION_OPERATION, &no_variables()), Ok(1));
    assert_eq!(point_cost(MUTATION_OPERATION, &no_variables()), Ok(1));

    // And a subscription, which GitHub's schema has no root for today but the
    // grammar admits, is answered rather than refused.
    assert_eq!(
        point_aggregate(SUBSCRIPTION_OPERATION, &no_variables()),
        Ok(1)
    );
    assert_eq!(point_cost(SUBSCRIPTION_OPERATION, &no_variables()), Ok(1));
}

/// 1 repositories request + 25 issues requests, one per repository.
const PAGE_ARGUMENT_AGGREGATE: u64 = 26;

#[test]
fn last_and_first_and_literal_and_variable_all_cost_the_same() {
    // The same four spellings of one page size the node count is held to. A
    // crate reading `first:` but not `last:` would see no connection at all in
    // the `last:` documents and aggregate 1 rather than 26.
    for (document, variables) in [
        (OUTER_FIRST_LITERAL, no_variables()),
        (OUTER_LAST_LITERAL, no_variables()),
        (OUTER_FIRST_VARIABLE, page(25)),
        (OUTER_LAST_VARIABLE, page(25)),
    ] {
        assert_eq!(
            point_aggregate(document, &variables),
            Ok(PAGE_ARGUMENT_AGGREGATE)
        );
        assert_eq!(point_cost(document, &variables), Ok(1));
    }
}

#[test]
fn a_field_supplying_both_first_and_last_is_priced_at_the_larger() {
    // Resolved once at the root, so the aggregate is 1 whichever page size is
    // read — the assertion that tells the two apart is the nested one below.
    assert_eq!(point_aggregate(BOTH_FIRST_AND_LAST, &no_variables()), Ok(1));
    assert_eq!(point_cost(BOTH_FIRST_AND_LAST, &no_variables()), Ok(1));

    // With a child, the chosen page size becomes the child's multiplier: 30, the
    // larger, so the price stays a worst case. Reading `first:` would give 4.
    assert_eq!(
        point_aggregate(BOTH_FIRST_AND_LAST_NESTED, &no_variables()),
        Ok(BOTH_FIRST_AND_LAST_NESTED_AGGREGATE)
    );
    assert_ne!(
        point_aggregate(BOTH_FIRST_AND_LAST_NESTED, &no_variables()),
        Ok(4)
    );
    assert_eq!(
        node_count(BOTH_FIRST_AND_LAST_NESTED, &no_variables()),
        Ok(BOTH_FIRST_AND_LAST_NESTED_NODES)
    );
}
