//! netray-web: the V2 view layer (SDD `specs/sdd/v2.md` §8). Phase 4a holds the tokens, the
//! components and the gate; `entries` also serves `/r/<id>`, which lens mounts.

pub mod components;
pub mod contrast;
pub mod entries;
pub mod gate;
pub mod spectrum;
pub mod tokens;
