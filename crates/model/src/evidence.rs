//! The evidence vocabulary: blocks of six shapes holding typed values under registered keys.

use std::collections::HashSet;
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{CheckResult, Status};

/// The version of the block, shape and value vocabulary.
pub const EVIDENCE_SCHEMA_VERSION: u32 = 1;

/// A block identifier of the form `<module>.<name>[@<qualifier>]`, e.g. `tls.chain@192.0.2.1:443`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockId {
    module: String,
    name: String,
    qualifier: Option<String>,
}

fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// A decimal port in 1..=65535 without sign or leading zero.
fn port(s: &str) -> Option<u16> {
    if s.starts_with('0') || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse::<u16>().ok().filter(|p| *p != 0)
}

/// The canonical form of a qualifier, or `None` when it is not one.
fn canonical_qualifier(q: &str) -> Option<String> {
    if let Some(rest) = q.strip_prefix('[') {
        let (addr, tail) = rest.split_once(']')?;
        let addr: Ipv6Addr = addr.parse().ok()?;
        return match tail {
            "" => Some(format!("[{addr}]")),
            _ => Some(format!("[{addr}]:{}", port(tail.strip_prefix(':')?)?)),
        };
    }
    if let Ok(addr) = q.parse::<Ipv4Addr>() {
        return Some(addr.to_string());
    }
    if let Some(p) = port(q) {
        return Some(p.to_string());
    }
    let (addr, p) = q.split_once(':')?;
    Some(format!("{}:{}", addr.parse::<Ipv4Addr>().ok()?, port(p)?))
}

impl BlockId {
    /// Parses `<module>.<name>[@<qualifier>]`; module and name match `[a-z][a-z0-9_]*`, the
    /// qualifier is an IPv4 address, `<ipv4>:<port>`, `[<ipv6>]`, `[<ipv6>]:<port>` or a port,
    /// and is kept in canonical form.
    pub fn parse(s: &str) -> Result<BlockId, BlockIdError> {
        let (base, qualifier) = match s.split_once('@') {
            Some((base, q)) => (base, Some(q)),
            None => (s, None),
        };
        let (module, name) = base
            .split_once('.')
            .ok_or_else(|| BlockIdError(format!("block id {s:?} has no '.' separator")))?;
        if !is_ident(module) || !is_ident(name) {
            return Err(BlockIdError(format!(
                "block id {s:?} needs module and name matching [a-z][a-z0-9_]*"
            )));
        }
        let qualifier = qualifier
            .map(|q| {
                canonical_qualifier(q).ok_or_else(|| {
                    BlockIdError(format!(
                        "block id {s:?} has qualifier {q:?}, not an address, address:port or port"
                    ))
                })
            })
            .transpose()?;
        Ok(BlockId {
            module: module.to_string(),
            name: name.to_string(),
            qualifier,
        })
    }

    pub fn module(&self) -> &str {
        &self.module
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn qualifier(&self) -> Option<&str> {
        self.qualifier.as_deref()
    }

    fn is_block_name(&self, block_name: &str) -> bool {
        block_name.split_once('.') == Some((self.module.as_str(), self.name.as_str()))
    }
}

impl fmt::Display for BlockId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.module, self.name)?;
        if let Some(q) = &self.qualifier {
            write!(f, "@{q}")?;
        }
        Ok(())
    }
}

impl utoipa::PartialSchema for BlockId {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        utoipa::openapi::ObjectBuilder::new()
            .schema_type(utoipa::openapi::schema::Type::String)
            .examples(["tls.chain@192.0.2.1:443"])
            .into()
    }
}

impl utoipa::ToSchema for BlockId {}

impl Serialize for BlockId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for BlockId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        BlockId::parse(&s).map_err(serde::de::Error::custom)
    }
}

/// Why a string is not a valid [`BlockId`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockIdError(String);

impl fmt::Display for BlockIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for BlockIdError {}

/// A typed value, tagged on the wire as `{"t": "<type>", "v": <value>}`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "t", content = "v", rename_all = "snake_case")]
pub enum Value {
    Text(String),
    Int(i64),
    Bool(bool),
    /// An RFC 3339 timestamp.
    Time(String),
    /// Milliseconds.
    Duration(u64),
    Addr(IpAddr),
}

