//! Schema-only mirrors of mhost's lookup serialisation for the OpenAPI document.
//!
//! mhost's `Lookups` and `RData` derive no `ToSchema`, and the authoritative branch of
//! authcompare synthesises the same shape with string rdata. These types are never
//! serialised; they only describe the JSON the batch events carry.

use std::collections::BTreeMap;

use utoipa::openapi::schema::{AnyOfBuilder, ArrayBuilder, Object, ObjectBuilder, Schema, Type};
use utoipa::openapi::{Ref, RefOr};
use utoipa::{PartialSchema, ToSchema};

/// All lookups of one batch.
#[derive(ToSchema)]
pub struct DnsLookups {
    pub lookups: Vec<DnsLookup>,
}

/// One server's answer to one query.
#[derive(ToSchema)]
pub struct DnsLookup {
    pub query: DnsQuery,
    /// The server, e.g. `udp:192.0.2.1:53`.
    pub name_server: String,
    pub result: DnsLookupResult,
}

#[derive(ToSchema)]
pub struct DnsQuery {
    pub name: String,
    pub record_type: String,
}

/// A response, an NXDOMAIN or NODATA answer, or an error, as a single-key object.
#[derive(ToSchema)]
pub enum DnsLookupResult {
    Response(DnsResponse),
    NxDomain(DnsNxDomain),
    Error(DnsLookupError),
}

#[derive(ToSchema)]
pub struct DnsResponse {
    pub records: Vec<DnsLookupRecord>,
    pub response_time: DnsDuration,
    pub valid_until: Option<String>,
}

#[derive(ToSchema)]
pub struct DnsNxDomain {
    pub response_time: DnsDuration,
    pub response_code: Option<DnsResponseCode>,
    pub referral: Option<bool>,
}

#[derive(ToSchema)]
pub struct DnsDuration {
    pub secs: u64,
    pub nanos: u32,
}

#[derive(ToSchema)]
pub struct DnsLookupRecord {
    pub name: String,
    #[schema(rename = "type")]
    pub record_type: String,
    pub ttl: u32,
    /// The record data by type name: a string for A, AAAA, NS, CNAME, PTR and every
    /// authoritative record, an object of the type's fields for the others.
    pub data: BTreeMap<String, DnsRData>,
}

fn primitive(t: Type) -> Object {
    ObjectBuilder::new().schema_type(t).build()
}

fn any_of(items: Vec<RefOr<Schema>>) -> RefOr<Schema> {
    items
        .into_iter()
        .fold(AnyOfBuilder::new(), |b, item| b.item(item))
        .into()
}

fn single_key(key: &str, inner: impl Into<RefOr<Schema>>) -> Object {
    ObjectBuilder::new()
        .schema_type(Type::Object)
        .property(key, inner)
        .required(key)
        .build()
}

/// The response code of a negative answer: its name or its number.
pub struct DnsResponseCode;

impl PartialSchema for DnsResponseCode {
    fn schema() -> RefOr<Schema> {
        any_of(vec![
            primitive(Type::String).into(),
            primitive(Type::Integer).into(),
        ])
    }
}

impl ToSchema for DnsResponseCode {}

/// A failed lookup: a status word, or `ResolveError` / `ProtoError` with a reason.
pub struct DnsLookupError;

impl PartialSchema for DnsLookupError {
    fn schema() -> RefOr<Schema> {
        let reason = || {
            ObjectBuilder::new()
                .schema_type(Type::Object)
                .property("reason", primitive(Type::String))
                .required("reason")
        };
        any_of(vec![
            ObjectBuilder::new()
                .schema_type(Type::String)
                .enum_values(Some([
                    "QueryRefused",
                    "ServerFailure",
                    "Timeout",
                    "NoRecordsFound",
                    "CancelledError",
                    "RuntimePanicError",
                ]))
                .build()
                .into(),
            single_key("ResolveError", reason()).into(),
            single_key("ProtoError", reason()).into(),
        ])
    }
}

impl ToSchema for DnsLookupError {}

/// One record's data: a string, or an object of fields.
pub struct DnsRData;

impl PartialSchema for DnsRData {
    fn schema() -> RefOr<Schema> {
        any_of(vec![
            primitive(Type::String).into(),
            ObjectBuilder::new()
                .schema_type(Type::Object)
                .additional_properties(Some(Ref::from_schema_name(DnsRDataField::name())))
                .build()
                .into(),
        ])
    }
}

impl ToSchema for DnsRData {
    fn schemas(schemas: &mut Vec<(String, RefOr<Schema>)>) {
        schemas.push((DnsRDataField::name().into(), DnsRDataField::schema()));
        <DnsRDataField as ToSchema>::schemas(schemas);
    }
}

/// A field of structured record data: any JSON value.
pub struct DnsRDataField;

impl PartialSchema for DnsRDataField {
    fn schema() -> RefOr<Schema> {
        let field = || Ref::from_schema_name(DnsRDataField::name());
        any_of(vec![
            primitive(Type::String).into(),
            primitive(Type::Integer).into(),
            primitive(Type::Number).into(),
            primitive(Type::Boolean).into(),
            primitive(Type::Null).into(),
            ArrayBuilder::new().items(field()).build().into(),
            ObjectBuilder::new()
                .schema_type(Type::Object)
                .additional_properties(Some(field()))
                .build()
                .into(),
        ])
    }
}

impl ToSchema for DnsRDataField {}
