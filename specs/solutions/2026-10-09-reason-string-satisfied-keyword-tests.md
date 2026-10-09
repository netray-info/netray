---
class: check-passes-without-observing
repeat-of: specs/solutions/2026-10-09-golden-rewrite-dropped-framing.md
---
# One reason string listing every keyword satisfied tlsight's contains-asserts

**What failed.** tlsight's target-policy tests assert `err.contains("loopback")`, `contains("private")` and so on per address. Moving tlsight onto the shared policy, the coder first returned one constant naming every range, so every assert held while every refused address showed the same text.
**What worked.** The orchestrator read the coder's report and replaced it with `target_policy::refusal_reason(ip) -> Option<&'static str>`, one reason per range, which tlsight returns (`crates/common/src/target_policy.rs`).
**How to notice next time.** A string assertion with `contains` over a value that is a constant in the code under test.
