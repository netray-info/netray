# Review: v2-v1-remnants

## e2bb5899..a29fecfb

### Reader

COUNTS blockers=2 majors=0 minors=1
LENSES Engineering, Testing
BLOCKER | crates/tls/tests/it/probe_budget.rs:10 | Commit a24a79d4 changes 19 protected TLS test files to rename the module (`netray_tls::routes::` becomes `netray_tls::inspection::`), and no commit in the branch carries an `ADLC-Test-Change` trailer for any of them; the tamper check refuses the branch, so report.md's "the gate passes" (Phase 3 C6) is false. | `adlc check` at a29fecfb: exit 1, `[tamper] refuted: listed: crates/tls/tests/inspector.rs — changed`, and the same for `crates/tls/tests/module.rs` and the 17 files under `crates/tls/tests/it/` touched by the rename.
BLOCKER | docs/ip-enrichment.md:37 | The moved reference puts `user_agent_regexes` among the optional sources, and its intro says an optional source that fails on reload is dropped. A configured regexes file that fails to load returns `LoadError`: `netray serve` refuses to start, and a reload keeps the whole previous context. Requirement 7 ("its reload as the code has them") is not met. | `[modules.ip] user_agent_regexes = "regexes.yaml"` with invalid YAML, then SIGHUP: the code returns `Err` at crates/ip/src/enrichment.rs:146 and `IpModule::reload` logs "keeping previous context" (crates/ip/src/module.rs:167).
MINOR | tests/repo/test_crate_docs.sh:36 | In a git pathspec `*` also matches `/`, so `crates/*/docs/*`, `crates/*/specs/*` and `crates/*/SECURITY.md` match nested paths anywhere inside a crate, not only `crates/<crate>/docs/` as requirement 6 states. | A staged `crates/web/templates/pages/docs/index.html` would fail the check with "V1 record files are tracked".

```quote crates/tls/tests/it/probe_budget.rs:10
use netray_tls::inspection::plan_budget;
```

```quote AGENTS.md:13
3. **Explain a deleted or changed test.** A protected test that is removed, renamed or weakened
```

```quote docs/ip-enrichment.md:37
**Optional** (warning logged, detection disabled): all other sources. The
```

```quote docs/ip-enrichment.md:5
required source fails keeps the previous data; an optional source that fails on reload is
```

```quote crates/ip/src/enrichment.rs:146
                        "user_agent_regexes: failed to load User-Agent regexes from {path}: {e}"
```

```quote crates/ip/src/module.rs:167
                tracing::warn!("Failed to reload enrichment data: {}; keeping previous context", e);
```

```quote tests/repo/test_crate_docs.sh:36
records=$(git ls-files 'crates/*/docs/*' 'crates/*/specs/*' 'crates/*/ROADMAP.md' 'crates/*/SECURITY.md' | head -5)
```

### Spec reader

COUNTS blockers=0 majors=1 minors=0
REQUIREMENTS met=13 partial=1 missed=0 contradicted=0
MAJOR | tests/repo/test_v1_remnants.sh:41 | The remnants check covers requirement 12 only by which files are tracked. It never reads `crates/tls/CLAUDE.md` or the module declaration in `lib.rs`, so it still passes when requirement 12 does not hold. | Put `crates/tls/CLAUDE.md:16` back to `src/routes.rs`, or declare `#[path = "inspection.rs"] pub mod routes;` in `crates/tls/src/lib.rs`: the script prints `PASS: test_v1_remnants`.

```quote specs/features/v2-v1-remnants/spec.md:92
12. `crates/tls/src/routes.rs` is `crates/tls/src/inspection.rs` (module `netray_tls::inspection`);
    every user, `crates/tls/CLAUDE.md` included, names the new path.
13. `tests/repo/test_v1_remnants.sh` fails while 10 to 12 do not hold.
```

