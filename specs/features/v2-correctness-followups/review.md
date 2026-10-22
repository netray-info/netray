# Review: v2-correctness-followups

## 4c6851dc..88684567

### Reader

COUNTS blockers=0 majors=0 minors=1
LENSES Engineering, Security, Testing
MINOR | CLAUDE.md:59 | After this change, CLAUDE.md still tells agents that a live domain run is charged against the DNS per-target burst. Phase 2 removed that charge, and requirement 9 asked for CLAUDE.md to be updated, so this line now contradicts the exception added at line 56. | Lens built from `lens.dev.toml` (per_target_burst 50) and run for a series of domains. Line 59 says only two runs are admitted and that raising `LENS_MODULES__DNS__LIMITS__PER_TARGET_BURST` lets more through. At END the registry instance is built with `.admitted_by_caller()` (crates/netray/src/main.rs:225-230), so `charge_targets` returns early. Every run is admitted, and changing that env var has no effect.

```quote CLAUDE.md:59
- A live domain run at the dev DNS limits admits two runs (burst 50, 19 per run, keyed by resolver): raise `LENS_MODULES__DNS__LIMITS__PER_TARGET_BURST` for a series of live checks.
```

```quote crates/netray/src/main.rs:225
    let dns = Arc::new(
```

Traced and found sound: TLS `decide` (version probes refuse on close_notify at level 1 or 2 and fatal 10, 40, 47, 50, 70, 71, 86, 112; 80, 90, unknown codes and odd level bytes stay Unanswered; suite groups unchanged); DNS `translate` (NotFound to Warn for `https_svcb` only, after the unmeasured passes); profile v5 (v4's numbers, lock hash matches); `lens_registry` (only the registry's DNS instance loses its limiter; `[api.dns]` unchanged); new tests registered, protected changes carry trailers; fixtures change only `profile_version`.

### Spec reader

COUNTS blockers=0 majors=0 minors=0
REQUIREMENTS met=9 partial=0 missed=0 contradicted=0
Each phase traced through the code at END with no divergence (Phase 1 `decide`; Phase 2 `lens_registry` `.admitted_by_caller()`, direct jobs under `[api.dns]`; Phase 3 `translate`, v5.toml, lock, fixtures and goldens; SDD amendments in their code commits 592bc288, 773bbbaf, 32aa9397; CLAUDE.md and CHANGELOG in df1656b6). Outside the requirements: CLAUDE.md line 59 no longer matches Phase 2 (the reader's MINOR).

### Refuted

None: no BLOCKER or MAJOR was reported, so no refuter ran.

### Calibration

None.

### Roll call

| id | answer | evidence |
|---|---|---|
| P03 | absence | no new Status or Verdict enum; no codename in UI copy or API output |
| P10 | convergence | `dns.https_svcb` NotFound becomes warn, a failed lookup stays unmeasured; version probes turn only refusing alerts into Refused, others stay Unanswered |
| P12 | absence | no new outbound client or engine deadline path |
| P13 | convergence | domain runs bounded by lens admission; direct DNS jobs keep `[api.dns]` |
| P18 | absence | no Client construction, no config struct |
| P26 | absence | no new check id |
| P35 | absence | no rules change |
| P36 | convergence | goldens and history fixtures regenerated with their sources |
| P40 | absence | no module added or removed |

### Summary

Before refutation: reader 0/0/1, spec reader 0/0/0 (REQUIREMENTS met=9 partial=0 missed=0
contradicted=0). After refutation: the same, 0 blockers, 0 majors, 1 minor (CLAUDE.md:59, stale
note on the dev DNS burst). verified 0, held 0. Roll call: 9 answered, 3 convergence, 0
divergence, 6 absence.
