//! The walk that turns a parsed document into a node count.
//!
//! GitHub publishes the rules this implements at
//! <https://docs.github.com/en/graphql/overview/rate-limits-and-node-limits-for-the-graphql-api>.
//! Four of them are load-bearing here:
//!
//! 1. **Node counts multiply down a nested path.** A connection asking for `n`
//!    items under a parent that already yields `m` nodes contributes `m * n`, and
//!    everything nested inside it is counted against that product — which is
//!    [`Counter::selection`] passing `nodes` down as the child multiplier.
//! 2. **Node counts sum across sibling paths.** Two connections under one parent
//!    each cost their own product and the parent pays both — which is
//!    [`Counter::selection_set`] adding, rather than taking a maximum.
//! 3. **Every connection supplies a `first` or a `last` inside `1..=100`.** A
//!    page size outside that range is rejected here the way GitHub rejects it,
//!    rather than counted.
//! 4. **The limit one query may not reach is 500,000** — published as
//!    [`NODE_LIMIT`](crate::NODE_LIMIT). This module computes the count; deciding
//!    what to do about it is the caller's.

use std::collections::HashMap;

use graphql_parser::query::{
    Definition, Document, Field, OperationDefinition, Selection, SelectionSet, Value,
};

use crate::error::{NodeCountError, Position};
use crate::Variables;

/// The two arguments GitHub's connections take a page size through. A field
/// carrying either is what this crate treats as a connection — see the crate
/// documentation for what working without a schema therefore cannot detect.
const PAGE_SIZE_ARGUMENTS: [&str; 2] = ["first", "last"];

/// GitHub's page-size range: `first`/`last` must be at least 1 and at most 100.
const PAGE_SIZE_RANGE: std::ops::RangeInclusive<i64> = 1..=100;

/// How an operation with no name is named in an error message.
const ANONYMOUS: &str = "<anonymous>";

/// The document text `graphql-parser` is asked to parse names as `&str`.
type Text<'a> = &'a str;

/// The name an operation carries, and the selection set it counts over.
fn operation_parts<'a>(
    operation: &'a OperationDefinition<'a, Text<'a>>,
) -> (Option<&'a str>, &'a SelectionSet<'a, Text<'a>>) {
    match operation {
        OperationDefinition::SelectionSet(set) => (None, set),
        OperationDefinition::Query(query) => (query.name, &query.selection_set),
        OperationDefinition::Mutation(mutation) => (mutation.name, &mutation.selection_set),
        OperationDefinition::Subscription(sub) => (sub.name, &sub.selection_set),
    }
}

/// A field as the document spells it, alias included, for an error message.
fn field_label<'a>(field: &Field<'a, Text<'a>>) -> String {
    match field.alias {
        Some(alias) => format!("{alias}:{}", field.name),
        None => field.name.to_string(),
    }
}

/// What a selection would be called in an overflow message.
fn selection_label<'a>(selection: &Selection<'a, Text<'a>>) -> (String, Position) {
    match selection {
        Selection::Field(field) => (field_label(field), field.position.into()),
        Selection::FragmentSpread(spread) => (
            format!("...{}", spread.fragment_name),
            spread.position.into(),
        ),
        Selection::InlineFragment(inline) => {
            ("... (inline fragment)".to_string(), inline.position.into())
        }
    }
}

/// Turn a `None` from checked arithmetic into an attributed overflow error.
fn checked(
    value: Option<u64>,
    attribution: impl FnOnce() -> (String, Position),
) -> Result<u64, NodeCountError> {
    value.ok_or_else(|| {
        let (field, position) = attribution();
        NodeCountError::Overflow { field, position }
    })
}

/// Counts one operation, resolving spreads against the document's fragments.
struct Counter<'a> {
    fragments: HashMap<&'a str, &'a SelectionSet<'a, Text<'a>>>,
    variables: &'a Variables,
    /// The fragments currently being counted, innermost last, so a spread that
    /// re-enters one is reported instead of recursing forever.
    open_fragments: Vec<&'a str>,
}

impl<'a> Counter<'a> {
    /// Sum every selection's contribution: siblings add, they do not compete.
    fn selection_set(
        &mut self,
        set: &'a SelectionSet<'a, Text<'a>>,
        multiplier: u64,
    ) -> Result<u64, NodeCountError> {
        let mut total: u64 = 0;
        for selection in &set.items {
            let contribution = self.selection(selection, multiplier)?;
            total = checked(total.checked_add(contribution), || {
                selection_label(selection)
            })?;
        }
        Ok(total)
    }

