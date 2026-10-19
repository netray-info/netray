//! spec v2-input-edges, Phase 1 (R1 to R4; C3, C6 to C11): the shared domain rule maps the trimmed
//! input with UTS #46 first, so full-width, ideographic and Unicode forms reach the address checks
//! and answer one ASCII form, and a character no domain holds is refused naming the input.
use netray_common::domain::validate;

#[test]
fn c3_a_mapping_error_names_the_trimmed_input() {
    let err = validate("  exa mple.com ").expect_err("space inside");
    assert!(err.starts_with("not a valid domain name"), "{err}");
    assert!(err.ends_with("exa mple.com"), "{err}");
}

#[test]
fn c6_fullwidth_addresses_are_refused_as_addresses() {
    for input in ["１.１.１.１", "１.１.１.１."] {
        let err = validate(input).expect_err(input);
        assert!(
            err.starts_with("IP addresses are not accepted as domain input"),
            "{input}: {err}"
        );
    }
}

#[test]
fn c7_a_fullwidth_numeric_last_label_is_refused() {
    let input = "０x７f.1";
    let err = validate(input).expect_err(input);
    assert!(
        err.starts_with("the last label is a number"),
        "{input}: {err}"
    );
}

#[test]
fn c8_forbidden_characters_inside_are_refused() {
    for input in [
        "1.1.1.1 .",
        "exa mple.com",
        "example.com/x",
        "ex%61mple.com",
        "exa\tmple.com",
    ] {
        let err = validate(input).expect_err(input);
        assert!(err.starts_with("not a valid domain name"), "{input}: {err}");
    }
}

#[test]
fn c9_width_stop_and_case_variants_map_to_one_form() {
    for input in ["ＥＸＡＭＰＬＥ.com", "example。com", "Example.COM."] {
        assert_eq!(validate(input), Ok("example.com".to_string()), "{input}");
    }
}

#[test]
fn c10_unicode_and_punycode_answer_one_ascii_form() {
    for input in ["bücher.de", "xn--bcher-kva.de"] {
        assert_eq!(
            validate(input),
            Ok("xn--bcher-kva.de".to_string()),
            "{input}"
        );
    }
}

#[test]
fn c11_ordinary_names_answer_unchanged() {
    for input in [
        "123.example.com",
        "1password.com",
        "localhost",
        "_dmarc.example.com",
        "xn--nxasmq6b.xn--jxalpdlp",
    ] {
        assert_eq!(validate(input), Ok(input.to_string()), "{input}");
    }
}

#[test]
fn c7_a_last_label_numeric_only_after_mapping_is_refused() {
    for input in ["a.１２３", "a.０x７f"] {
        let err = validate(input).expect_err(input);
        assert!(
            err.starts_with("the last label is a number"),
            "{input}: {err}"
        );
    }
}
