//! Transport comparison endpoint: query over all 4 transports and compare answers.
//!
//! - `POST /api/compare` — accept a structured JSON body.
//!
//! For each transport (UDP, TCP, TLS, HTTPS), builds a resolver group and fans out
//! per-record-type lookups. Each batch event includes the transport that produced it.
//! Providers that don't support a transport are skipped with a warning.
