//! netray-web: the V2 view layer (SDD `specs/sdd/v2.md` §8). It holds the tokens, the
//! components, the gate, the pages every service shares (the frame, the static files and the
//! error pages, mounted through `pages::router`) and `entries`, which serves `/r/<id>`; `api` is
//! the documented v1 API (spec v2-api), mounted by lens at `/api/v1`; `evidence` turns a
//! `netray-model` evidence block into the view `evidence_block` renders, and `explore` derives
//! its explore-live links.

pub mod api;
mod client;
pub mod compact;
pub mod components;
pub mod contrast;
pub mod entries;
pub mod evidence;
pub mod explore;
pub mod export;
pub mod gate;
pub mod grade_legend;
pub mod pages;
pub mod plate;
pub mod selectors;
pub mod spectrum;
pub mod stream;
pub mod tokens;