    /// One selection's contribution under `multiplier` parent nodes.
    fn selection(
        &mut self,
        selection: &'a Selection<'a, Text<'a>>,
        multiplier: u64,
    ) -> Result<u64, NodeCountError> {
        match selection {
            Selection::Field(field) => self.field(field, multiplier),
            // An inline fragment adds no level of its own: its selections are
            // counted against the same parent nodes the spread sits under.
            Selection::InlineFragment(inline) => {
                self.selection_set(&inline.selection_set, multiplier)
            }
            Selection::FragmentSpread(spread) => {
                let name = spread.fragment_name;
                let position = Position::from(spread.position);
                let set = *self
                    .fragments
                    .get(name)
                    .ok_or(NodeCountError::UndefinedFragment {
                        name: name.to_string(),
                        position,
                    })?;
                if self.open_fragments.contains(&name) {
                    return Err(NodeCountError::FragmentCycle {
                        name: name.to_string(),
                        position,
                    });
                }
                self.open_fragments.push(name);
                let total = self.selection_set(set, multiplier);
                self.open_fragments.pop();
                total
            }
        }
    }

    /// A field's own nodes, plus everything nested beneath them.
    fn field(
        &mut self,
        field: &'a Field<'a, Text<'a>>,
        multiplier: u64,
    ) -> Result<u64, NodeCountError> {
        let Some(page_size) = self.page_size(field)? else {
            // No `first`/`last`, so this crate does not treat the field as a
            // connection: it adds no nodes and no multiplier, and its children
            // are counted against the same parent nodes it is.
            return self.selection_set(&field.selection_set, multiplier);
        };
        let label = || (field_label(field), field.position.into());
        let nodes = checked(multiplier.checked_mul(u64::from(page_size)), label)?;
        let nested = self.selection_set(&field.selection_set, nodes)?;
        checked(nodes.checked_add(nested), label)
    }

    /// The page size a field asks for, or `None` when it is not a connection.
    ///
    /// A field supplying both `first` and `last` is invalid to GitHub; the larger
    /// of the two is taken here so the answer stays the worst case.
    fn page_size(&self, field: &Field<'a, Text<'a>>) -> Result<Option<u32>, NodeCountError> {
        let mut largest: Option<u32> = None;
        for (name, value) in &field.arguments {
            if !PAGE_SIZE_ARGUMENTS.contains(name) {
                continue;
            }
            let resolved = self.resolve_page_size(field, name, value)?;
            largest = Some(largest.map_or(resolved, |seen: u32| seen.max(resolved)));
        }
        Ok(largest)
    }

    /// Resolve one `first:`/`last:` argument to a page size inside `1..=100`.
    fn resolve_page_size(
        &self,
        field: &Field<'a, Text<'a>>,
        argument: &str,
        value: &Value<'a, Text<'a>>,
    ) -> Result<u32, NodeCountError> {
        let position = Position::from(field.position);
        let raw: i64 = match value {
            // `as_i64` is infallible for a literal the parser accepted — GraphQL's
            // own `Int` is 32-bit, so an enormous literal is a parse error long
            // before it reaches here. The fallback keeps that fact from becoming a
            // branch, and lands on a value the range check below refuses anyway.
            Value::Int(number) => number.as_i64().unwrap_or(i64::MAX),
            Value::Variable(name) => i64::from(*self.variables.get(*name).ok_or_else(|| {
                NodeCountError::UnboundVariable {
                    field: field_label(field),
                    argument: argument.to_string(),
                    variable: (*name).to_string(),
                    position,
                }
            })?),
            other => {
                return Err(NodeCountError::PageSizeNotAnInteger {
                    field: field_label(field),
                    argument: argument.to_string(),
                    found: other.to_string(),
                    position,
                })
            }
        };
        if !PAGE_SIZE_RANGE.contains(&raw) {
            return Err(NodeCountError::PageSizeOutOfRange {
                field: field_label(field),
                argument: argument.to_string(),
                value: raw,
                position,
            });
        }
        // The range check above bounds `raw` to 1..=100, so this cannot truncate.
        Ok(raw as u32)
    }
}

/// Count the one operation in `document` under `variables`.
///
/// This is [`crate::node_count`]'s body; the split keeps the public module free
/// of the walk.
pub(crate) fn count(document: &str, variables: &Variables) -> Result<u64, NodeCountError> {
    let parsed: Document<'_, Text<'_>> =
        graphql_parser::parse_query(document).map_err(|error| NodeCountError::Parse {
            message: error.to_string(),
        })?;

    let mut fragments = HashMap::new();
    let mut operations = Vec::new();
    for definition in &parsed.definitions {
        match definition {
            Definition::Operation(operation) => operations.push(operation),
            Definition::Fragment(fragment) => {
                fragments.insert(fragment.name, &fragment.selection_set);
            }
        }
    }

    let operation = match operations.as_slice() {
        [] => return Err(NodeCountError::NoOperation),
        [only] => *only,
        many => {
            return Err(NodeCountError::MultipleOperations {
                names: many
                    .iter()
                    .map(|operation| {
                        operation_parts(operation)
                            .0
                            .unwrap_or(ANONYMOUS)
                            .to_string()
                    })
                    .collect(),
            })
        }
    };

    let mut counter = Counter {
        fragments,
        variables,
        open_fragments: Vec::new(),
    };
    // One operation node is not itself counted: GitHub's own worked examples
    // start the multiplier at the single root the operation returns.
    counter.selection_set(operation_parts(operation).1, 1)
}
