//! The retired V1 surface (spec v2-cutover, requirements 13 and 14; SDD §7.2 and §7.3): the
//! redirects and the `410` answers per host. Every `Location` and every successor is absolute
//! on the apex.

use std::collections::HashMap;

use axum::Json;
use axum::extract::Query;
use axum::http::header::{ALLOW, LOCATION};
use axum::http::{HeaderValue, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use netray_common::error::{ErrorInfo, ErrorResponse};

use crate::pages::domain_path;

/// One request as the legacy tables see it.
pub(super) struct Legacy<'a> {
    pub apex: &'a str,
    pub method: &'a Method,
    pub uri: &'a Uri,
}

const IP_FIELDS: [&str; 13] = [
    "tcp",
    "location",
    "user_agent",
    "network",
    "host",
    "isp",
    "country",
    "city",
    "timezone",
    "latitude",
    "longitude",
    "region",
    "headers",
];

fn gone(successor: &str) -> Response {
    let body = ErrorResponse {
        error: ErrorInfo {
            code: "gone",
            message: format!("This endpoint has been removed. Use {successor}."),
        },
    };
    (StatusCode::GONE, Json(body)).into_response()
}

fn under(path: &str, prefix: &str) -> bool {
    path == prefix
        || path
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with('/'))
}

impl Legacy<'_> {
    fn path(&self) -> &str {
        self.uri.path()
    }

    fn url(&self, path: &str) -> String {
        format!("https://{}{path}", self.apex)
    }

    /// 301 to `https://<apex><target>` for GET and HEAD, 405 for any other method.
    fn moved(&self, target: &str) -> Response {
        if matches!(*self.method, Method::GET | Method::HEAD) {
            let location = self.url(target);
            match HeaderValue::try_from(location) {
                Ok(value) => (StatusCode::MOVED_PERMANENTLY, [(LOCATION, value)]).into_response(),
                Err(_) => StatusCode::BAD_REQUEST.into_response(),
            }
        } else {
            (
                StatusCode::METHOD_NOT_ALLOWED,
                [(ALLOW, HeaderValue::from_static("GET, HEAD"))],
            )
                .into_response()
        }
    }

    fn gone(&self, successor: &str) -> Response {
        gone(&self.url(successor))
    }

    /// 13a: `lens.<apex>` to the same path and query on the apex.
    pub fn lens_host(&self) -> Response {
        let target = self.uri.path_and_query().map_or("/", |pq| pq.as_str());
        self.moved(target)
    }

    /// 13f and 13h on `dns.`, `tls.`, `http.`, `email.` and `ip.`.
    pub fn subdomain_common(&self) -> Option<Response> {
        let path = self.path();
        match path {
            _ if under(path, "/api") => Some(self.gone("/api/v1")),
            "/docs" | "/docs/" => Some(self.moved("/api/v1/docs")),
            "/api-docs/openapi.json" => Some(self.gone("/api/v1/openapi.json")),
            _ => None,
        }
    }

    /// 13g, f, h, then c, d and e on `email.`.
    pub fn email_host(&self) -> Response {
        let path = self.path();
        if under(path, "/inspect") {
            return self.gone("/api/v1/email/inspect");
        }
        if let Some(response) = self.subdomain_common() {
            return response;
        }
        if path == "/" {
            let params = Query::<HashMap<String, String>>::try_from_uri(self.uri)
                .map(|Query(params)| params)
                .unwrap_or_default();
            let token = params
                .get("q")
                .and_then(|q| q.split_whitespace().next())
                .or_else(|| params.get("domain").map(String::as_str));
            if let Some(domain) = token.and_then(domain_path) {
                return self.moved(&format!("{domain}/email"));
            }
        }
        self.moved("/")
    }

    /// Requirement 14: the cut routes of `ip.`.
    pub fn ip_successor(&self) -> Option<Response> {
        let path = self.path();
        match path {
            "/api/meta" | "/meta" => return Some(self.gone("/api/v1/meta")),
            "/api-docs/openapi.json" => return Some(self.gone("/api/v1/openapi.json")),
            _ => {}
        }
        let mut segments = path.strip_prefix('/')?.split('/');
        let first = segments.next()?;
        let second = segments.next();
        if segments.next().is_some() {
            return None;
        }
        let successor = match (first, second) {
            ("json" | "yaml" | "toml" | "csv", None) => "/api/v1/ip".to_string(),
            ("all" | "ip", _) if second != Some("cidr") => "/api/v1/ip".to_string(),
            ("ip", Some("cidr")) | ("range", None) => "/api/v1/ip/range/{cidr}".to_string(),
            ("asn", _) => "/api/v1/asn/{n}".to_string(),
            ("batch", _) => "/api/v1/ip/batch".to_string(),
            ("diff", None) => "/api/v1/ip/diff".to_string(),
            ("ipv4" | "ipv6", _) => {
                let flag = &first[3..];
                return Some(gone(&format!(
                    "{}, or `curl -{flag} https://ip.{}/` for the caller's own address",
                    self.url("/api/v1/ip/{address}"),
                    self.apex
                )));
            }
            (field, _) if IP_FIELDS.contains(&field) => format!("/api/v1/ip?field={field}"),
            _ => return None,
        };
        Some(self.gone(&successor))
    }

    /// Rows m, i, o, l and n of 13, in that order, on the apex.
    pub fn apex_legacy(&self) -> Option<Response> {
        let path = self.path();
        if matches!(
            path,
            "/api"
                | "/api/"
                | "/api/index"
                | "/api/dns"
                | "/api/tls"
                | "/api/http"
                | "/api/email"
                | "/api/ip"
                | "/api/lens"
        ) || path.strip_prefix("/api/").is_some_and(|name| {
            name.strip_suffix(".html").is_some_and(|stem| {
                ["index", "dns", "tls", "http", "email", "ip", "lens"].contains(&stem)
            })
        }) {
            return Some(self.moved("/api/v1/docs"));
        }
        if under(path, "/api/check") {
            return Some(self.gone("/api/v1/domains/{domain}/runs"));
        }
        if let Some(rest) = path.strip_prefix("/api/")
            && rest.split('/').next() != Some("v1")
        {
            return Some(self.gone("/api/v1"));
        }
        match path {
            "/docs" | "/docs/" => Some(self.moved("/api/v1/docs")),
            "/api-docs/openapi.json" => Some(self.gone("/api/v1/openapi.json")),
            "/tools" | "/tools/" => Some(self.moved("/")),
            "/guide/lens" => Some(self.moved("/guide/scoring")),
            _ => None,
        }
    }
}
