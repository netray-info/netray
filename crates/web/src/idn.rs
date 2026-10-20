//! The Unicode form of an internationalised domain (spec v2-site-nav, requirement 16): shown
//! beside the A-label, never instead of it. URLs, canonical links, exports and the API keep the
//! A-label.

/// The Unicode form of `name` when its A-labels decode (UTS #46 to_unicode) without error and
/// it differs from `name`, else `None`.
pub fn unicode(name: &str) -> Option<String> {
    let (decoded, result) = idna::domain_to_unicode(name);
    (result.is_ok() && decoded != name).then_some(decoded)
}

/// `name` as a page title starts with it: "<unicode> (<a-label>)" for a decoded name, else the
/// name itself.
pub fn title(name: &str) -> String {
    unicode(name).map_or_else(|| name.to_string(), |u| format!("{u} ({name})"))
}