impl Value {
    pub fn value_type(&self) -> ValueType {
        match self {
            Value::Text(_) => ValueType::Text,
            Value::Int(_) => ValueType::Int,
            Value::Bool(_) => ValueType::Bool,
            Value::Time(_) => ValueType::Time,
            Value::Duration(_) => ValueType::Duration,
            Value::Addr(_) => ValueType::Addr,
        }
    }
}

fn digits(s: &[u8]) -> Option<u32> {
    if s.is_empty() || !s.iter().all(u8::is_ascii_digit) {
        return None;
    }
    Some(s.iter().fold(0, |n, d| n * 10 + u32::from(d - b'0')))
}

/// Whether `s` is an RFC 3339 `date-time`: `YYYY-MM-DDTHH:MM:SS[.frac](Z|+HH:MM|-HH:MM)`.
fn is_rfc3339(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() < 20
        || b[4] != b'-'
        || b[7] != b'-'
        || !matches!(b[10], b'T' | b't')
        || b[13] != b':'
        || b[16] != b':'
    {
        return false;
    }
    let (Some(year), Some(month), Some(day), Some(hour), Some(minute), Some(second)) = (
        digits(&b[0..4]),
        digits(&b[5..7]),
        digits(&b[8..10]),
        digits(&b[11..13]),
        digits(&b[14..16]),
        digits(&b[17..19]),
    ) else {
        return false;
    };
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    if day == 0 || day > days || hour > 23 || minute > 59 || second > 60 {
        return false;
    }
    let mut rest = &b[19..];
    if let Some(frac) = rest.strip_prefix(b".") {
        let n = frac.iter().take_while(|c| c.is_ascii_digit()).count();
        if n == 0 {
            return false;
        }
        rest = &frac[n..];
    }
    match rest {
        [b'Z' | b'z'] => true,
        [b'+' | b'-', h1, h2, b':', m1, m2] => {
            matches!((digits(&[*h1, *h2]), digits(&[*m1, *m2])), (Some(h), Some(m)) if h <= 23 && m <= 59)
        }
        _ => false,
    }
}

/// The type of a [`Value`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueType {
    Text,
    Int,
    Bool,
    Time,
    Duration,
    Addr,
}

impl fmt::Display for ValueType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ValueType::Text => "text",
            ValueType::Int => "int",
            ValueType::Bool => "bool",
            ValueType::Time => "time",
            ValueType::Duration => "duration",
            ValueType::Addr => "addr",
        })
    }
}

pub type Key = String;
pub type Fields = Vec<(Key, Value)>;

/// A table column, named by its registered key.
pub type Column = Key;
/// A table row, one value per column in column order.
pub type Row = Vec<Value>;

/// A DNS resource record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Record {
    #[schema(value_type = ValueSchema)]
    pub owner: Value,
    #[serde(rename = "type")]
    #[schema(value_type = ValueSchema)]
    pub rtype: Value,
    #[schema(value_type = ValueSchema)]
    pub ttl: Value,
    #[schema(value_type = ValueSchema)]
    pub data: Value,
}

/// A node of a `tree` block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Node {
    #[schema(value_type = Vec<FieldPair>)]
    pub fields: Fields,
    #[schema(no_recursion)]
    pub children: Vec<Node>,
}

/// The six shapes a block can take.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum Shape {
    Records(Vec<Record>),
    Table {
        /// The column whose value identifies a row.
        key: Column,
        columns: Vec<Column>,
        rows: Vec<Row>,
    },
    Chain(Vec<Fields>),
    Tree(Node),
    Fields(Fields),
    Text(String),
}

impl Shape {
    pub fn kind(&self) -> ShapeKind {
        match self {
            Shape::Records(_) => ShapeKind::Records,
            Shape::Table { .. } => ShapeKind::Table,
            Shape::Chain(_) => ShapeKind::Chain,
            Shape::Tree(_) => ShapeKind::Tree,
            Shape::Fields(_) => ShapeKind::Fields,
            Shape::Text(_) => ShapeKind::Text,
        }
    }
}

