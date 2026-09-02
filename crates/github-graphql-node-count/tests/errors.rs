//! Every documented failure comes back as an error naming the field or position
//! at fault — not a panic, not a zero, not a wrong count.

use std::error::Error;

use github_graphql_node_count::{
    node_count, NodeCountError, PageSizeArgument, Position, Variables,
};

/// No page-size variables bound at all.
fn unbound() -> Variables {
    Variables::new()
}

/// The error `document` produces, or a panic naming the count it wrongly gave.
fn error_from(document: &str, variables: &Variables) -> NodeCountError {
    match node_count(document, variables) {
        Err(error) => error,
        Ok(total) => panic!("expected an error, got a count of {total}"),
    }
}

const UNPARSEABLE: &str = "query { viewer { repositories(first: 10) ";

#[test]
fn text_that_is_not_graphql_is_an_error() {
    let error = error_from(UNPARSEABLE, &unbound());
    assert!(matches!(error, NodeCountError::Parse { .. }), "{error:?}");
    let shown = error.to_string();
    assert!(
        shown.starts_with("the document is not valid GraphQL:"),
        "{shown}"
    );
}

const FRAGMENT_ONLY: &str = r#"
fragment RepositoryIssues on Repository {
  issues(first: 10) { edges { node { title } } }
}
"#;

#[test]
fn a_document_with_no_operation_is_an_error() {
    let error = error_from(FRAGMENT_ONLY, &unbound());
    assert_eq!(error, NodeCountError::NoOperation);
    assert_eq!(
        error.to_string(),
        "the document declares no operation to count"
    );
}

const TWO_OPERATIONS: &str = r#"
query ViewerRepositories {
  viewer { repositories(first: 10) { edges { node { name } } } }
}

query ViewerFollowers {
  viewer { followers(first: 10) { edges { node { login } } } }
}
"#;

#[test]
fn a_document_with_two_operations_is_an_error_rather_than_a_guess() {
    let error = error_from(TWO_OPERATIONS, &unbound());
    assert_eq!(
        error,
        NodeCountError::MultipleOperations {
            names: vec![
                "ViewerRepositories".to_string(),
                "ViewerFollowers".to_string()
            ],
        }
    );
    let shown = error.to_string();
    assert!(
        shown.contains("ViewerRepositories, ViewerFollowers"),
        "{shown}"
    );
    assert!(shown.contains("2 operations"), "{shown}");
}

const TWO_ANONYMOUS_OPERATIONS: &str = r#"
{ viewer { repositories(first: 1) { edges { node { name } } } } }
{ viewer { followers(first: 1) { edges { node { login } } } } }
"#;

#[test]
fn anonymous_operations_are_named_in_the_error() {
    let error = error_from(TWO_ANONYMOUS_OPERATIONS, &unbound());
    assert_eq!(
        error,
        NodeCountError::MultipleOperations {
            names: vec!["<anonymous>".to_string(), "<anonymous>".to_string()],
        }
    );
}

const UNBOUND_PAGE_VARIABLE: &str = r#"
query ViewerRepositories($page: Int!) {
  viewer {
    repositories(first: $page) { edges { node { name } } }
  }
}
"#;

#[test]
fn a_page_size_variable_the_caller_did_not_bind_is_an_error() {
    let error = error_from(UNBOUND_PAGE_VARIABLE, &unbound());
    assert_eq!(
        error,
        NodeCountError::UnboundVariable {
            field: "repositories".to_string(),
            argument: PageSizeArgument::First,
            variable: "page".to_string(),
            position: Position { line: 4, column: 5 },
        }
    );
    let shown = error.to_string();
    assert!(shown.contains("field `repositories`"), "{shown}");
    assert!(shown.contains("`first: $page`"), "{shown}");
    assert!(shown.contains("at 4:5,"), "{shown}");
}

/// A declared default is not a binding: the caller supplies the number, so a
/// document counted against a default nobody passed would be counting a query
/// nobody sent.
const PAGE_VARIABLE_WITH_A_DEFAULT: &str = r#"
query ViewerRepositories($page: Int = 10) {
  viewer {
    repositories(first: $page) { edges { node { name } } }
  }
}
"#;

