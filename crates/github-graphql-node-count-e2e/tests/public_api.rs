//! The crate as a dependent meets it.
//!
//! This suite lives in a project of its own that takes
//! `github-graphql-node-count` as an ordinary dependency, so it can reach only
//! the published surface — `node_count`, `point_cost`, `point_aggregate`,
//! `NODE_LIMIT`, `Variables` and `NodeCountError`, and no private item. It drives
//! GitHub's own worked examples for both answers, the consumer shape
//! `onetaskgraph-github-projects` assembles its queries in, and inputs that must
//! come back as errors.

use github_graphql_node_count::{
    node_count, point_aggregate, point_cost, NodeCountError, PageSizeArgument, Variables,
    NODE_LIMIT,
};

/// No page-size variables: every page size in the document is a literal.
fn no_variables() -> Variables {
    Variables::new()
}

/// GitHub's "simple" worked example, transcribed as it is published at
/// <https://docs.github.com/en/graphql/overview/rate-limits-and-node-limits-for-the-graphql-api>.
/// The fixture is the document, so a different query reaching the same total does
/// not pass.
const GITHUB_SIMPLE_EXAMPLE: &str = r#"
query {
  viewer {
    repositories(first: 50) {
      edges {
        repository:node {
          name
          issues(first: 10) {
            totalCount
            edges {
              node {
                title
                bodyHTML
              }
            }
          }
        }
      }
    }
  }
}
"#;

/// GitHub's arithmetic, quoted: 50 = 50 repositories + 50 x 10 = 500 repository
/// issues = 550 total nodes.
const GITHUB_SIMPLE_EXAMPLE_NODES: u64 = 550;

#[test]
fn githubs_simple_worked_example_counts_to_githubs_own_total() {
    assert_eq!(
        node_count(GITHUB_SIMPLE_EXAMPLE, &no_variables()),
        Ok(GITHUB_SIMPLE_EXAMPLE_NODES)
    );
}

/// GitHub's "complex" worked example. It carries what the simple one does not:
/// two sibling connections under one parent whose costs are summed, and a
/// three-level nested path. A counter taking the maximum across siblings instead
/// of the sum answers 21,050 here and 550 on the simple example, so this is the
/// document that tells the two apart.
const GITHUB_COMPLEX_EXAMPLE: &str = r#"
query {
  viewer {
    repositories(first: 50) {
      edges {
        repository:node {
          name
          pullRequests(first: 20) {
            edges {
              pullRequest:node {
                title
                comments(first: 10) {
                  edges {
                    comment:node {
                      bodyHTML
                    }
                  }
                }
              }
            }
          }
          issues(first: 20) {
            totalCount
            edges {
              issue:node {
                title
                bodyHTML
                comments(first: 10) {
                  edges {
                    comment:node {
                      bodyHTML
                    }
                  }
                }
              }
            }
          }
        }
      }
    }
    followers(first: 10) {
      edges {
        follower:node {
          login
        }
      }
    }
  }
}
"#;

/// GitHub's arithmetic, quoted: 50 = 50 repositories + 50 x 20 = 1,000
/// pullRequests + 50 x 20 x 10 = 10,000 pullRequest comments + 50 x 20 = 1,000
/// issues + 50 x 20 x 10 = 10,000 issue comments + 10 = 10 followers = 22,060
/// total nodes.
const GITHUB_COMPLEX_EXAMPLE_NODES: u64 = 22_060;

/// What a counter that took the maximum across sibling paths would answer.
const MAXIMUM_ACROSS_SIBLINGS: u64 = 21_050;

#[test]
fn githubs_complex_worked_example_counts_to_githubs_own_total() {
    assert_eq!(
        node_count(GITHUB_COMPLEX_EXAMPLE, &no_variables()),
        Ok(GITHUB_COMPLEX_EXAMPLE_NODES)
    );
    assert_ne!(
        node_count(GITHUB_COMPLEX_EXAMPLE, &no_variables()),
        Ok(MAXIMUM_ACROSS_SIBLINGS),
        "sibling connections must sum, not compete"
    );
}

