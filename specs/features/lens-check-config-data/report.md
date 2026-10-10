# Report: lens check-config without data

## Phase 1 — Config-only check

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1: `--check-config` validates `[modules.*]` (unknown keys, both GeoIP keys, values) without checking or loading data files | green | tests/repo/test_check_config.sh |
| C2 | R2: startup refuses a configured GeoIP database that does not exist, naming the key | green | tests/repo/test_check_config.sh |
| C3 | R3: CLAUDE.md records the data-file exception | green | CLAUDE.md |
| C4 | both GeoIP keys at missing paths → `--check-config` exit 0 | green | tests/repo/test_check_config.sh |
| C5 | the same config → `netray lens` exits non-zero naming `geoip_city_db` | green | tests/repo/test_check_config.sh |
| C6 | one GeoIP key only, or an unknown key → exit 1 naming it | already_implemented | tests/repo/test_check_config.sh |

RED (69adbf4): `--check-config` with missing GeoIP files exited 1.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 2 (fmt) | sonnet | — | — |

### Behavioural verification

`just adlc-verify` green; `test_check_config.sh` runs both the config check (exit 0) and lens startup (refuses, naming `geoip_city_db`) on a config with missing GeoIP files.