#[test]
fn a_declared_default_does_not_stand_in_for_a_binding() {
    let error = error_from(PAGE_VARIABLE_WITH_A_DEFAULT, &unbound());
    assert!(
        matches!(error, NodeCountError::UnboundVariable { .. }),
        "{error:?}"
    );
    // Bound, it counts.
    let bound = Variables::from([("page".to_string(), 10)]);
    assert_eq!(node_count(PAGE_VARIABLE_WITH_A_DEFAULT, &bound), Ok(10));
}

const LITERAL_ABOVE_THE_RANGE: &str = r#"
query {
  viewer {
    repositories(first: 101) { edges { node { name } } }
  }
}
"#;

const LITERAL_BELOW_THE_RANGE: &str = r#"
query {
  viewer {
    followers(last: 0) { edges { node { login } } }
  }
}
"#;

const LITERAL_NEGATIVE: &str = r#"
query {
  viewer {
    followers(first: -5) { edges { node { login } } }
  }
}
"#;

#[test]
fn a_literal_page_size_outside_the_range_is_an_error() {
    let error = error_from(LITERAL_ABOVE_THE_RANGE, &unbound());
    assert_eq!(
        error,
        NodeCountError::PageSizeOutOfRange {
            field: "repositories".to_string(),
            argument: PageSizeArgument::First,
            value: 101,
            position: Position { line: 4, column: 5 },
        }
    );
    let shown = error.to_string();
    assert!(
        shown.contains("field `repositories` passes `first: 101`"),
        "{shown}"
    );
    assert!(shown.contains("1..=100"), "{shown}");

    assert!(
        matches!(
            error_from(LITERAL_BELOW_THE_RANGE, &unbound()),
            NodeCountError::PageSizeOutOfRange { value: 0, .. }
        ),
        "a page size of 0 must be refused"
    );
    assert!(
        matches!(
            error_from(LITERAL_NEGATIVE, &unbound()),
            NodeCountError::PageSizeOutOfRange { value: -5, .. }
        ),
        "a negative page size must be refused"
    );
}

#[test]
fn a_bound_variable_outside_the_range_is_an_error() {
    let over = Variables::from([("page".to_string(), 101)]);
    assert!(
        matches!(
            error_from(UNBOUND_PAGE_VARIABLE, &over),
            NodeCountError::PageSizeOutOfRange { value: 101, .. }
        ),
        "a variable bound above the range must be refused"
    );
    let zero = Variables::from([("page".to_string(), 0)]);
    assert!(
        matches!(
            error_from(UNBOUND_PAGE_VARIABLE, &zero),
            NodeCountError::PageSizeOutOfRange { value: 0, .. }
        ),
        "a variable bound to zero must be refused"
    );

    // The edges of the range are inside it.
    for (page, expected) in [(1u32, 1u64), (100, 100)] {
        let variables = Variables::from([("page".to_string(), page)]);
        assert_eq!(node_count(UNBOUND_PAGE_VARIABLE, &variables), Ok(expected));
    }
}

/// A literal too large for a signed 64-bit integer never reaches the range
/// check: GraphQL's own `Int` is 32-bit, so the parser refuses it first. The
/// answer is still an error naming where it stopped.
const LITERAL_ENORMOUS: &str = r#"
query {
  viewer {
    repositories(first: 99999999999999999999999) { edges { node { name } } }
  }
}
"#;

#[test]
fn a_page_size_too_large_for_an_i64_is_refused_by_the_parser() {
    let error = error_from(LITERAL_ENORMOUS, &unbound());
    assert!(matches!(error, NodeCountError::Parse { .. }), "{error:?}");
    assert!(error.to_string().contains("4:25"), "{error}");
}

const PAGE_SIZE_IS_A_STRING: &str = r#"
query {
  viewer {
    repositories(first: "ten") { edges { node { name } } }
  }
}
"#;

#[test]
fn a_page_size_that_is_not_an_integer_is_an_error() {
    let error = error_from(PAGE_SIZE_IS_A_STRING, &unbound());
    assert_eq!(
        error,
        NodeCountError::PageSizeNotAnInteger {
            field: "repositories".to_string(),
            argument: PageSizeArgument::First,
            found: "\"ten\"".to_string(),
            position: Position { line: 4, column: 5 },
        }
    );
    let shown = error.to_string();
    assert!(
        shown.contains("neither an integer nor a variable"),
        "{shown}"
    );
}

