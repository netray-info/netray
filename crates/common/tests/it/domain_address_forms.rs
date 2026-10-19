//! spec v2-input-limits, Phase 1 (R1, R2; C1, C2, C4 to C6): the shared domain rule refuses every
//! name that reads as an IP address, a trailing dot, brackets and a numeric last label included.
use netray_common::domain::validate;

#[test]
fn c4_dotted_and_bracketed_addresses_are_refused_as_addresses() {
    for input in [
        "192.0.2.1.",
        "2001:db8::1.",
        "::1.",
        "[2001:db8::1]",
        "[::1]",
        "[::1].",
        "[2001:db8::1].",
    ] {
        let err = validate(input).expect_err(input);
        assert!(
            err.starts_with("IP addresses are not accepted as domain input"),
            "{input}: {err}"
        );
    }
}

#[test]
fn c5_a_numeric_last_label_is_refused() {
    for input in [
        "1.2.3",
        "3221225985",
        "0x7f.1",
        "example.0x1F",
        "192.0.2.01",
        "example.123.",
    ] {
        let err = validate(input).expect_err(input);
        assert!(
            err.starts_with("the last label is a number"),
            "{input}: {err}"
        );
    }
}

#[test]
fn c6_digits_elsewhere_stay_valid() {
    let cases = [
        ("123.example.com", "123.example.com"),
        ("1password.com", "1password.com"),
        ("Example.COM.", "example.com"),
        ("localhost", "localhost"),
        ("a1", "a1"),
        ("x.0xg", "x.0xg"),
    ];
    for (input, want) in cases {
        assert_eq!(validate(input), Ok(want.to_string()), "{input}");
    }
}