#[test]
fn both_worked_examples_sit_under_the_published_limit() {
    assert_eq!(NODE_LIMIT, 500_000);
    for (document, total) in [
        (GITHUB_SIMPLE_EXAMPLE, GITHUB_SIMPLE_EXAMPLE_NODES),
        (GITHUB_COMPLEX_EXAMPLE, GITHUB_COMPLEX_EXAMPLE_NODES),
    ] {
        let counted = node_count(document, &no_variables()).expect("a published example counts");
        assert_eq!(counted, total);
        assert!(
            counted < NODE_LIMIT,
            "{counted} should be under {NODE_LIMIT}"
        );
    }
}

/// GitHub's worked example for the *point* rule — the one it prices rather than
/// counts — transcribed as it is published at
/// <https://docs.github.com/en/graphql/overview/rate-limits-and-node-limits-for-the-graphql-api>.
///
/// The fixture is the document: a different query reaching 51 does not pass.
const GITHUB_POINT_EXAMPLE: &str = r#"
query {
  viewer {
    login
    repositories(first: 100) {
      edges {
        node {
          id
          issues(first: 50) {
            edges {
              node {
                id
                labels(first: 60) {
                  edges {
                    node {
                      id
                      name
                    }
                  }
                }
              }
            }
          }
        }
      }
    }
  }
}
"#;

/// GitHub's own figure, quoted: this query "requires 5,101 requests to fulfill".
///
/// It is also the rule applied to the text above, which is what makes the example
/// an independent check rather than a restatement of our own arithmetic:
/// `repositories` is resolved once, `issues` a hundred times, and `labels`
/// 100 x 50 = 5,000 times.
const GITHUB_POINT_EXAMPLE_AGGREGATE: u64 = 5_101;

/// GitHub's own final score for that query: 5,101 / 100 = 51.01, which rounds to
/// **51**.
const GITHUB_POINT_EXAMPLE_POINTS: u64 = 51;

/// The other answer about the same document, stated so the two are visibly
/// different numbers: 100 repositories + 100 x 50 = 5,000 issues
/// + 100 x 50 x 60 = 300,000 labels.
const GITHUB_POINT_EXAMPLE_NODES: u64 = 305_100;

#[test]
fn githubs_worked_point_example_prices_to_githubs_own_score() {
    assert_eq!(
        point_aggregate(GITHUB_POINT_EXAMPLE, &no_variables()),
        Ok(GITHUB_POINT_EXAMPLE_AGGREGATE)
    );
    assert_eq!(
        point_cost(GITHUB_POINT_EXAMPLE, &no_variables()),
        Ok(GITHUB_POINT_EXAMPLE_POINTS)
    );

    assert_eq!(
        node_count(GITHUB_POINT_EXAMPLE, &no_variables()),
        Ok(GITHUB_POINT_EXAMPLE_NODES)
    );
}

#[test]
fn a_call_costs_at_least_githubs_published_minimum_of_one_point() {
    // GitHub: "The minimum point value of a call to the GraphQL API is 1."
    // Nothing here carries `first`/`last`, so it aggregates no requests at all.
    let no_connection = "query { viewer { login } }";
    assert_eq!(point_aggregate(no_connection, &no_variables()), Ok(0));
    assert_eq!(point_cost(no_connection, &no_variables()), Ok(1));

    // And the largest page GitHub allows, resolved once, is still one request.
    let one_connection = "query { viewer { repositories(first: 100) { nodes { name } } } }";
    assert_eq!(point_aggregate(one_connection, &no_variables()), Ok(1));
    assert_eq!(point_cost(one_connection, &no_variables()), Ok(1));
}

// The consumer shape: one shared fragment concatenated onto each of several
// operations, giving several documents that each hold exactly one operation —
// which is how `onetaskgraph-github-projects` builds its query constants.

/// The fragment every Projects query concatenates. Its `fieldValues` connection
/// is the cost a document picks up by including it.
const PROJECT_ITEM_FIELDS: &str = r#"
fragment ProjectItemFields on ProjectV2Item {
  id
  type
  fieldValues(first: 20) {
    nodes {
      ... on ProjectV2ItemFieldTextValue { text }
      ... on ProjectV2ItemFieldSingleSelectValue { name }
    }
  }
  content {
    ... on Issue { number title }
    ... on PullRequest { number title }
  }
}
"#;