/// The schema of a [`Value`]: the tagged union as it serialises. An address is its text form.
#[derive(utoipa::ToSchema)]
#[serde(tag = "t", content = "v", rename_all = "snake_case")]
#[schema(as = Value)]
#[allow(dead_code)]
enum ValueSchema {
    Text(String),
    Int(i64),
    Bool(bool),
    Time(String),
    Duration(u64),
    Addr(String),
}

impl utoipa::PartialSchema for Value {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        <ValueSchema as utoipa::PartialSchema>::schema()
    }
}

impl utoipa::ToSchema for Value {
    fn name() -> std::borrow::Cow<'static, str> {
        "Value".into()
    }

    fn schemas(
        schemas: &mut Vec<(
            String,
            utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>,
        )>,
    ) {
        <ValueSchema as utoipa::ToSchema>::schemas(schemas);
    }
}

/// One `[key, value]` entry of [`Fields`], as the tuple serialises.
struct FieldPair;

impl utoipa::PartialSchema for FieldPair {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        use utoipa::openapi::RefOr;
        use utoipa::openapi::schema::{ArrayBuilder, ObjectBuilder, Schema, Type};
        let key = Schema::Object(ObjectBuilder::new().schema_type(Type::String).build());
        let RefOr::T(value) = <Value as utoipa::PartialSchema>::schema() else {
            unreachable!("a derived schema is inline")
        };
        ArrayBuilder::new()
            .items(utoipa::openapi::schema::ArrayItems::False)
            .prefix_items([key, value])
            .min_items(Some(2))
            .max_items(Some(2))
            .into()
    }
}

impl utoipa::ToSchema for FieldPair {}

/// The schema of a [`Shape`], the adjacently tagged union as it serialises.
#[derive(utoipa::ToSchema)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
#[allow(dead_code)]
enum ShapeSchema {
    Records(Vec<Record>),
    Table {
        key: String,
        columns: Vec<String>,
        rows: Vec<Vec<ValueSchema>>,
    },
    Chain(Vec<Vec<FieldPair>>),
    Tree(Node),
    Fields(Vec<FieldPair>),
    Text(String),
}

impl utoipa::PartialSchema for Shape {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        <ShapeSchema as utoipa::PartialSchema>::schema()
    }
}

impl utoipa::ToSchema for Shape {
    fn name() -> std::borrow::Cow<'static, str> {
        "Shape".into()
    }

    fn schemas(
        schemas: &mut Vec<(
            String,
            utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>,
        )>,
    ) {
        <ShapeSchema as utoipa::ToSchema>::schemas(schemas);
    }
}

/// The kind of a [`Shape`], as a register declares it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShapeKind {
    Records,
    Table,
    Chain,
    Tree,
    Fields,
    Text,
}

/// One block of evidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Block {
    pub id: BlockId,
    pub title: String,
    pub shape: Shape,
}

/// A module's register: its block names with their shape, and the keys each block name uses
/// with their value type, e.g. `("dns.caa", "data", ValueType::Text)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Register {
    pub blocks: &'static [(&'static str, ShapeKind)],
    pub keys: &'static [(&'static str, &'static str, ValueType)],
}

/// Why a block does not conform to its module's register.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceError {
    ForeignModule {
        module: String,
        block: BlockId,
    },
    UnregisteredBlock {
        block: BlockId,
    },
    DuplicateBlock {
        block: BlockId,
    },
    ShapeMismatch {
        block: BlockId,
        registered: ShapeKind,
        found: ShapeKind,
    },
    UnregisteredKey {
        block: BlockId,
        key: String,
    },
    TypeMismatch {
        block: BlockId,
        key: String,
        registered: ValueType,
        found: ValueType,
    },
    InvalidTime {
        block: BlockId,
        key: String,
        value: String,
    },
    KeyColumnMissing {
        block: BlockId,
        key: String,
    },
    DuplicateRowKey {
        block: BlockId,
        key: String,
        row: usize,
    },
    RowWidth {
        block: BlockId,
        row: usize,
        columns: usize,
        values: usize,
    },
}

