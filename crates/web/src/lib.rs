//! netray-web: the V2 view layer (SDD `specs/sdd/v2.md` §8). Phase 4a holds the tokens, the
//! components and the gate; `entries` also serves `/r/<id>`, which lens mounts; `api` is the
//! documented v1 API (spec v2-api), mounted by lens at `/api/v1`; `evidence` turns a
//! `netray-model` evidence block into the view `evidence_block` renders.

pub mod api;
pub mod components;
pub mod contrast;
pub mod entries;
pub mod evidence;
pub mod explore;
pub mod gate;
pub mod spectrum;
pub mod tokens;
