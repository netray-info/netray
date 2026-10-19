## 12bea04a..f2fe046c

### Reader

COUNTS blockers=0 majors=2 minors=0
LENSES Engineering, Security, Testing
MAJOR | justfile:60 | The ADLC-RAN line reports every tracked `crates/*/tests/it/**` file as run, including files in crates that have no `it` target. A test file that never compiles therefore passes adlc's orphan check. | I tested this in a scratch clone at f2fe046. I added `crates/ip/tests/it/stray.rs` containing `#[test] fn never_runs() { panic!() }` and staged it. Results: (1) `cargo metadata` shows no target for it, so it never compiles. (2) `tests/repo/test_test_binaries.sh` printed `ok`: its declared-module check (tests/repo/test_test_binaries.sh:51) only covers crates in MERGED, and its check for crates outside MERGED (tests/repo/test_test_binaries.sh:65) only looks for `main.rs`. (3) The recipe printed `ADLC-RAN crates/ip/tests/it/stray.rs`, so adlc counts it as run and orphans comes back verified. Before this change the same file was not any target's `src_path` and adlc refused it as an orphan. The same applies to `crates/common/tests/it/` and `crates/netray/tests/it/`.
MAJOR | crates/ip/build.rs:23 | In a reftable checkout the only git path the script watches is `.git/HEAD`, which never changes there. Cargo then never reruns the script, and `GIT_SHORT_SHA` stays at an old commit. Before the change it was correct, because the script reran on every build. | I tested this with git 2.56 and the build script compiled on its own. `git init --ref-format=reftable` produces a HEAD stub `ref: refs/heads/.invalid`. The script printed only `rerun-if-changed=<repo>/.git/HEAD` (`logs/HEAD` and the loose branch ref do not exist there) and `GIT_SHORT_SHA=6adec5b`. After `git commit` HEAD was `3cf30fd`, but the mtime of `.git/HEAD` was unchanged (1792429609 before and after). Cargo would keep the stale `6adec5b` in netray-ip's `build.revision`. Files-backend checkouts (the default, and what this repo and CI use) are not affected.

```quote justfile:60
    @git ls-files 'crates/*/tests/it/*.rs' 'crates/*/tests/it/**/*.rs' | grep -v '/main\.rs$' | sed 's/^/ADLC-RAN /'
```

```quote tests/repo/test_test_binaries.sh:51
    for f in files:
```

```quote tests/repo/test_test_binaries.sh:65
for f in subprocess.run(["git", "ls-files", "crates/*/tests/it/main.rs"], capture_output=True, text=True).stdout.split():
```

```quote crates/ip/build.rs:23
    let mut watched: Vec<String> = ["HEAD", "logs/HEAD"].iter().filter_map(|n| path_of(n)).collect();
```

