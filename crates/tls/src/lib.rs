// The TLS inspector as a library: its `Module` implementation, the inspection netray-web runs
// and its response types. The modules are `pub` for the integration tests in `tests/`.

pub mod backend;
pub mod config;
pub mod dns;
pub mod error;
pub mod input;
pub mod inspection;
pub mod inspector;
pub mod module;
pub mod quality;
pub mod security;
pub mod state;
#[cfg(feature = "testing")]
pub mod testing;
pub mod tls;
pub mod validate;

pub use backend::TlsBackend;
pub use config::ModuleConfig;
pub use inspector::{TlsInspectRequest, TlsInspector, TlsRefusal};
pub use module::{TlsModule, translate};
