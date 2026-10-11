# Report: changelog 0.25.0

## Phase 1 — Release notes

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | `[Unreleased]` holds the lead with the four shifts, Changed (check names, config rules), Fixed, Known issues | test-unwritable | — |
| C2 | only `netray.info` appears as a real domain | test-unwritable | — |

Release-note prose: `just release` moves the section under a version header, so a standing test on `[Unreleased]` breaks at the release. Checked once: the section's four headings are present, and a grep for domain names in it finds only `netray.info`.

### Behavioural verification

skipped: prose only.
