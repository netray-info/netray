//! Strict layered configuration loading shared by all services.
//!
//! Precedence (highest first): environment variables > TOML file > serde defaults.
//! Unknown keys are rejected when the target type carries `#[serde(deny_unknown_fields)]`.

use std::collections::HashMap;
use std::ffi::OsString;

use serde::de::DeserializeOwned;

pub use ::config::ConfigError;

/// Load configuration from an optional TOML file and the process environment.
///
/// `prefix` is the full variable prefix including its separator: `"PRISM_"` for
/// `PRISM_SERVER__BIND`, `"NETRAY_HTTP_"` for `NETRAY_HTTP_SERVER__BIND`. `<NAME>_CONFIG`
/// (NAME = prefix without trailing underscores) names the config file, not a key.
pub fn load<T: DeserializeOwned>(path: Option<&str>, prefix: &str) -> Result<T, ConfigError> {
    load_with_env(path, prefix, std::env::vars_os())
}

/// Fails when `env` holds a variable of the retired prefix `legacy` (ASCII case-insensitive),
/// naming that variable, the `new` prefix and `<new>CONFIG`.
pub fn refuse_legacy_prefix(
    legacy: &str,
    new: &str,
    env: &[(OsString, OsString)],
) -> Result<(), ConfigError> {
    let found = env.iter().find_map(|(k, _)| {
        let name = k.to_string_lossy();
        name.as_bytes()
            .get(..legacy.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(legacy.as_bytes()))
            .then(|| name.into_owned())
    });
    match found {
        Some(var) => Err(ConfigError::Message(format!(
            "{var} is set: these variables are now {new}* (config file: {new}CONFIG)"
        ))),
        None => Ok(()),
    }
}

/// Like [`load`], reading variables from `env` instead of the process environment.
/// Entries whose key or value is not valid UTF-8 are skipped. The prefix and the
/// `<NAME>_CONFIG` exclusion match ASCII case-insensitively.
pub fn load_with_env<T: DeserializeOwned>(
    path: Option<&str>,
    prefix: &str,
    env: impl IntoIterator<Item = (OsString, OsString)>,
) -> Result<T, ConfigError> {
    let name = prefix.trim_end_matches('_');
    let config_var = format!("{name}_CONFIG");

    let map: HashMap<String, String> = env
        .into_iter()
        .filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)))
        .filter(|(k, _)| {
            k.as_bytes()
                .get(..prefix.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(prefix.as_bytes()))
                && !k.eq_ignore_ascii_case(&config_var)
        })
        .collect();

    let mut builder = ::config::Config::builder();
    if let Some(path) = path {
        builder = builder.add_source(::config::File::with_name(path).required(true));
    }
    builder = builder.add_source(
        ::config::Environment::with_prefix(name)
            .prefix_separator(&prefix[name.len()..])
            .separator("__")
            .try_parsing(true)
            .source(Some(map)),
    );

    builder.build()?.try_deserialize()
}