```quote tests/repo/test_v1_remnants.sh:41
git ls-files --error-unmatch crates/tls/src/inspection.rs >/dev/null 2>&1 \
    || fails+=("crates/tls/src/inspection.rs is not tracked")
```

```quote crates/tls/CLAUDE.md:16
| `src/inspection.rs` | the inspection core: `resolve_target`, `filter_allowed`, `plan_*`, `inspect`, `InspectResponse` |
```

### Refuted

None. No refuter was spawned: each BLOCKER and MAJOR was verified by command below, which a refuter could not overturn.

### Calibration

| finding | refuter answer | confidence | outcome | command |
|---|---|---|---|---|
| tamper trailers (probe_budget.rs:10) | not asked | - | held | `adlc check` exit 1, `[tamper] refuted` listing 19 tls test files |
| user_agent_regexes optional (ip-enrichment.md:37) | not asked | - | held | read crates/ip/src/enrichment.rs:143-147: a configured failing file returns `Err(LoadError)` |
| remnants check misses req 12 (test_v1_remnants.sh:41) | not asked | - | held | the script checks only `git ls-files` of the two paths |

### Roll call

| principle | answer |
|---|---|
| P03 | convergence: no status enum added; codename references removed |
| P10 | absence: no check or scoring change |
| P12 | absence: no new outbound client or module entry |
| P13 | absence: limiter setup untouched |
| P18 | convergence: no reqwest client added; test_config_strict.sh comment only |
| P26 | absence: no check id added |
| P35 | absence: no rule changed |
| P36 | absence: no generated artefact touched |
| P40 | absence: no module added or removed |

### Summary

Before refutation: reader 2/0/1, spec reader 0/1/0 (REQUIREMENTS met=13 partial=1 missed=0 contradicted=0). After: the same; verified 3, held 3. Roll call: answered 9, convergence 2, divergence 0, absence 7. Outcome: two BLOCKERs held; fixed on this branch under the operator's autonomy mandate, then read again.

## a29fecfb..9444f01a

### Reader

COUNTS blockers=0 majors=0 minors=0
LENSES Engineering, Testing

No wrong result in a29fecfb..9444f01a: the doc claims match `EnrichmentContext::load_with`, `IpModule::reload` and `ip_module`; the new requirement-12 lines fail on `#[path = "inspection.rs"] pub mod routes;` and on `src/routes.rs` in `crates/tls/CLAUDE.md`; the `:(glob)` pathspecs still refuse a staged `crates/dns/docs/notes.md`; commit 4cb501a1 carries 21 parseable `ADLC-Test-Change` trailers.

### Spec reader

COUNTS blockers=0 majors=2 minors=0
REQUIREMENTS met=1 partial=2 missed=0 contradicted=0
LENSES Spec (requirements 7, 9, 13 at 9444f01a)
MAJOR | docs/ip-enrichment.md:6 | The reload sentence says every source other than GeoIP and `user_agent_regexes` is dropped when it fails on reload, but a configured `asn_patterns` file that fails falls back to the built-in patterns. | `asn_patterns = "/nonexistent.toml"` and SIGHUP: `load_with` takes the `Err` arm and returns `AsnPatterns::builtin()`; the doc says "dropped until the next reload".
MAJOR | tests/repo/test_v1_remnants.sh:45 | A `routes` alias by re-export, with users importing it as a bare name, passes the check although requirement 12 does not hold. | `pub(crate) use inspection as routes;` in crates/tls/src/lib.rs with `use crate::{routes, ...}` in module.rs prints `PASS: test_v1_remnants`.

```quote specs/features/v2-v1-remnants/spec.md:76
7. The IP classification reference lives at `docs/ip-enrichment.md`, describes the V2 IP module
```

```quote crates/ip/src/enrichment.rs:286
                Err(e) => {
```

```quote specs/features/v2-v1-remnants/spec.md:94
13. `tests/repo/test_v1_remnants.sh` fails while 10 to 12 do not hold.
```

