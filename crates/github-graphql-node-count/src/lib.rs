//! Compute the worst-case **node count** GitHub's GraphQL API attributes to a
//! query — offline, from the query text and its page-size variable bindings
//! alone, before the query is ever sent.
//!
//! # `nodeCount`, not `cost`
//!
//! GitHub meters two different numbers against two different limits, and this
//! crate is about one of them.
//!
//! * **`nodeCount`** — the maximum number of nodes **one query may return**,
//!   limited **per query** at [`NODE_LIMIT`]. That is what [`node_count`]
//!   computes.
//! * **`cost`** — the rate-limit **points** a call spends, metered **per hour**
//!   across everything one credential does.
//!
//! Neither the code nor these docs computes or claims anything about `cost`.
//!
//! # The rules, and where they come from
//!
//! GitHub publishes them at
//! <https://docs.github.com/en/graphql/overview/rate-limits-and-node-limits-for-the-graphql-api>:
//! node counts **multiply** down a nested path, **sum** across sibling paths,
//! every connection supplies a `first` or a `last` inside `1..=100`, and the
//! limit one query may not reach is 500,000. Check this crate against that page
//! rather than against our confidence.
//!
//! # No schema, by design
//!
//! [`node_count`] is handed document text and nothing else. It reaches no
//! network, reads no credential, and consults no GraphQL schema — so it cannot
//! know which fields are connections by type. Instead:
//!
//! * a field carrying a `first:` or a `last:` argument **is** a connection, and
//!   multiplies the count beneath it;
//! * every other field contributes no multiplier and no nodes of its own.
//!
//! That is what makes this crate runnable in a fork pull request with no secret,
//! which is the whole reason a consumer can put it in a gate. The cost is one
//! blind spot, stated plainly: **a connection that supplies neither `first` nor
//! `last` is invalid to GitHub and invisible here.** Such a field is counted as
//! an ordinary field, so the answer is an undercount rather than an error. A
//! field supplying *both* is also invalid to GitHub; the larger of the two is
//! used, so the answer stays a worst case.
//!
//! # Example
//!
//! ```
//! use github_graphql_node_count::{node_count, NodeCountError, Variables, NODE_LIMIT};
//!
//! let document = r#"
//!     query($repos: Int!) {
//!       viewer {
//!         repositories(first: $repos) {
//!           edges { node { name issues(first: 10) { edges { node { title } } } } }
//!         }
//!       }
//!     }
//! "#;
//! let variables = Variables::from([("repos".to_string(), 50)]);
//!
//! // 50 repositories + 50 x 10 issues.
//! assert_eq!(node_count(document, &variables)?, 550);
//! assert!(node_count(document, &variables)? < NODE_LIMIT);
//!
//! // A page size GitHub would reject comes back as an error, not a number.
//! let over = Variables::from([("repos".to_string(), 500)]);
//! assert!(matches!(
//!     node_count(document, &over),
//!     Err(NodeCountError::PageSizeOutOfRange { .. })
//! ));
//! # Ok::<(), NodeCountError>(())
//! ```

#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

mod count;
mod error;

pub use error::{NodeCountError, PageSizeArgument, Position};

/// GitHub's published limit on the number of nodes one query may return.
///
/// > Individual calls cannot request more than 500,000 total nodes.
///
/// — [Rate limits and node limits for the GraphQL API](https://docs.github.com/en/graphql/overview/rate-limits-and-node-limits-for-the-graphql-api).
///
/// This is a **per-query** ceiling on `nodeCount`. It is not the hourly `cost`
/// budget, which this crate says nothing about.
pub const NODE_LIMIT: u64 = 500_000;

/// The integer bound each page-size variable a document names is given, keyed by
/// variable name **without** the leading `$`.
///
/// A variable a document declares but no `first:`/`last:` argument references
/// need not appear. A variable a `first:`/`last:` *does* reference must, or
/// [`node_count`] returns [`NodeCountError::UnboundVariable`] — a declared
/// default value is not consulted.
///
/// A `u32` rather than a validated page-size newtype: this alias is a contract
/// with the repositories that call this crate, restated verbatim in their own
/// builds, so it is not ours to narrow. The `1..=100` range GitHub requires is
/// enforced where the value is *used*, and a binding outside it comes back as
/// [`NodeCountError::PageSizeOutOfRange`] rather than being silently counted.
// llmlint: ignore[invalid_states_unrepresentable] the type of this alias is a frozen
// cross-repository contract (see the paragraph above); narrowing it to a newtype would break
// the consumer written against it, so the range is validated at the one point of use instead.
pub type Variables = std::collections::BTreeMap<String, u32>;

/// The worst-case number of nodes the one operation in `document` may return,
/// under `variables`, computed by GitHub's published rules.
///
/// The document must hold exactly one operation. Fragment definitions beside it
/// are resolved, so the natural consumer shape — one shared fragment
/// concatenated onto each of several operations, giving several
/// single-operation documents — is counted correctly, once per document.
///
/// # Errors
///
/// Returns [`NodeCountError`] rather than panicking, returning zero, or
/// returning a wrong count, when:
///
/// * the text does not parse as GraphQL ([`NodeCountError::Parse`]);
/// * the document declares no operation ([`NodeCountError::NoOperation`]);
/// * it declares more than one, which this signature cannot disambiguate
///   ([`NodeCountError::MultipleOperations`]);
/// * a `first:`/`last:` names a variable `variables` does not bind
///   ([`NodeCountError::UnboundVariable`]);
/// * a `first:`/`last:` value falls outside `1..=100`
///   ([`NodeCountError::PageSizeOutOfRange`]) or is not an integer at all
///   ([`NodeCountError::PageSizeNotAnInteger`]);
/// * a spread names a fragment the document does not define
///   ([`NodeCountError::UndefinedFragment`]), or the spreads form a cycle
///   ([`NodeCountError::FragmentCycle`]);
/// * the count grows past `u64` ([`NodeCountError::Overflow`]), which only a
///   document far above [`NODE_LIMIT`] can do.
pub fn node_count(document: &str, variables: &Variables) -> Result<u64, NodeCountError> {
    count::count(document, variables)
}
