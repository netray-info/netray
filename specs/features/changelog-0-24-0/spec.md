# Spec: changelog 0.24.0

Status: Done
Created: 2026-10-10
Finished: 2026-10-10

## Goal

`CHANGELOG.md`'s `## [Unreleased]` names every breaking change of 0.24.0 a self-hoster must act on, so `just release 0.24.0` publishes complete release notes.

## Non-goals

- Code or config changes.

## Context and constraints

- `[Unreleased]` already names the env prefixes (old ones refuse startup), `[modules.*]` with `[backends.<p>] url` and `[backends] dns_servers` refused, `resolve_timeout_ms`, and that lens loads the IP data (features v2-modules, lens-check-config-data).
- Missing: that lens's deployment must mount the data directory (`/netray/data` in the image layout) and that `netray lens --check-config` no longer reads data files (lens-check-config-data).

## Requirements

1. The IP-data entry says the lens container needs the same read-only data mount as `netray ip` (`/netray/data`).
2. A `### Changed` entry says `netray lens --check-config` validates configuration only and no longer reads the IP data files; startup refuses a missing or unparsable file, naming its key.

## Phase 1 — Release notes

**Depends on:** none
**Requirements:** 1, 2

### Test Scenarios

- GIVEN `CHANGELOG.md` WHEN `[Unreleased]` is read THEN it names `/netray/data` for lens and the `--check-config` change.

## Decision log

- Add the two lines before `just release 0.24.0` (operator, 2026-10-10).

## Open decisions

None.

## Out of scope

- The release itself (`just release`, operator's push).
