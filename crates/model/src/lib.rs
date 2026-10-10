//! The V2 check vocabulary: protocols, check ids, statuses, grades and results.
//!
//! Pure data types with no I/O and no dependency on any other workspace crate.

use std::fmt;

use serde::{Serialize, Serializer};

/// The protocol a check belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    Dns,
    Tls,
    Http,
    Email,
    Ip,
}

impl Protocol {
    fn as_str(self) -> &'static str {
        match self {
            Protocol::Dns => "dns",
            Protocol::Tls => "tls",
            Protocol::Http => "http",
            Protocol::Email => "email",
            Protocol::Ip => "ip",
        }
    }

    fn from_prefix(s: &str) -> Option<Self> {
        match s {
            "dns" => Some(Protocol::Dns),
            "tls" => Some(Protocol::Tls),
            "http" => Some(Protocol::Http),
            "email" => Some(Protocol::Email),
            "ip" => Some(Protocol::Ip),
            _ => None,
        }
    }
}

/// A check identifier of the form `<protocol>.<name>`, e.g. `tls.chain_trusted`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckId {
    protocol: Protocol,
    name: String,
}

impl CheckId {
    /// Parses `<protocol>.<name>`; refuses a missing dot, an unknown protocol or a name that is
    /// not `[a-z][a-z0-9_]*`.
    pub fn parse(s: &str) -> Result<CheckId, CheckIdError> {
        let (prefix, name) = s
            .split_once('.')
            .ok_or_else(|| CheckIdError(format!("check id {s:?} has no '.' separator")))?;
        let protocol = Protocol::from_prefix(prefix).ok_or_else(|| {
            CheckIdError(format!("check id {s:?} has unknown protocol {prefix:?}"))
        })?;
        let mut chars = name.chars();
        let valid = chars.next().is_some_and(|c| c.is_ascii_lowercase())
            && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        if !valid {
            return Err(CheckIdError(format!(
                "check id {s:?} needs a name matching [a-z][a-z0-9_]*"
            )));
        }
        Ok(CheckId {
            protocol,
            name: name.to_string(),
        })
    }

    pub fn protocol(&self) -> Protocol {
        self.protocol
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

impl fmt::Display for CheckId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.protocol.as_str(), self.name)
    }
}

impl Serialize for CheckId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

/// Why a string is not a valid [`CheckId`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckIdError(String);

impl fmt::Display for CheckIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CheckIdError {}

/// The outcome of one check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Pass,
    Warn,
    Fail,
    NotApplicable,
    NotTested,
    Unmeasured,
}

/// How serious a failed check is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Critical,
    High,
    Medium,
    Low,
}

/// Who can fix a failed check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FixOwner {
    DnsProvider,
    Registrar,
    CertificateProvider,
    WebServer,
    MailProvider,
    Hosting,
}

/// The overall grade of a scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Grade {
    #[serde(rename = "A+")]
    APlus,
    A,
    B,
    C,
    D,
    F,
    #[serde(rename = "incomplete")]
    Incomplete,
}

/// One check's id, status and supporting text.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CheckResult {
    pub id: CheckId,
    pub status: Status,
    pub findings: Vec<String>,
    pub evidence: Vec<String>,
}