/// One page of a project's items.
const ITEMS_PAGE_OPERATION: &str = r#"
query ProjectItemsPage($items: Int!) {
  viewer {
    projectV2(number: 1) {
      items(first: $items) {
        pageInfo { hasNextPage endCursor }
        nodes { ...ProjectItemFields }
      }
    }
  }
}
"#;

/// The board: the project's views beside its items — two sibling connections.
const BOARD_OPERATION: &str = r#"
query ProjectBoard($items: Int!, $views: Int!) {
  viewer {
    projectV2(number: 1) {
      views(first: $views) { nodes { name } }
      items(first: $items) { nodes { ...ProjectItemFields } }
    }
  }
}
"#;

/// A sweep across repositories, so the fragment is reached three levels down.
const SWEEP_OPERATION: &str = r#"
query ProjectItemsAcrossRepositories($repos: Int!, $projects: Int!, $items: Int!) {
  viewer {
    repositories(first: $repos) {
      nodes {
        projectsV2(first: $projects) {
          nodes {
            items(first: $items) { nodes { ...ProjectItemFields } }
          }
        }
      }
    }
  }
}
"#;

/// How the consumer assembles a document: one operation, then the shared
/// fragment, concatenated into one string that holds exactly one operation.
fn assemble(operation: &str) -> String {
    format!("{operation}\n{PROJECT_ITEM_FIELDS}")
}

/// 50 items + 50 x 20 = 1,000 field values.
const ITEMS_PAGE_NODES: u64 = 1_050;
/// 10 views + 50 items + 50 x 20 = 1,000 field values.
const BOARD_NODES: u64 = 1_060;
/// 5 repositories + 5 x 2 = 10 projects + 10 x 10 = 100 items
/// + 100 x 20 = 2,000 field values.
const SWEEP_NODES: u64 = 2_115;

/// 1 items request + 50 fieldValues requests, one per item.
const ITEMS_PAGE_AGGREGATE: u64 = 51;
/// 1 views request + 1 items request + 50 fieldValues requests.
const BOARD_AGGREGATE: u64 = 52;
/// 1 repositories + 5 projectsV2 + 10 items + 100 fieldValues requests.
const SWEEP_AGGREGATE: u64 = 116;

#[test]
fn each_assembled_consumer_document_counts_to_its_own_total() {
    let paging = Variables::from([("items".to_string(), 50)]);
    assert_eq!(
        node_count(&assemble(ITEMS_PAGE_OPERATION), &paging),
        Ok(ITEMS_PAGE_NODES)
    );

    let board = Variables::from([("items".to_string(), 50), ("views".to_string(), 10)]);
    assert_eq!(
        node_count(&assemble(BOARD_OPERATION), &board),
        Ok(BOARD_NODES)
    );

    let sweep = Variables::from([
        ("repos".to_string(), 5),
        ("projects".to_string(), 2),
        ("items".to_string(), 10),
    ]);
    assert_eq!(
        node_count(&assemble(SWEEP_OPERATION), &sweep),
        Ok(SWEEP_NODES)
    );

    // Three documents, three different answers: the shared fragment is counted
    // into each of them at that document's own multiplier.
    let totals = [ITEMS_PAGE_NODES, BOARD_NODES, SWEEP_NODES];
    for (i, left) in totals.iter().enumerate() {
        for right in &totals[i + 1..] {
            assert_ne!(left, right, "each assembled document has its own total");
        }
    }
}

#[test]
fn each_assembled_consumer_document_is_priced_at_its_own_aggregate() {
    let paging = Variables::from([("items".to_string(), 50)]);
    assert_eq!(
        point_aggregate(&assemble(ITEMS_PAGE_OPERATION), &paging),
        Ok(ITEMS_PAGE_AGGREGATE)
    );

    let board = Variables::from([("items".to_string(), 50), ("views".to_string(), 10)]);
    assert_eq!(
        point_aggregate(&assemble(BOARD_OPERATION), &board),
        Ok(BOARD_AGGREGATE)
    );

    let sweep = Variables::from([
        ("repos".to_string(), 5),
        ("projects".to_string(), 2),
        ("items".to_string(), 10),
    ]);
    assert_eq!(
        point_aggregate(&assemble(SWEEP_OPERATION), &sweep),
        Ok(SWEEP_AGGREGATE)
    );

    // All three are under a point's worth of requests, so all three cost
    // GitHub's minimum — while their node counts differ by a thousand. That is
    // the two limits being genuinely different measurements of one document, and
    // why a consumer gating on one is not gating on the other.
    for (document, variables) in [
        (assemble(ITEMS_PAGE_OPERATION), &paging),
        (assemble(BOARD_OPERATION), &board),
        (assemble(SWEEP_OPERATION), &sweep),
    ] {
        assert_eq!(point_cost(&document, variables), Ok(1));
    }
    assert_ne!(ITEMS_PAGE_NODES, SWEEP_NODES);
}

