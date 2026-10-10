# Review: v2 model

## 73c3f58..35e9571

### Reader

COUNTS blockers=0 majors=0 minors=4
LENSES Engineering, Security, Testing
MINOR | tests/repo/test_engine_names_no_module.sh:22 | The check selects the package by name `netray-engine`; a renamed engine package passes with a module dependency. | `name = "netray-engine-core"` plus a beacon dev-dependency: PASS.
MINOR | tests/repo/fixtures/engine-deps/good.toml:7 | The good fixture has no dependencies, so C7 (only netray-model and external crates pass) never runs and a substring match would go unnoticed. | `grep -q` instead of `grep -qx` still passes.
MINOR | tests/repo/test_cargo_workspace.sh:28 | Nothing enforces R1's "model depends on no workspace crate" after the skip. | `netray-common` added to crates/model/Cargo.toml: no check fails.
MINOR | tests/repo/test_engine_names_no_module.sh:13 | Only the listed module crates are refused, so another workspace crate passes against R4 "netray-model only". | `netray-common` added to crates/engine/Cargo.toml: PASS.

```quote tests/repo/test_engine_names_no_module.sh:22
    if p["name"] == "netray-engine":
```

```quote tests/repo/fixtures/engine-deps/good.toml:7
[dependencies]
```

```quote tests/repo/test_cargo_workspace.sh:28
    case "$m" in crates/common/Cargo.toml|crates/model/Cargo.toml|crates/engine/Cargo.toml) continue ;; esac
```

```quote tests/repo/test_engine_names_no_module.sh:13
modules='beacon tlsight spectra prism ifconfig-rs lens netray-dns netray-tls netray-http netray-email netray-ip'
```

```quote CLAUDE.md:17
| `crates/{model,engine}` | V2 core (planning SDD `v2.md` §3): `netray-model` the check vocabulary, no workspace dependency; `netray-engine` the `Module`/`FactsProvider` traits, depends on `netray-model` only (`tests/repo/test_engine_names_no_module.sh`). Each V1 crate maps its status word onto `netray_model::Status` once |
```

### Summary

0/0/4, no blocker or major to refute or verify. The four minors are fixed on this branch: the check selects the package by manifest path and refuses every workspace crate but netray-model for the engine and every workspace crate for the model; the good fixture lists netray-model and lookalikes.

## 35e9571..fe909d5

### Reader

COUNTS blockers=0 majors=0 minors=1
LENSES Engineering, Testing
MINOR | tests/repo/test_engine_names_no_module.sh:60 | No fixture pins the module-name clause: every fixture's beacon is also a workspace member. | Mutant without `d in modules` passes the self-test; engine with a git `netray-dns` would pass it.

```quote tests/repo/test_engine_names_no_module.sh:60
for c in rename dotted target-inline dev build renamed-package; do case_flags "$c" beacon; done
```

### Summary

0/0/1; fixed with fixture `module-external` (registry `netray-dns`); the mutant now fails `self-test: fixture module-external did not report netray-dns`.