impl fmt::Display for EvidenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EvidenceError::ForeignModule { module, block } => {
                write!(f, "block {block} does not belong to module {module}")
            }
            EvidenceError::UnregisteredBlock { block } => {
                write!(f, "block {block} has no registered name")
            }
            EvidenceError::DuplicateBlock { block } => {
                write!(f, "block {block} appears more than once")
            }
            EvidenceError::ShapeMismatch {
                block,
                registered,
                found,
            } => write!(
                f,
                "block {block} is registered as {registered:?}, found {found:?}"
            ),
            EvidenceError::UnregisteredKey { block, key } => {
                write!(f, "block {block} uses unregistered key {key:?}")
            }
            EvidenceError::TypeMismatch {
                block,
                key,
                registered,
                found,
            } => write!(
                f,
                "block {block} key {key:?} is registered as {registered}, found {found}"
            ),
            EvidenceError::InvalidTime { block, key, value } => {
                write!(f, "block {block} key {key:?} holds {value:?}, not RFC 3339")
            }
            EvidenceError::KeyColumnMissing { block, key } => {
                write!(
                    f,
                    "block {block} key column {key:?} is not among its columns"
                )
            }
            EvidenceError::DuplicateRowKey { block, key, row } => {
                write!(
                    f,
                    "block {block} row {row} repeats a value of key column {key:?}"
                )
            }
            EvidenceError::RowWidth {
                block,
                row,
                columns,
                values,
            } => write!(
                f,
                "block {block} row {row} has {values} values for {columns} columns"
            ),
        }
    }
}

impl std::error::Error for EvidenceError {}

struct Checker<'a> {
    register: &'a Register,
    block: &'a BlockId,
    errors: Vec<EvidenceError>,
}

impl Checker<'_> {
    fn registered(&self, key: &str) -> Option<ValueType> {
        self.register
            .keys
            .iter()
            .find(|(name, k, _)| self.block.is_block_name(name) && *k == key)
            .map(|(_, _, ty)| *ty)
    }

    fn value(&mut self, key: &str, registered: ValueType, value: &Value) {
        let found = value.value_type();
        if found != registered {
            self.errors.push(EvidenceError::TypeMismatch {
                block: self.block.clone(),
                key: key.to_string(),
                registered,
                found,
            });
        } else if let Value::Time(t) = value
            && !is_rfc3339(t)
        {
            self.errors.push(EvidenceError::InvalidTime {
                block: self.block.clone(),
                key: key.to_string(),
                value: t.clone(),
            });
        }
    }

    fn key(&mut self, key: &str, value: &Value) {
        match self.registered(key) {
            Some(registered) => self.value(key, registered, value),
            None => self.errors.push(EvidenceError::UnregisteredKey {
                block: self.block.clone(),
                key: key.to_string(),
            }),
        }
    }

    fn fields(&mut self, fields: &Fields) {
        for (key, value) in fields {
            self.key(key, value);
        }
    }

    fn node(&mut self, node: &Node) {
        self.fields(&node.fields);
        for child in &node.children {
            self.node(child);
        }
    }

    fn table(&mut self, key: &Column, columns: &[Column], rows: &[Row]) {
        let key_index = columns.iter().position(|c| c == key);
        if key_index.is_none() {
            self.errors.push(EvidenceError::KeyColumnMissing {
                block: self.block.clone(),
                key: key.clone(),
            });
        }
        let mut row_keys = HashSet::new();
        for (i, row) in rows.iter().enumerate() {
            if row.len() != columns.len() {
                self.errors.push(EvidenceError::RowWidth {
                    block: self.block.clone(),
                    row: i,
                    columns: columns.len(),
                    values: row.len(),
                });
            }
            if let Some(v) = key_index.and_then(|k| row.get(k))
                && !row_keys.insert(v)
            {
                self.errors.push(EvidenceError::DuplicateRowKey {
                    block: self.block.clone(),
                    key: key.clone(),
                    row: i,
                });
            }
        }
        for (c, column) in columns.iter().enumerate() {
            let Some(registered) = self.registered(column) else {
                self.errors.push(EvidenceError::UnregisteredKey {
                    block: self.block.clone(),
                    key: column.clone(),
                });
                continue;
            };
            for value in rows.iter().filter_map(|row| row.get(c)) {
                self.value(column, registered, value);
            }
        }
    }

    fn shape(&mut self, shape: &Shape) {
        match shape {
            Shape::Records(records) => {
                for r in records {
                    self.key("owner", &r.owner);
                    self.key("type", &r.rtype);
                    self.key("ttl", &r.ttl);
                    self.key("data", &r.data);
                }
            }
            Shape::Table { key, columns, rows } => self.table(key, columns, rows),
            Shape::Chain(items) => items.iter().for_each(|item| self.fields(item)),
            Shape::Tree(node) => self.node(node),
            Shape::Fields(fields) => self.fields(fields),
            Shape::Text(_) => {}
        }
    }
}