#[test]
fn a_consumers_page_size_moves_what_one_call_spends_on_the_hourly_budget() {
    // The gate the consuming repository actually writes: bind the page sizes it
    // plans to send and pin the points, so a change that gives back a saving is
    // a failing test rather than an hour of budget nobody notices.
    let modest = Variables::from([
        ("repos".to_string(), 25),
        ("projects".to_string(), 2),
        ("items".to_string(), 25),
    ]);
    // 1 repositories + 25 projectsV2 + 50 items + 1,250 fieldValues = 1,326
    // requests, which is 13.26 points.
    assert_eq!(
        point_aggregate(&assemble(SWEEP_OPERATION), &modest),
        Ok(1_326)
    );
    assert_eq!(point_cost(&assemble(SWEEP_OPERATION), &modest), Ok(13));

    let greedy = Variables::from([
        ("repos".to_string(), 100),
        ("projects".to_string(), 10),
        ("items".to_string(), 100),
    ]);
    // 1 + 100 + 1,000 + 100,000 = 101,101 requests, which is 1,011.01 points.
    assert_eq!(
        point_aggregate(&assemble(SWEEP_OPERATION), &greedy),
        Ok(101_101)
    );
    assert_eq!(point_cost(&assemble(SWEEP_OPERATION), &greedy), Ok(1_011));
}

#[test]
fn an_operation_missing_its_fragment_is_an_error_rather_than_a_cheaper_count() {
    // The same operation *without* the fragment concatenated: proof that the
    // fragment's cost is counted rather than skipped.
    let paging = Variables::from([("items".to_string(), 50)]);
    let error = node_count(ITEMS_PAGE_OPERATION, &paging).expect_err("the spread is unresolved");
    assert!(
        matches!(&error, NodeCountError::UndefinedFragment { name, .. }
                 if name == "ProjectItemFields"),
        "{error:?}"
    );
    assert!(
        error.to_string().contains("...ProjectItemFields"),
        "{error}"
    );
}

#[test]
fn a_consumers_page_size_can_push_a_document_over_the_published_limit() {
    // The gate a consumer actually writes: bind the page sizes it plans to send
    // and compare against NODE_LIMIT before sending anything.
    let modest = Variables::from([
        ("repos".to_string(), 25),
        ("projects".to_string(), 2),
        ("items".to_string(), 25),
    ]);
    let modest_count = node_count(&assemble(SWEEP_OPERATION), &modest).expect("counts");
    assert!(modest_count < NODE_LIMIT, "{modest_count}");

    let greedy = Variables::from([
        ("repos".to_string(), 100),
        ("projects".to_string(), 10),
        ("items".to_string(), 100),
    ]);
    let greedy_count = node_count(&assemble(SWEEP_OPERATION), &greedy).expect("counts");
    assert!(
        greedy_count > NODE_LIMIT,
        "{greedy_count} should exceed {NODE_LIMIT}"
    );
}

#[test]
fn a_page_size_github_would_reject_comes_back_as_an_error() {
    let over = Variables::from([("items".to_string(), 500)]);
    let error = node_count(&assemble(ITEMS_PAGE_OPERATION), &over).expect_err("out of range");
    assert!(
        matches!(
            error,
            NodeCountError::PageSizeOutOfRange {
                value: 500,
                argument: PageSizeArgument::First,
                ..
            }
        ),
        "{error:?}"
    );

    // Nothing was bound for `items` at all.
    let error =
        node_count(&assemble(ITEMS_PAGE_OPERATION), &no_variables()).expect_err("unbound variable");
    assert!(
        matches!(&error, NodeCountError::UnboundVariable { variable, .. } if variable == "items"),
        "{error:?}"
    );
}

