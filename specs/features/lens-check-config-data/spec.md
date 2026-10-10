# Spec: lens check-config without data

Status: Done
Created: 2026-10-10
Finished: 2026-10-10

## Goal

`netray lens --check-config` validates lens's configuration without touching the IP data files, as `netray ip --check-config` does: a config naming both GeoIP databases passes on a machine without them, while lens's startup still refuses a configured file that does not load.

## Non-goals

- Any change to startup behaviour or to the other services' `--check-config`.

## Context and constraints

- argus-oci runs every `--check-config` (K4) with the native binary on the operator's machine, which has no GeoLite data; 0.24.0's lens check fails there with `modules.ip: geoip_city_db: no file at /netray/data/GeoLite2-City.mmdb` (planning session, 2026-10-10).
- `crates/netray/src/main.rs` (~205–231): the check path builds the full registry, so it runs the file check and `IpModule::new` (loads the databases).
- CLAUDE.md "Startup rejects are checked" requires config refusals at startup to fail `--check-config` too; data files are the exception this spec records.

## Requirements

1. `netray lens --check-config` parses and validates every `[modules.*]` table (unknown keys, the both-GeoIP-keys rule, value checks such as `resolve_timeout_ms` below the IP timeout) but neither checks that data files exist nor loads them; it builds no IP module.
2. `netray lens` (startup) still refuses a configured GeoIP database that does not exist or fails to load, naming the key.
3. CLAUDE.md records the exception: data files are checked at startup, not by `--check-config`.

## Phase 1 — Config-only check

**Depends on:** none
**Requirements:** 1, 2, 3

### Test Scenarios

- GIVEN a lens config whose `[modules.ip]` names both GeoIP databases at paths that do not exist WHEN `netray lens --check-config` THEN exit 0.
- GIVEN the same config WHEN `netray lens` starts THEN it exits non-zero naming `geoip_city_db`.
- GIVEN a `[modules.ip]` naming only one GeoIP key, or an unknown key WHEN `--check-config` THEN exit 1 naming it (unchanged).

## Decision log

- `--check-config` validates config only, as `netray ip --check-config` does; the host's startup, health wait and rollback catch a missing data file (operator, 2026-10-10, on argus-oci's K4 finding). Amends v2-modules R10.

## Open decisions

None.

## Out of scope

- argus-oci's deploy script.
