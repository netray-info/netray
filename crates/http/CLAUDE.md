# CLAUDE.md -- netray-http

The HTTP module of `netray serve` (V1 codename spectra): a library, no binary. What it measures,
its checks and its config keys are in `README.md`. Repo-wide rules, including the http-specific
ones (graded headers judged in `src/module.rs`, `http.isolation`, volatile paths, contract
goldens), live in the root [CLAUDE.md](../../CLAUDE.md) and are not repeated here.

## Layout

- `src/module.rs` -- `HttpModule` (`Module` impl), `REGISTER`, `VOLATILE`, `translate` from the
  inspection onto the thirteen check ids and their evidence blocks.
- `src/job.rs` -- the inspection netray-web runs (API v1 and `/inspect/http`): parse, charge
  limits, resolve under the target policy, inspect; refusals as `HttpRefusal`.
- `src/input.rs` -- URL parsing and target validation (`netray_common::target_policy`).
- `src/inspect/` -- the three probes (`mod.rs`), the outbound request chain over
  `netray_common::fetch` (`request.rs`), header analysers (security, csp, cors, cookies,
  caching, fingerprint) and the V1-shaped `InspectResponse` (`assembler.rs`).
- `src/quality/` -- per-check scoring of the V1 report; `translate` maps it onto the model.
- `src/security/` -- `RateLimitState`, the per-IP and per-target limiters the job uses.
- `src/testing.rs` -- stubs behind the `testing` feature.

## Rules

- `CheckStatus` orders Pass < Skip < Warn < Fail; `Skip` maps to `NotApplicable`.
- Limits are charged before any DNS resolution, in the module (`measure`) and in the job
  (`inspect_url`).
- Only `timeout`, `blocked` and `unreachable` probe errors are unanswered (`unmeasured`);
  `refused`, `reset` and `tls` on the HTTPS probe are a measured fail (`unanswered` in
  `src/module.rs`).
- Input with an explicit `http://` scheme is refused; the port-80 probe runs for every
  `https://` target.
- The `testing`-feature targets (`tests/module.rs`, `tests/job.rs`) need
  `cargo test -p netray-http --features testing --test <name>`; everything else is in
  `tests/it/` (`cargo test -p netray-http --test it <module>`).

## Specs

- Apply [architecture-rules](../../specs/rules/architecture-rules.md) when changing outbound
  probes, limits or errors.
- Apply [logging-rules](../../specs/rules/logging-rules.md) when changing tracing.
