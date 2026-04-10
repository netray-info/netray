use utoipa::OpenApi;

use crate::routes::{
    CheckItem, CheckPostBody, DnsEvent, DoneEvent, HealthResponse, IpAddressInfo, IpEvent,
    MetaEcosystem, MetaResponse, ProfileData, RateLimitInfo, ReadyResponse, SummaryEvent,
    SyncCheckResponse, TlsEvent,
};

#[derive(OpenApi)]
#[openapi(
    info(title = "lens", version = env!("CARGO_PKG_VERSION")),
    components(schemas(
        CheckItem,
        IpAddressInfo,
        DnsEvent,
        TlsEvent,
        IpEvent,
        SummaryEvent,
        DoneEvent,
        SyncCheckResponse,
        CheckPostBody,
        HealthResponse,
        ReadyResponse,
        MetaResponse,
        MetaEcosystem,
        ProfileData,
        RateLimitInfo,
    ))
)]
pub struct ApiDoc;

pub fn build_openapi(
    health_api: utoipa::openapi::OpenApi,
    api_api: utoipa::openapi::OpenApi,
) -> utoipa::openapi::OpenApi {
    let mut doc = ApiDoc::openapi();
    doc.merge(health_api);
    doc.merge(api_api);
    doc
}