#[test]
fn concatenating_two_operations_into_one_document_is_refused_by_contract() {
    // The consumer builds several single-operation documents, not one document
    // holding several: `node_count` takes no operation name, so it refuses.
    let both = format!("{ITEMS_PAGE_OPERATION}\n{BOARD_OPERATION}\n{PROJECT_ITEM_FIELDS}");
    let variables = Variables::from([("items".to_string(), 50), ("views".to_string(), 10)]);
    let error = node_count(&both, &variables).expect_err("two operations");
    assert!(
        matches!(&error, NodeCountError::MultipleOperations { names }
                 if names == &["ProjectItemsPage".to_string(), "ProjectBoard".to_string()]),
        "{error:?}"
    );
}

/// The items the consumer's build is written against, asserted at compile time
/// from outside the crate.
///
/// This is the contract two repositories hold to: `onetaskgraph`'s GitHub
/// Projects source restates exactly these names and types. Renaming, retyping or
/// narrowing any of them stops this file compiling, which is the point — the
/// break surfaces here rather than in a repository this one cannot see.
///
/// `point_cost` joins the list on the same terms: it was agreed as
/// `fn(&str, &Variables) -> Result<u64, NodeCountError>` — `node_count`'s
/// signature, so a consumer reaches for it the same way — over the same
/// `Variables` and the same `NodeCountError`, and pinning that here is what stops
/// a second error type or a second variable map appearing later.
mod frozen_surface {
    use super::*;

    /// `node_count(&str, &Variables) -> Result<u64, NodeCountError>`.
    const _NODE_COUNT: fn(&str, &Variables) -> Result<u64, NodeCountError> = node_count;

    /// `point_cost(&str, &Variables) -> Result<u64, NodeCountError>`: the same
    /// shape, the same map, the same error type.
    const _POINT_COST: fn(&str, &Variables) -> Result<u64, NodeCountError> = point_cost;

    /// `point_aggregate(&str, &Variables) -> Result<u64, NodeCountError>`.
    const _POINT_AGGREGATE: fn(&str, &Variables) -> Result<u64, NodeCountError> = point_aggregate;

    /// `NODE_LIMIT: u64`.
    const _NODE_LIMIT: u64 = NODE_LIMIT;

    /// `Variables = BTreeMap<String, u32>`, keyed without the leading `$`.
    const _VARIABLES: fn(std::collections::BTreeMap<String, u32>) -> Variables = |map| map;
}

#[test]
fn the_promised_surface_behaves_as_a_consumer_declares_it() {
    // The map really is a `BTreeMap<String, u32>` a consumer can build itself.
    let mut variables: std::collections::BTreeMap<String, u32> = Default::default();
    variables.insert("repos".to_string(), 50);
    let variables: Variables = variables;

    let document = r#"
        query($repos: Int!) {
          viewer { repositories(first: $repos) { edges { node { name } } } }
        }
    "#;
    assert_eq!(node_count(document, &variables), Ok(50));
    assert_eq!(NODE_LIMIT, 500_000);

    // The same map and the same document answer the other number: one
    // `repositories` request, which is GitHub's minimum of one point.
    assert_eq!(point_aggregate(document, &variables), Ok(1));
    assert_eq!(point_cost(document, &variables), Ok(1));

    // `NodeCountError` is `#[non_exhaustive]`, so a consumer matching it must
    // carry a wildcard arm — which is what makes adding a variant non-breaking.
    // It is the *same* type either answer fails with, so this arm serves both.
    assert_eq!(point_cost("{", &variables), node_count("{", &variables));
    let error = node_count("{", &variables).expect_err("unparseable");
    let described = match &error {
        NodeCountError::Parse { .. } => "parse",
        NodeCountError::NoOperation => "no operation",
        _ => "something else",
    };
    assert_eq!(described, "parse");
    // And it is a `std::error::Error` a consumer can box.
    let boxed: Box<dyn std::error::Error> = Box::new(error);
    assert!(!boxed.to_string().is_empty());
}