Traced and found sound: `check` against `adlc-verify` (superset guard, fixtures; netray-ip's `testing` tests run under `cargo test --workspace`); the `tests/it` merges in all nine crates (include paths, `use crate::common`, no env mutation, thread-local metrics recorders, `let _ =` rustls installs, no shared temp prefix); the ok_handlers shared `OnceCell` app (`ip_cache` off, `dns_cache` consulted only when DNS is not skipped, no rate-limit assertions); the mold step and `test_workflows.sh`; the c15 clock change.

### Spec reader

COUNTS blockers=2 majors=2 minors=0
REQUIREMENTS met=6 partial=1 missed=0 contradicted=2
BLOCKER | crates/ip/build.rs:23 | The build script watches `HEAD`, `logs/HEAD` and the branch's loose ref. It never names `refs/heads` or `packed-refs`, which are the paths requirement 4 lists. | R4: in this worktree `git rev-parse --path-format=absolute --git-path refs/heads` gives `/Users/lukas/Documents/src/netray/.git/refs/heads`, and that path exists. The script never emits it and emits `logs/HEAD` in its place. Decision log line 187 amends this, but R4's own wording was never changed to match.
BLOCKER | .github/workflows/ci.yml:15 | `timeout-minutes` is 20. The measured `check` job took 5m34s, so twice that, rounded up to a multiple of five, is 15; 20 is about 3.6 times the measured run. | R9 and the Phase 6 scenario: D = 5m34s gives 2·D = 11m08s, which rounds up to 15. Decision log line 191 picks 20 for cold-cache headroom, but R9's wording was not changed. The disk half of R9 is met.
MAJOR | tests/repo/test_test_binaries.sh:9 | `netray-common` is not merged, although four of its test files have no required-features and use no environment variables or global state. | R7, "Every other crate whose tests merge mechanically does the same": `crates/common/tests/prefetch.rs`, `config_loader.rs`, `ip_extract_no_cf.rs` and `target_policy_results_table.rs` each still build as their own test binary. report.md line 119 excuses common as "mostly required-features".
MAJOR | tests/repo/test_test_binaries.sh:34 | The repo check lets through any test target that has `required-features` or `harness = false`, whatever its name, so the exception list in `EXCEPTIONS` is not the only way past it. | R7, "fails when a merged crate has a test target other than `it` and its listed exceptions": add `crates/dns/tests/x.rs` with `[[test]] name = "x"` and `required-features = ["testing"]`. Line 34 reaches `continue` and the check passes.

```quote crates/ip/build.rs:23
    let mut watched: Vec<String> = ["HEAD", "logs/HEAD"].iter().filter_map(|n| path_of(n)).collect();
```

```quote .github/workflows/ci.yml:15
    timeout-minutes: 20
```

```quote tests/repo/test_test_binaries.sh:9
MERGED="netray-web lens netray-engine netray-model netray-store netray-dns netray-tls netray-email netray-http"
```

```quote tests/repo/test_test_binaries.sh:34
        if t.get("required-features") or not t.get("test", True) or f"{n}:{tn}" in exc:
```

```quote specs/features/v2-ci-speed/spec.md:65
4. A second `cargo build -p netray` with no source change reports `netray-ip` fresh: its build
```

```quote specs/features/v2-ci-speed/spec.md:82
9. After the measured run, `timeout-minutes` drops to about twice the measured `check` duration,
```

```quote specs/features/v2-ci-speed/spec.md:74
7. The integration tests of `netray-web` and `lens` each build as one test target `it`
```

### Refuted

None dropped. The refuter answered the spec reader's two BLOCKERs (crates/ip/build.rs:23 R4 wording; .github/workflows/ci.yml:15 timeout) REFUTED at confidence 6: the decision log records both deviations with reasons, so the code does not contradict the spec; R4's and R9's text is stale. Below 7, both are downgraded to MINOR.

### Calibration

| finding | refuter answer | confidence | result | command |
|---|---|---|---|---|
| justfile:60 ADLC-RAN covers non-merged crates | CONFIRMED | 8 | held | read justfile:60 and test_test_binaries.sh:65 (only main.rs outside MERGED is checked) |
| crates/ip/build.rs:23 reftable never reruns | CONFIRMED | 7 | held | git 2.56 reftable: HEAD is a static stub, logs/HEAD and loose refs absent, path_of drops them |
| crates/ip/build.rs:23 R4 wording (BLOCKER) | REFUTED | 6 | not held as a wrong result (decision log line documents it); kept as MINOR, stale R4 text | read spec.md:65 and the decision log |
| ci.yml:15 timeout 20 vs R9 (BLOCKER) | REFUTED | 6 | not held as a wrong result (decision log documents 20 over 15); kept as MINOR, stale R9 text | read spec.md:82, 172 and the decision log |
| test_test_binaries.sh:9 netray-common not merged | CONFIRMED | 6 | held | grep: config_loader, prefetch, ip_extract_no_cf, target_policy_results_table have no [[test]] entry and no set_var |
| test_test_binaries.sh:34 required-features bypasses the list | CONFIRMED | 7 | held | read line 34 against R7's wording |

### Roll call

| principle | answer | evidence |
|---|---|---|
| P03 | absence | no enum, UI or API copy in the range |
| P10 | absence | no check or scoring code changed |
| P12 | absence | no reqwest builder or module entry changed |
| P13 | absence | no limiter or route change |
| P18 | absence | no client construction or config struct added |
| P26 | absence | no check ID added or moved |
| P35 | convergence | rule edits are path updates; new tests/repo checks enforce the new layout, build freshness, verbs superset |
| P36 | convergence | no generated artefact changed without its source |
| P40 | absence | no module added or removed |

### Summary

Before refutation: reader 0/2/0, spec reader 2/2/0 (REQUIREMENTS met=6 partial=1 missed=0 contradicted=2).
After refutation: blockers 0, majors 4, minors 2 (reader 0/2/0; spec reader 0/2/2). verified 4, held 4.
Roll call: answered 9, convergence 2, divergence 0, absence 7.
All four majors and both minors are fixed on the branch after f2fe046 (see report.md, review fixes).
