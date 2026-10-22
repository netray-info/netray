//! The pages of a stored entry (the entry page, the gone page and the privacy page) and the
//! routes that serve `/r/<id>` from the history; an ID that is no stored entry answers 410.

pub mod render;
mod routes;
pub mod view;

pub(crate) use routes::well_formed;
pub use routes::{CardRenderer, EntriesDeps, EntryCard, NOT_FOUND_HTML, privacy_router, router};
pub use view::{EntryPageView, StatusCounts};
