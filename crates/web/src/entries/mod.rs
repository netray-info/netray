//! The pages of a stored entry (the entry page, the gone page and the privacy page) and the
//! routes that serve `/r/<id>`: history first, then the V1 snapshots lens hands in.

pub mod render;
mod routes;
pub mod view;

pub use routes::{CardRenderer, EntriesDeps, EntryCard, NOT_FOUND_HTML, V1Lookup, router};
pub use view::{EntryPageView, StatusCounts};