const UNDEFINED_FRAGMENT: &str = r#"
query {
  viewer {
    repositories(first: 10) {
      edges { node { ...RepositoryIssues } }
    }
  }
}
"#;

#[test]
fn a_spread_naming_an_undefined_fragment_is_an_error() {
    let error = error_from(UNDEFINED_FRAGMENT, &unbound());
    assert_eq!(
        error,
        NodeCountError::UndefinedFragment {
            name: "RepositoryIssues".to_string(),
            position: Position {
                line: 5,
                column: 25
            },
        }
    );
    let shown = error.to_string();
    assert!(shown.contains("...RepositoryIssues"), "{shown}");
    assert!(shown.contains("at 5:25,"), "{shown}");
}

const FRAGMENT_CYCLE: &str = r#"
query {
  viewer { ...Outer }
}

fragment Outer on User {
  repositories(first: 2) { edges { node { ...Inner } } }
}

fragment Inner on Repository {
  owner { ...Outer }
}
"#;

#[test]
fn a_cycle_of_fragment_spreads_is_an_error_rather_than_a_hang() {
    let error = error_from(FRAGMENT_CYCLE, &unbound());
    assert!(
        matches!(&error, NodeCountError::FragmentCycle { name, .. } if name == "Outer"),
        "{error:?}"
    );
    let shown = error.to_string();
    assert!(shown.contains("must not form a cycle"), "{shown}");
}

/// One fragment spread twice on sibling paths is not a cycle: the counter
/// re-enters it only after leaving it.
const FRAGMENT_SPREAD_TWICE: &str = r#"
query {
  viewer {
    repositories(first: 2) { edges { node { ...Titles } } }
    starredRepositories(first: 3) { edges { node { ...Titles } } }
  }
}

fragment Titles on Repository {
  issues(first: 5) { edges { node { title } } }
}
"#;

/// 2 + 2 x 5 repositories' issues, plus 3 + 3 x 5 starred repositories' issues.
const FRAGMENT_SPREAD_TWICE_NODES: u64 = 2 + 10 + 3 + 15;

#[test]
fn one_fragment_spread_on_two_paths_is_not_a_cycle() {
    assert_eq!(
        node_count(FRAGMENT_SPREAD_TWICE, &unbound()),
        Ok(FRAGMENT_SPREAD_TWICE_NODES)
    );
}

/// A document nesting `deep(first: 100)` `levels` deep, with `innermost` spliced
/// into the middle. Generated rather than transcribed: these fixtures exist to
/// drive the arithmetic past `u64`, not to reproduce a published rule, and a
/// hand-written eleven-level query would be less legible, not more.
fn deeply_nested(levels: usize, innermost: &str) -> String {
    let mut document = innermost.to_string();
    for _ in 0..levels {
        document = format!("deep(first: 100) {{ {document} }}");
    }
    format!("query {{ {document} }}")
}

#[test]
fn a_multiplier_that_outgrows_a_u64_is_an_error() {
    // 100^10 is 1e20, past u64::MAX (about 1.8e19).
    let error = error_from(&deeply_nested(10, "name"), &unbound());
    assert!(
        matches!(&error, NodeCountError::Overflow { field, .. } if field == "deep"),
        "{error:?}"
    );
    let shown = error.to_string();
    assert!(
        shown.contains("past what a 64-bit integer can express"),
        "{shown}"
    );

    // One level shallower still answers: 100^9 nodes at the deepest level.
    let counted = node_count(&deeply_nested(9, "name"), &unbound());
    assert!(counted.is_ok(), "{counted:?}");
}

#[test]
fn a_field_whose_own_nodes_plus_its_subtree_outgrow_a_u64_is_an_error() {
    // At 100^9 = 1e18 parents, `wide(first: 10)` is 1e19 nodes and its single
    // child adds 1e19 more, so the field's own addition overflows.
    let document = deeply_nested(9, "wide(first: 10) { tail(first: 1) { name } }");
    let error = error_from(&document, &unbound());
    assert!(
        matches!(&error, NodeCountError::Overflow { field, .. } if field == "wide"),
        "{error:?}"
    );
}

