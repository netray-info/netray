# Report: changelog 0.24.0

## Phase 1 — Release notes

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | the IP-data entry names the lens data mount `/netray/data` | test-unwritable | — |
| C2 | a Changed entry names the config-only `--check-config` | test-unwritable | — |
| C3 | `[Unreleased]` names `/netray/data` for lens and the `--check-config` change | test-unwritable | — |

Release-note prose: `just release` moves the section under a version header, so a standing test on `[Unreleased]` breaks at the release. Checked once: `grep -n '/netray/data\|check-config checks configuration only' CHANGELOG.md` finds both in `[Unreleased]`.

### Behavioural verification

skipped: prose only.
