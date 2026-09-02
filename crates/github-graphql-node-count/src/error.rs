//! The one error type [`node_count`](crate::node_count) returns.

use std::fmt;

use graphql_parser::Pos;

/// Which argument a connection took its page size from.
///
/// GraphQL and GitHub define exactly these two, so the set is closed and a
/// consumer may match it exhaustively.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageSizeArgument {
    /// `first:` — a forward page.
    First,
    /// `last:` — a backward page.
    Last,
}

impl PageSizeArgument {
    /// The argument name as a document spells it.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::First => "first",
            Self::Last => "last",
        }
    }
}

impl fmt::Display for PageSizeArgument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Where in the document a problem was found, rendered as `line:column`.
///
/// `graphql-parser` reports a position for every field and fragment, so an error
/// can point a reader at the text that caused it rather than at the whole
/// document.
// llmlint: ignore[invalid_states_unrepresentable] a 1-based line/column arrives from the
// parser as a plain `usize` and is only ever rendered; narrowing it to `NonZeroUsize` would
// buy an unrepresentable zero at the price of an unreachable fallback at the `From<Pos>`
// boundary — trading a state nothing constructs for a branch no test can cover.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    /// 1-based line number.
    pub line: usize,
    /// 1-based column number.
    pub column: usize,
}

impl From<Pos> for Position {
    fn from(pos: Pos) -> Self {
        Self {
            line: pos.line,
            column: pos.column,
        }
    }
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.column)
    }
}

/// Why a document's node count could not be computed.
///
/// Marked `#[non_exhaustive]`: a future release may add a variant without that
/// being a breaking change, so match with a `_ =>` arm.
///
/// Every variant's [`Display`](fmt::Display) names the field or the document
/// position at fault, so a consumer can report which part of its query is wrong
/// without re-parsing the document itself.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum NodeCountError {
    /// The text is not a GraphQL document.
    Parse {
        /// The parser's own message, which carries the position it stopped at.
        message: String,
    },
    /// The document declares no operation, so there is nothing to count.
    NoOperation,
    /// The document declares more than one operation.
    ///
    /// [`node_count`](crate::node_count) counts *the* operation in a document and
    /// takes no operation name, so it cannot say which one was meant. Split the
    /// document and ask once per operation.
    MultipleOperations {
        /// The operations found, named in document order; an anonymous operation
        /// appears as `<anonymous>`.
        names: Vec<String>,
    },
    /// A `first:`/`last:` argument names a variable the caller did not bind.
    UnboundVariable {
        /// The field carrying the argument, as the document spells it.
        field: String,
        /// Which page-size argument it was.
        argument: PageSizeArgument,
        /// The variable name, without the leading `$`.
        variable: String,
        /// Where the field appears in the document.
        position: Position,
    },
    /// A `first:`/`last:` page size falls outside GitHub's `1..=100`.
    PageSizeOutOfRange {
        /// The field carrying the argument, as the document spells it.
        field: String,
        /// Which page-size argument it was.
        argument: PageSizeArgument,
        /// The page size that was rejected.
        value: i64,
        /// Where the field appears in the document.
        position: Position,
    },
    /// A `first:`/`last:` argument is neither an integer literal nor a variable.
    PageSizeNotAnInteger {
        /// The field carrying the argument, as the document spells it.
        field: String,
        /// Which page-size argument it was.
        argument: PageSizeArgument,
        /// The argument value as the document wrote it.
        found: String,
        /// Where the field appears in the document.
        position: Position,
    },
    /// A spread names a fragment the document does not define.
    UndefinedFragment {
        /// The fragment name, without the leading `...`.
        name: String,
        /// Where the spread appears in the document.
        position: Position,
    },
    /// A fragment spreads itself, directly or through other fragments.
    ///
    /// GraphQL forbids this; counting it would not terminate.
    FragmentCycle {
        /// The fragment the cycle was detected re-entering.
        name: String,
        /// Where the spread that closes the cycle appears.
        position: Position,
    },
    /// The count grew past what a `u64` can express.
    ///
    /// Reachable only from a document nesting enough maximum-size pages that the
    /// running multiplier overflows — far above [`NODE_LIMIT`](crate::NODE_LIMIT),
    /// so such a document is over the limit whatever the exact number would be.
    Overflow {
        /// The field whose multiplication overflowed.
        field: String,
        /// Where that field appears in the document.
        position: Position,
    },
}

impl fmt::Display for NodeCountError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse { message } => {
                write!(f, "the document is not valid GraphQL: {message}")
            }
            Self::NoOperation => f.write_str("the document declares no operation to count"),
            Self::MultipleOperations { names } => write!(
                f,
                "the document declares {} operations ({}); node_count counts one \
                 operation and cannot tell which was meant",
                names.len(),
                names.join(", "),
            ),
            Self::UnboundVariable {
                field,
                argument,
                variable,
                position,
            } => write!(
                f,
                "at {position}, field `{field}` passes `{argument}: ${variable}`, \
                 but no binding for `{variable}` was supplied",
            ),
            Self::PageSizeOutOfRange {
                field,
                argument,
                value,
                position,
            } => write!(
                f,
                "at {position}, field `{field}` passes `{argument}: {value}`, \
                 outside GitHub's page-size range 1..=100",
            ),
            Self::PageSizeNotAnInteger {
                field,
                argument,
                found,
                position,
            } => write!(
                f,
                "at {position}, field `{field}` passes `{argument}: {found}`, \
                 which is neither an integer nor a variable",
            ),
            Self::UndefinedFragment { name, position } => write!(
                f,
                "at {position}, the spread `...{name}` names a fragment the document \
                 does not define",
            ),
            Self::FragmentCycle { name, position } => write!(
                f,
                "at {position}, the spread `...{name}` re-enters a fragment already \
                 being counted; fragment spreads must not form a cycle",
            ),
            Self::Overflow { field, position } => write!(
                f,
                "at {position}, the node count for field `{field}` grew past what a \
                 64-bit integer can express; the document is far above NODE_LIMIT",
            ),
        }
    }
}

impl std::error::Error for NodeCountError {}
