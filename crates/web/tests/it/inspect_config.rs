//! Phase 5, requirement 14: the `[inspect]` config of lens, its defaults, strictness and validation.

use netray_web::pages::{InspectConfig, InspectLimits, Tool};

fn limits(deadline_ms: u64, max_concurrent: u32) -> InspectLimits {
    InspectLimits {
        deadline_ms,
        max_concurrent,
    }
}

#[test]
fn defaults_are_the_documented_budgets() {
    let c = InspectConfig::default();
    assert_eq!(c.dns, limits(30000, 8));
    assert_eq!(c.tls, limits(15000, 8));
    assert_eq!(c.http, limits(30000, 8));
    assert_eq!(c.ip, limits(5000, 8));
    assert_eq!(c.limits(Tool::Tls), limits(15000, 8));
    assert!(c.validate().is_ok());
}

#[test]
fn a_partial_table_keeps_the_other_defaults() {
    let c: InspectConfig = toml::from_str("[dns]\nmax_concurrent = 2").unwrap();
    assert_eq!(c.dns, limits(30000, 2));
    assert_eq!(c.tls, limits(15000, 8));
    assert_eq!(c.http, limits(30000, 8));
    assert_eq!(c.ip, limits(5000, 8));
}

#[test]
fn unknown_keys_are_refused() {
    let nested = toml::from_str::<InspectConfig>("[tls]\nfoo = 1").unwrap_err();
    assert!(nested.to_string().contains("foo"), "{nested}");
    let top = toml::from_str::<InspectConfig>("[smtp]\nmax_concurrent = 1").unwrap_err();
    assert!(top.to_string().contains("smtp"), "{top}");
}

#[test]
fn validate_refuses_zero_and_names_the_key() {
    for tool in ["dns", "tls", "http", "ip"] {
        for key in ["max_concurrent", "deadline_ms"] {
            let c: InspectConfig = toml::from_str(&format!("[{tool}]\n{key} = 0")).unwrap();
            let err = c.validate().unwrap_err();
            assert!(
                err.contains(&format!("inspect.{tool}.{key}")),
                "{tool}.{key}: {err}"
            );
        }
    }
}
