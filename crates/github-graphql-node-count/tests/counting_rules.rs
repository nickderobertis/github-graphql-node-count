//! The counting rules GitHub publishes, one fixture and one asserted total each.
//!
//! Every expected total is a named constant beside the document it belongs to,
//! so a different document reaching the same number does not pass.

use github_graphql_node_count::{node_count, Variables};

/// No page-size variables: the document binds every page size as a literal.
fn no_variables() -> Variables {
    Variables::new()
}

/// One page-size variable, bound to `value`.
fn page(value: u32) -> Variables {
    Variables::from([("page".to_string(), value)])
}

// --- multiplication down a nested path ------------------------------------

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

// --- summation across sibling paths ---------------------------------------

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

// --- a named fragment definition reached by a spread ----------------------

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

// --- an inline fragment on a union ----------------------------------------

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

// --- a spread reached through another spread ------------------------------

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

// --- one variable spent twice down a single nested path -------------------

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

// --- `first` and `last`, literal and variable -----------------------------

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

#[test]
fn a_field_supplying_both_first_and_last_takes_the_larger() {
    assert_eq!(
        node_count(BOTH_FIRST_AND_LAST, &no_variables()),
        Ok(BOTH_FIRST_AND_LAST_NODES)
    );
}

// --- fields that are not connections --------------------------------------

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

// --- the operation shapes the parser admits -------------------------------

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
