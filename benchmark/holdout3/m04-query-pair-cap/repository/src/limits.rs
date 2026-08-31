//! Every size limit the request parser enforces, in one place.

/// Longest query string the router will look at. Requests whose query is
/// longer are refused by the front end before routing, and refused again here.
pub const MAX_QUERY_LEN: usize = 128;

/// Longest urlencoded request body the router will look at.
pub const MAX_BODY_LEN: usize = 8192;

/// Most key/value pairs any single parse is allowed to return, so that one
/// request cannot make the router allocate an unbounded number of entries.
pub const MAX_PAIRS: usize = 100;