```quote crates/tls/src/module.rs:24
use crate::inspection::{self, CaaSource, InspectRequest, InspectResponse, PortResult};
```

### Refuted

None; no refuter spawned, both MAJORs verified by command.

### Calibration

| finding | refuter answer | confidence | outcome | command |
|---|---|---|---|---|
| asn_patterns reload (ip-enrichment.md:6) | not asked | - | held | read crates/ip/src/enrichment.rs:286-288: the Err arm returns AsnPatterns::builtin() |
| routes alias passes (test_v1_remnants.sh:45) | not asked | - | held | appended `pub(crate) use inspection as routes;` to crates/tls/src/lib.rs: the 9444f01a script passes |

### Summary

Reader 0/0/0; spec reader 0/2/0 (REQUIREMENTS met=1 partial=2). verified 2, held 2. No blocker. Both MAJORs fixed in the next commit (doc names the asn_patterns fallback; the check refuses any `routes` word in the tls crate) and read again.

## 9444f01a..52b87c2e

### Reader

COUNTS blockers=0 majors=1 minors=0
LENSES Engineering, Testing
MAJOR | tests/repo/test_v1_remnants.sh:51 | The rewritten web half of the users check passes a web-crate file that names the old TLS file path or `routes::plan` in text; the old check refused it. | `// mirrors crates/tls/src/routes.rs` in crates/web/src/api/jobs.rs: the old pattern matched, `netray_tls::routes` does not, and the script prints PASS.

```quote tests/repo/test_v1_remnants.sh:51
    git ls-files 'crates/web/*.rs' | xargs grep -nE 'netray_tls::routes'; } 2>/dev/null | head -3)
```

### Spec reader

COUNTS blockers=0 majors=1 minors=0
REQUIREMENTS met=1 partial=1 missed=0 contradicted=0
MAJOR | tests/repo/test_v1_remnants.sh:51 | Same defect as the reader's (merged): the web scan dropped `src/routes\.rs`. | `// cost plan: crates/tls/src/routes.rs` in crates/web/src/api/jobs.rs passes.

```quote specs/features/v2-v1-remnants/spec.md:93
    every user, `crates/tls/CLAUDE.md` included, names the new path.
```

### Refuted

None; no refuter spawned, the merged MAJOR verified by command.

### Calibration

| finding | refuter answer | confidence | outcome | command |
|---|---|---|---|---|
| web scan narrowed (test_v1_remnants.sh:51) | not asked | - | held | `grep -nE netray_tls::routes` on a line naming crates/tls/src/routes.rs: no match |

### Summary

Reader 0/1/0, spec reader 0/1/0, merged to one MAJOR; verified 1, held 1. No blocker. Fixed in the next commit (the web scan names the old file path and `routes::plan` again).

## 52b87c2e..7e07f474

### Reader

COUNTS blockers=0 majors=0 minors=1
LENSES Engineering, Testing
MINOR | crates/netray/tests/serve_hosts.rs:141 | If the metrics port is taken on the last attempt, the test fails with "GET /health never answered 200 within 30 s" although /health answered 200, and never prints the serve log naming the cause; the same in all ten files. | On attempt 3 `bind_failed` is skipped, a metrics bind failure does not stop serve, and `listening` never holds; the test waits 30 s and panics with the wrong cause.

```quote crates/netray/tests/serve_hosts.rs:141
        panic!("GET /health never answered 200 within 30 s");
```

```quote crates/netray/tests/support/mod.rs:20
    log.contains("netray listening") && log.contains("metrics server listening")
```

### Spec reader

COUNTS blockers=0 majors=0 minors=0
REQUIREMENTS met=4 partial=0 missed=0 contradicted=0
LENSES requirements=10,11,12,13 (Phase 3) decision-log=serve-test-port-race

No findings.

### Summary

Reader 0/0/1, spec reader 0/0/0 (REQUIREMENTS met=4). No BLOCKER or MAJOR, so no refutation; verified 0, held 0. The MINOR is deferred.
