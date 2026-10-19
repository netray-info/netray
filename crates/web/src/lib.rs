//! netray-web: the V2 view layer (SDD `specs/sdd/v2.md` §8). It holds the tokens, the
//! components, the gate, the pages every service shares (the frame, the static files and the
//! error pages, mounted through `pages::router`) and `entries`, which serves `/r/<id>`; `api` is
//! the documented v1 API (spec v2-api), mounted at `/api/v1`; `evidence` turns a `netray-model`
//! evidence block into the view `evidence_block` renders, and `explore` derives its explore-live
//! links. `admission` runs domains (one run per domain, throttle, budget, slots, limits),
//! `badge` and `og` draw the badge and the OG card, and `host` composes the apex and the
//! inspector hosts for `netray serve` (`host::build`, `host::layers`, `host::probes`).

pub mod admission;
pub mod api;
pub mod badge;
mod client;
pub mod components;
pub mod config;
pub mod contrast;
pub mod entries;
pub mod error;
pub mod evidence;
pub mod explore;
pub mod gate;
pub mod host;
pub mod metrics;
pub mod og;
pub mod pages;
pub mod plate;
pub mod spectrum;
pub mod stream;
pub mod tokens;
