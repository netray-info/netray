# Plan: v2-input-limits

## Phase 1 — Address forms in the domain rule

### Groups

1. C1 to C11: one production file; every caller reaches the shared rule.

### Plan

Group 1: `crates/common/src/domain.rs` `validate`: test the input, the input without one trailing
dot, and that without surrounding brackets as `IpAddr` before the dot is stripped; then refuse a
numeric last label (`is_number`: all digits, or `0x` and hex digits). No caller changes: lens's
`validate_domain`, the evidence pages, `registrable_domain` and `check_domain` map any `Err` to
their existing refusal.
