//! V2 spec v2-inspectors, Phase 1, requirement 1 (SDD §3.7): the evidence renderer is generic.
//! No module name, block name or key appears in `crates/web/src/evidence.rs`.
use std::fs;
use std::path::PathBuf;

const BLOCK_PREFIXES: [&str; 5] = ["dns.", "tls.", "http.", "email.", "ip."];

#[test]
fn evidence_renderer_source_names_no_block_prefix() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/evidence.rs");
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("missing renderer source {}: {e}", path.display()));

    assert!(
        source.contains("pub fn block_view"),
        "{} does not define `pub fn block_view`",
        path.display()
    );

    let code: String = source
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");

    for prefix in BLOCK_PREFIXES {
        assert!(
            !code.contains(prefix),
            "evidence.rs names the block prefix `{prefix}`; the renderer must stay generic"
        );
    }
}
