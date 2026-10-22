use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use netray_common::fetch::{ClientSettings, Resolve};
use netray_common::target_policy::is_allowed_target;

/// Settings for the MTA-STS and BIMI fetches.
#[derive(Clone)]
pub struct OutboundFetch {
    pub settings: ClientSettings,
    pub resolver: Arc<dyn Resolve>,
    pub allow: fn(IpAddr) -> bool,
    pub timeout: Duration,
    /// The port the MTA-STS policy fetch connects to; `None` is HTTPS's 443.
    pub mta_sts_port: Option<u16>,
}

impl OutboundFetch {
    pub fn new(timeout_ms: u64, resolver: Arc<dyn Resolve>) -> Self {
        Self {
            settings: ClientSettings {
                user_agent: Some(format!(
                    "beacon/{} (netray.info)",
                    env!("CARGO_PKG_VERSION")
                )),
                ..ClientSettings::default()
            },
            resolver,
            allow: is_allowed_target,
            timeout: Duration::from_millis(timeout_ms),
            mta_sts_port: None,
        }
    }

    pub fn with_mta_sts_port(self, port: u16) -> Self {
        Self {
            mta_sts_port: Some(port),
            ..self
        }
    }
}