/// Checks `blocks` against `module`'s `register`: every block belongs to `module`, appears once,
/// has a registered name and the registered shape, and every key it uses is registered for that
/// name with the value's type; times are RFC 3339 and table key-column values are unique.
pub fn validate(
    module: &str,
    register: &Register,
    blocks: &[Block],
) -> Result<(), Vec<EvidenceError>> {
    let mut errors = Vec::new();
    let mut seen = HashSet::new();
    for block in blocks {
        if !seen.insert(&block.id) {
            errors.push(EvidenceError::DuplicateBlock {
                block: block.id.clone(),
            });
        }
        if block.id.module() != module {
            errors.push(EvidenceError::ForeignModule {
                module: module.to_string(),
                block: block.id.clone(),
            });
            continue;
        }
        let Some(registered) = register
            .blocks
            .iter()
            .find(|(name, _)| block.id.is_block_name(name))
            .map(|(_, kind)| *kind)
        else {
            errors.push(EvidenceError::UnregisteredBlock {
                block: block.id.clone(),
            });
            continue;
        };
        let found = block.shape.kind();
        if found != registered {
            errors.push(EvidenceError::ShapeMismatch {
                block: block.id.clone(),
                registered,
                found,
            });
            continue;
        }
        let mut checker = Checker {
            register,
            block: &block.id,
            errors: Vec::new(),
        };
        checker.shape(&block.shape);
        errors.append(&mut checker.errors);
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Checks that every block id a check references names a block of `blocks`; returns the dangling
/// ids, each once, in reference order.
pub fn check_references(checks: &[CheckResult], blocks: &[Block]) -> Result<(), Vec<BlockId>> {
    let present: HashSet<&BlockId> = blocks.iter().map(|b| &b.id).collect();
    let mut seen = HashSet::new();
    let dangling: Vec<BlockId> = checks
        .iter()
        .flat_map(|c| &c.evidence)
        .filter(|id| !present.contains(id) && seen.insert(*id))
        .cloned()
        .collect();
    if dangling.is_empty() {
        Ok(())
    } else {
        Err(dangling)
    }
}

/// A check that breaks the evidence rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceRuleError {
    /// A `pass`, `warn` or `fail` check references no block.
    Unreferenced { check: crate::CheckId },
    /// A check references a block the run does not hold.
    Dangling {
        check: crate::CheckId,
        block: BlockId,
    },
}

impl fmt::Display for EvidenceRuleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EvidenceRuleError::Unreferenced { check } => {
                write!(f, "check {check} was judged without evidence")
            }
            EvidenceRuleError::Dangling { check, block } => {
                write!(f, "check {check} references missing block {block}")
            }
        }
    }
}

impl std::error::Error for EvidenceRuleError {}

/// The evidence rule: a `pass`, `warn` or `fail` check references at least one block, and every
/// reference of any check resolves to a block of `blocks`.
pub fn check_evidence_rule(
    results: &[CheckResult],
    blocks: &[Block],
) -> Result<(), Vec<EvidenceRuleError>> {
    let present: HashSet<&BlockId> = blocks.iter().map(|b| &b.id).collect();
    let mut errors = Vec::new();
    for r in results {
        if matches!(r.status, Status::Pass | Status::Warn | Status::Fail) && r.evidence.is_empty() {
            errors.push(EvidenceRuleError::Unreferenced {
                check: r.id.clone(),
            });
        }
        for id in r.evidence.iter().filter(|id| !present.contains(id)) {
            errors.push(EvidenceRuleError::Dangling {
                check: r.id.clone(),
                block: id.clone(),
            });
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}
