use std::sync::LazyLock;

use regex::Regex;

use super::store::{SHORTID_ALPHABET, SHORTID_LEN};

static SHORTID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[0-9A-Za-z]{8}$").expect("valid regex"));

pub fn generate() -> String {
    nanoid::nanoid!(SHORTID_LEN, &SHORTID_ALPHABET)
}

pub fn validate(s: &str) -> bool {
    SHORTID_RE.is_match(s)
}
