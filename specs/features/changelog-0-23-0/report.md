# Report: changelog 0.23.0

## Phase 1 — Unreleased entries

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | Each grade-affecting change has one `### Changed` entry naming its requirement | test-unwritable | — |
| C2 | The R3.7 entry names the mixed-address case | test-unwritable | — |
| C3 | Fixes without a grade change go under `### Fixed` | test-unwritable | — |
| C4 | No `### Security` section; no entry says security, SSRF or names an attack | test-unwritable | — |
| C5 | `## [Unreleased]` holds `### Changed` and `### Fixed` and no `### Security` | test-unwritable | — |
| C6 | The `0.22.2` section is unchanged | test-unwritable | — |

All criteria are test-unwritable as a standing test: `just release 0.23.0` moves the entries below a version header, so any assertion on `## [Unreleased]` breaks at the release. They were checked once, below.

### Behavioural verification

```
$ awk '/^## \[Unreleased\]/{f=1} /^## \[0.22.2\]/{f=0} f' CHANGELOG.md | grep -ciE 'security|ssrf|attack'
0
$ grep -n '^## \|^### ' CHANGELOG.md | head -5
8:## [Unreleased]
12:### Changed
30:### Fixed
36:## [0.22.2] - 2026-10-09
38:### Changed
$ git diff --stat main -- CHANGELOG.md
 CHANGELOG.md | 26 ++++++++++++++++++++++++++
```

The diff adds lines only, so the 0.22.2 section is unchanged. Each entry was checked against the code: `lens_unknown_verdict_total` (`crates/lens/src/backends/mod.rs`), the budget refusal at config load (`crates/lens/src/config.rs:467`), the `?` badge and OG card (`crates/lens/src/badge/render.rs:28`, `og/render.rs:107`), the shipped timeouts (`crates/lens/lens.example.toml`), the sampling note (`crates/lens/src/backends/ip.rs`), `exempt_cidrs` (`crates/ifconfig-rs/src/config.rs`), the `dns=false` cache fix (`ae229b2`) and the export sections (`12d4987`).

### Amendments

- The scratchpad draft said `netray lens --check-config` refuses a timeout budget; no such flag exists, and lens refuses the configuration when it loads. Corrected in the entry.
