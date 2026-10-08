# Handoff: monorepo-p3/p1
Step: implement
Reason: amendment-forward
What you must decide: `--check-config` for http and email only deserialises, because spectra and beacon have no `validate()`; a config that crash-loops at startup passes the operator check. (a) Add `validate()` to spectra (zero rate limits) and beacon (`per_ip` parses, non-zero) and a tlsight check that `custom_ca_dir` exists, as a Phase 1 follow-up before Phase 2 — small, test-first, makes C5 honest; (b) accept the gap and record it in the spec — the check stays a schema check for http/email.
Detail: reader finding on 719ab5f: spectra fixture with `per_ip_per_minute = 0` → `netray http --check-config` exit 0, `netray http` panics at crates/spectra/src/security/rate_limit.rs:20 (`expect("validated non-zero")`); beacon `per_ip = "0/min"` → exit 0, `AppState::new` fails at crates/beacon/src/state.rs:76; tlsight missing `custom_ca_dir` → exit 0, panic at crates/tlsight/src/state.rs:85.
Resume: /adlc-process:cycle --from implement
