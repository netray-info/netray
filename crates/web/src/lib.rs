//! netray-web: the V2 view layer (SDD `specs/sdd/v2.md` §8). It holds the tokens, the
//! components, the gate, and the pages every service shares: the frame, the static files and the
//! error pages, mounted through `pages::router`.

pub mod components;
pub mod contrast;
pub mod gate;
pub mod pages;
pub mod spectrum;
pub mod stream;
pub mod tokens;
