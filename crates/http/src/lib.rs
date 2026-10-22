// The HTTP inspector as a library: its `Module` implementation and the inspection netray-web
// runs.

pub mod config;
pub mod error;
pub mod input;
pub mod inspect;
pub mod job;
pub mod module;
pub mod quality;
pub mod security;
#[cfg(feature = "testing")]
pub mod testing;

pub use config::ModuleConfig;
pub use job::{HttpInspect, HttpInspectRequest, HttpRefusal};
pub use module::{HttpModule, translate};