#[test]
fn a_sum_across_siblings_that_outgrows_a_u64_is_an_error() {
    // Two siblings of 1e19 nodes each: neither multiplication overflows, their
    // sum does.
    let document = deeply_nested(9, "left(first: 10) { name } right(first: 10) { name }");
    let error = error_from(&document, &unbound());
    assert!(
        matches!(&error, NodeCountError::Overflow { field, .. } if field == "right"),
        "{error:?}"
    );
}

#[test]
fn the_error_type_is_a_std_error_that_is_debuggable_and_comparable() {
    let error = error_from(UNDEFINED_FRAGMENT, &unbound());
    // `std::error::Error`, reachable as a trait object the way a consumer's
    // `Box<dyn Error>` holds it.
    let boxed: Box<dyn Error> = Box::new(error.clone());
    assert_eq!(boxed.to_string(), error.to_string());
    assert!(boxed.source().is_none());

    // Debug renders the variant and its fields.
    let debugged = format!("{error:?}");
    assert!(debugged.contains("UndefinedFragment"), "{debugged}");
    assert!(debugged.contains("RepositoryIssues"), "{debugged}");

    // Clone and PartialEq.
    assert_eq!(error.clone(), error);
    assert_ne!(error, NodeCountError::NoOperation);
}

#[test]
fn a_position_renders_as_line_and_column() {
    let position = Position {
        line: 12,
        column: 7,
    };
    assert_eq!(position.to_string(), "12:7");
    // Copy, Clone, PartialEq and Debug, as a consumer reporting one would use.
    let copied = position;
    assert_eq!(copied, position.clone());
    assert_ne!(
        copied,
        Position {
            line: 12,
            column: 8
        }
    );
    assert_eq!(format!("{position:?}"), "Position { line: 12, column: 7 }");
}

#[test]
fn a_sum_overflowing_on_a_spread_names_the_spread() {
    let document = format!(
        "{}\nfragment Right on Thing {{ right(first: 10) {{ name }} }}",
        deeply_nested(9, "left(first: 10) { name } ...Right"),
    );
    let error = error_from(&document, &unbound());
    assert!(
        matches!(&error, NodeCountError::Overflow { field, .. } if field == "...Right"),
        "{error:?}"
    );
}

#[test]
fn a_sum_overflowing_on_an_inline_fragment_names_it() {
    let document = deeply_nested(
        9,
        "left(first: 10) { name } ... on Thing { right(first: 10) { name } }",
    );
    let error = error_from(&document, &unbound());
    assert!(
        matches!(&error, NodeCountError::Overflow { field, .. } if field == "... (inline fragment)"),
        "{error:?}"
    );
}

/// An aliased connection is reported the way the document spells it, so a
/// consumer reading the message can find the line.
const ALIASED_CONNECTION_OUT_OF_RANGE: &str = r#"
query {
  viewer {
    repos: repositories(first: 250) { edges { node { name } } }
  }
}
"#;

#[test]
fn an_error_on_an_aliased_field_names_the_alias_and_the_field() {
    let error = error_from(ALIASED_CONNECTION_OUT_OF_RANGE, &unbound());
    assert_eq!(
        error,
        NodeCountError::PageSizeOutOfRange {
            field: "repos:repositories".to_string(),
            argument: PageSizeArgument::First,
            value: 250,
            position: Position { line: 4, column: 5 },
        }
    );
    assert!(
        error.to_string().contains("`repos:repositories`"),
        "{error}"
    );
}

#[test]
fn the_page_size_argument_names_the_two_arguments_github_defines() {
    assert_eq!(PageSizeArgument::First.as_str(), "first");
    assert_eq!(PageSizeArgument::Last.as_str(), "last");
    assert_eq!(PageSizeArgument::Last.to_string(), "last");
    assert_eq!(format!("{:?}", PageSizeArgument::First), "First");
    assert_ne!(PageSizeArgument::First, PageSizeArgument::Last);
    // Copy and Clone, as a consumer holding one out of an error would use them.
    let copied = PageSizeArgument::Last;
    assert_eq!(copied, PageSizeArgument::Last.clone());

    // The `last:` spelling reaches the same error path, named as itself.
    let error = error_from(LITERAL_BELOW_THE_RANGE, &unbound());
    assert!(
        matches!(
            error,
            NodeCountError::PageSizeOutOfRange {
                argument: PageSizeArgument::Last,
                ..
            }
        ),
        "{error:?}"
    );
    assert!(error.to_string().contains("`last: 0`"), "{error}");
}
