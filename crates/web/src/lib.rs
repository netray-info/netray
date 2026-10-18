//! netray-web: the V2 view layer (SDD `specs/sdd/v2.md` §8). It holds the tokens, the
//! components, the gate, the pages every service shares (the frame, the static files and the
//! error pages, mounted through `pages::router`), `entries`, which serves `/r/<id>`, and `api`,
//! the documented v1 API (spec v2-api), mounted by lens at `/api/v1`.

pub mod api;
pub mod components;
pub mod contrast;
pub mod entries;
pub mod gate;
pub mod host;
pub mod pages;
pub mod spectrum;
pub mod stream;
pub mod tokens;
