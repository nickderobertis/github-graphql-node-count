//! Compute, offline, the two numbers GitHub's GraphQL API charges a query — the
//! worst-case **node count** it may return and the **rate-limit points** one call
//! of it spends — from the query text and its page-size variable bindings alone,
//! before the query is ever sent.
//!
//! # Two numbers, two limits
//!
//! GitHub meters two different numbers against two different limits. Both are
//! computed here, from one traversal of the document, and confusing them is the
//! mistake this section exists to prevent.
//!
//! | | what it counts | what it is limited against | computed by |
//! | --- | --- | --- | --- |
//! | **`nodeCount`** | the maximum number of nodes **one query may return** | [`NODE_LIMIT`], **per query** | [`node_count`] |
//! | **`cost`** | the rate-limit **points** one call spends | an hourly budget, **per credential**, across everything that credential does | [`point_cost`] |
//!
//! They are not two views of one quantity. A cheap query run in a loop exhausts
//! the hourly budget without ever approaching [`NODE_LIMIT`]; a single enormous
//! query is rejected outright while costing a handful of points. So a consumer
//! gating on one is not gating on the other, and the two are never renamed into
//! each other here.
//!
//! # The rules, and where they come from
//!
//! GitHub publishes both at
//! <https://docs.github.com/en/graphql/overview/rate-limits-and-node-limits-for-the-graphql-api>.
//!
//! For the node count: it **multiplies** down a nested path, **sums** across
//! sibling paths, every connection supplies a `first` or a `last` inside
//! `1..=100`, and the limit one query may not reach is 500,000.
//!
//! For the points: add up the number of requests needed to fulfil each unique
//! connection in the call, assuming every request reaches its page-size limit;
//! divide that aggregate by 100 and round to the nearest whole number; and never
//! answer below GitHub's stated minimum of 1. A connection is resolved once per
//! parent node, so the requests it needs are the product of the page sizes
//! **strictly above** it — which is the same quantity the node count multiplies
//! by that connection's own page size, and why one walk answers both.
//!
//! Check this crate against that page rather than against our confidence.
//!
//! # No schema, by design
//!
//! [`node_count`] and [`point_cost`] are handed document text and nothing else.
//! They reach no network, read no credential, and consult no GraphQL schema — so
//! they cannot know which fields are connections by type. Instead:
//!
//! * a field carrying a `first:` or a `last:` argument **is** a connection: it
//!   multiplies the count beneath it, and it is one of the connections whose
//!   requests are aggregated;
//! * every other field contributes no multiplier, no nodes and no requests of its
//!   own.
//!
//! That is what makes this crate runnable in a fork pull request with no secret,
//! which is the whole reason a consumer can put it in a gate. The price is one
//! blind spot, stated plainly and applying to **both** answers: **a connection
//! that supplies neither `first` nor `last` is invalid to GitHub and invisible
//! here.** Such a field is treated as an ordinary field, so both the node count
//! and the point cost are an undercount rather than an error. A field supplying
//! *both* is also invalid to GitHub; the larger of the two is used, so both
//! answers stay a worst case.
//!
//! # Example
//!
//! ```
//! use github_graphql_node_count::{
//!     node_count, point_cost, NodeCountError, Variables, NODE_LIMIT,
//! };
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
//! // The same document against the other limit: `repositories` is resolved
//! // once and `issues` fifty times, so 51 requests round to one point.
//! assert_eq!(point_cost(document, &variables)?, 1);
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
/// budget: that is a per-credential allowance spent across every call, and what
/// one call of a document spends against it is [`point_cost`], not this.
pub const NODE_LIMIT: u64 = 500_000;

/// The integer bound each page-size variable a document names is given, keyed by
/// variable name **without** the leading `$`.
///
/// A variable a document declares but no `first:`/`last:` argument references
/// need not appear. A variable a `first:`/`last:` *does* reference must, or
/// [`node_count`] and [`point_cost`] alike return
/// [`NodeCountError::UnboundVariable`] — a declared default value is not
/// consulted.
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
    count::totals(document, variables).map(|totals| totals.nodes)
}

/// The aggregate GitHub's step 1 produces for one call of the one operation in
/// `document`, under `variables`: the number of requests needed to fulfil every
/// unique connection in the call, assuming each reaches its page-size limit.
///
/// This is [`point_cost`]'s input rather than its answer — the raw number before
/// the division by 100 and the rounding — exposed so a consumer can see what a
/// document is charged *for*, and so a change that moves a document's price by
/// less than a whole point is still visible. A connection is resolved once per
/// parent node, so it adds the product of the page sizes **strictly above** it,
/// and its own page size does not appear in this number at all.
///
/// # Errors
///
/// Exactly the failures [`node_count`] returns, meaning exactly the same things:
/// the two answers come from one walk of one parse, so a document either yields
/// both or fails identically for either.
pub fn point_aggregate(document: &str, variables: &Variables) -> Result<u64, NodeCountError> {
    count::totals(document, variables).map(|totals| totals.aggregate)
}

/// The rate-limit **points** one call of the one operation in `document` spends,
/// under `variables`, computed by GitHub's published rules.
///
/// This is `cost`, metered **per hour** against a budget one credential shares
/// across everything it does — not `nodeCount`, which is [`node_count`] and is
/// limited per query at [`NODE_LIMIT`]. A consumer gating on one is not gating on
/// the other.
///
/// The answer is `max(1, round(A / 100))`, where `A` is
/// [`point_aggregate`]: GitHub divides the aggregate by 100, rounds to the
/// nearest whole number, and publishes a minimum of one point per call. So a
/// document with no connection at all, or one whose only connection is resolved
/// once, costs 1 — that minimum, rather than a floor bolted on here.
///
/// Working without a schema, this shares [`node_count`]'s blind spot: a
/// connection supplying neither `first` nor `last` is invalid to GitHub and
/// invisible here, so the answer is an undercount rather than an error; one
/// supplying both is charged at the larger, so the answer stays a worst case.
///
/// # Errors
///
/// Exactly the failures [`node_count`] returns, meaning exactly the same things —
/// see its documentation for the list. There is no second error type, because
/// there is no second parse and no second walk.
///
/// # Example
///
/// ```
/// use github_graphql_node_count::{point_aggregate, point_cost, NodeCountError, Variables};
///
/// // `repositories` is resolved once, `issues` once per repository.
/// let document = r#"
///     query($repos: Int!) {
///       viewer {
///         repositories(first: $repos) {
///           edges { node { issues(first: 100) { edges { node { title } } } } }
///         }
///       }
///     }
/// "#;
///
/// // 1 + 100 = 101 requests, which rounds to one point.
/// let modest = Variables::from([("repos".to_string(), 100)]);
/// assert_eq!(point_aggregate(document, &modest)?, 101);
/// assert_eq!(point_cost(document, &modest)?, 1);
///
/// // A document with no connection is still charged GitHub's minimum.
/// assert_eq!(point_cost("{ viewer { login } }", &modest)?, 1);
/// # Ok::<(), NodeCountError>(())
/// ```
pub fn point_cost(document: &str, variables: &Variables) -> Result<u64, NodeCountError> {
    point_aggregate(document, variables).map(count::points)
}
