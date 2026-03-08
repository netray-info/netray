pub mod caa;
pub mod tlsa;

use std::sync::Arc;
use std::time::Duration;

use mhost::resolver::{ResolverGroup, ResolverGroupBuilder};

pub use caa::CaaLookup;
pub use tlsa::{TlsaLookup, TlsaRecord};

pub struct DnsResolver {
    resolvers: Arc<ResolverGroup>,
}

impl DnsResolver {
    pub async fn new(timeout_secs: u64) -> Result<Self, mhost::Error> {
        let resolvers = ResolverGroupBuilder::new()
            .system()
            .timeout(Duration::from_secs(timeout_secs))
            .build()
            .await?;
        Ok(Self {
            resolvers: Arc::new(resolvers),
        })
    }

    pub async fn lookup_caa(&self, hostname: &str) -> CaaLookup {
        caa::lookup_caa(&self.resolvers, hostname).await
    }

    pub async fn lookup_tlsa(&self, hostname: &str, port: u16) -> TlsaLookup {
        tlsa::lookup_tlsa(&self.resolvers, hostname, port).await
    }
}
