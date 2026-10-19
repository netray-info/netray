
## 5ae774d2b18d75b82306babf9a08fc290a05fcea..0fa6aa3aafb1a156f93040ac5567f19ec7b77694

### Reader

COUNTS blockers=0 majors=0 minors=2
LENSES Engineering, Security, Testing
MINOR | crates/http/tests/csp_evidence.rs:165 | The test is named "in order" and requirement 7 asks for the `csp` findings in issue order, but it only checks that each finding is one of the issues, so a wrong order passes. | Mutation: in `check_result` (crates/http/src/module.rs:397), add the issues in reverse, or push one issue message twice. The fixture has the two issues "script-src contains 'unsafe-inline'" and "Missing base-uri directive". Either mutation still gives `findings.len() == 2` with every entry in `issues`, so the test stays green. No other test or lens golden contains a CSP with two or more issues.
MINOR | crates/http/tests/csp_structured_issues.rs:65 | Requirement 5 says the V1 `security.csp.issues` texts must not change, but this test builds its expected value with the same `issues()` call that `analyze_csp` uses, so a changed text passes. | Mutation: in `crates/http/src/inspect/csp.rs` `issues()`, change "object-src is not set to 'none'" (or "base-uri is permissive", "script-src contains data: URI", "style-src contains 'unsafe-inline'") to any other wording. The test stays green because both sides change together. Only "Missing default-src directive" is pinned word for word, and the unit test `object_src_not_none_is_warn` only checks `contains("object-src")`.

```quote crates/http/tests/csp_evidence.rs:165
        assert!(issues.contains(&text(f)), "{f} is no issue in {issues:?}");
```

```quote crates/http/tests/csp_structured_issues.rs:65
        assert_eq!(report.issues, messages, "csp: {csp}");
```

### Spec reader

COUNTS blockers=0 majors=0 minors=0
REQUIREMENTS met=13 partial=0 missed=0 contradicted=0

No divergences in requirements 1-13; each traced with a concrete input through the code at 0fa6aa3a. Two behaviours beyond the requirement text (a received `set-cookie#<n>` header keeps its row; Referrer-Policy reads every header line) are recorded in the spec's Decision log and counted as met.

### Refuted

None: no BLOCKER or MAJOR finding to refute.

### Calibration

None: the refuter was not called.

### Roll call

| principle | answer | evidence |
|---|---|---|
| P03 | convergence | CspSeverity is no status type; CheckStatus reused; no codename in added UI or API text |
| P10 | convergence | non-ASCII and unrecognised Referrer-Policy warn; missing header takes the presence path |
| P12 | absence | no outbound call or engine run added |
| P13 | absence | no limiter or route change |
| P18 | absence | no client or config struct added |
| P26 | convergence | no new check ID; each new block built once from the existing analysis |
| P35 | absence | no specs/rules change; the volatile-row rule has its own test |
| P36 | convergence | the nine lens-full goldens change with module.rs, regenerated |
| P40 | convergence | only the http module, a catalogue text and lens's http.server reader change |

### Summary

Before refutation: reader 0/0/2, spec reader 0/0/0 (REQUIREMENTS met=13 partial=0 missed=0 contradicted=0). After refutation: the same; verified 0, held 0. Roll call: answered 9, convergence 5, divergence 0, absence 4. Both MINORs are test-strength gaps already listed as NITs in report.md.
