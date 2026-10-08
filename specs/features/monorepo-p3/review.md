
## main..c620c7c

### Reader

COUNTS blockers=3 majors=1 minors=0
LENSES Engineering, Security, Testing
BLOCKER | crates/ifconfig-rs/src/config.rs:225 | `netray ip --check-config` prints `config ok` and exits 0 for a bind address that startup rejects. `validate()` never parses `server.bind` or `admin_bind`, and both are `String` fields. | Ran it. I took the ifconfig production fixture and set `bind = "0.0.0.0"`. `--check-config` printed `config ok` and exited 0. `netray ip` on the same file panicked at crates/ifconfig-rs/src/lib.rs:247 with `Invalid bind address: AddrParseError(Socket)` and exited 101.
BLOCKER | crates/beacon/src/config.rs:171 | beacon's `validate()` checks only `rate_limit.per_ip`. So `netray email --check-config` passes an unparsable `server.metrics_bind` or `server.bind` (both `String`), and startup refuses it. This contradicts commit c620c7c's claim to cover what beacon refuses at startup. | Ran it. I took the beacon production fixture and set `metrics_bind = "0.0.0.0"`. `--check-config` printed `config ok` and exited 0. `netray email` on the same file panicked at crates/beacon/src/lib.rs:141 with `invalid metrics_bind address` and exited 101.
BLOCKER | crates/lens/src/config.rs:440 | lens's `validate()` does not reject `badges.ttl_seconds = 0`, and `AppState::new` panics on that value. So `netray lens --check-config` approves a config that crash-loops, against R3's "every subcommand's validate() covers what its startup rejects". | Ran it. I added `[badges]` / `ttl_seconds = 0` to the lens production fixture. `--check-config` printed `config ok` and exited 0. `netray lens` on the same file panicked at crates/lens/src/state.rs:75 with `badge ttl_seconds must be non-zero` and exited 101.
MAJOR | crates/tlsight/src/config.rs:342 | The `custom_ca_dir` existence check now sits in `validate()`, which tlsight's SIGHUP reload calls through `Config::load`. A missing CA directory therefore aborts the whole reload: the old custom CAs stay trusted, every other config edit is dropped, and the reload path's tolerant branch at crates/tlsight/src/state.rs:161 can no longer be reached. | Traced through the code, not run. tlsight runs with `custom_ca_dir = "/etc/tlsight/cas"`. The operator deletes that directory to stop trusting its CA, changes `limits.per_ip_per_minute` and sends SIGHUP. Before this change: the trust store was rebuilt from the Mozilla roots only, a warning was logged, and the new limit applied. After: crates/tlsight/src/reload.rs:49 gets `Err` from `Config::load` and returns. The removed CA stays in the trust store and the old limit stays in force.

```quote crates/ifconfig-rs/src/config.rs:225
    pub fn validate(&self) -> Result<(), config::ConfigError> {
```

```quote crates/ifconfig-rs/src/lib.rs:247
    let bind_addr: SocketAddr = config.server.bind.parse().expect("Invalid bind address");
```

```quote crates/beacon/src/config.rs:171
    fn validate(&self) -> Result<(), config::ConfigError> {
```

```quote crates/beacon/src/lib.rs:141
        .expect("invalid metrics_bind address");
```

```quote crates/lens/src/config.rs:440
    pub fn validate(&mut self) -> Result<(), ConfigError> {
```

```quote crates/lens/src/state.rs:75
            .expect("badge ttl_seconds must be non-zero")
```

```quote crates/tlsight/src/config.rs:342
        if let Some(dir) = &self.validation.custom_ca_dir
```

```quote crates/tlsight/src/reload.rs:49
    let new_config = match Config::load(config_path) {
```

```quote crates/tlsight/src/state.rs:161
            tracing::warn!(dir = dir, "custom_ca_dir not found during reload");
```

### Refuted

None.

### Calibration

| Finding | Refuter | Confidence | Result | Command |
|---|---|---|---|---|
| F1 ifconfig-rs bind not validated | CONFIRMED | 9 | held | `netray ip --check-config` on fixture with `bind = "0.0.0.0"` → `config ok`, rc 0 |
| F2 beacon metrics_bind not validated | CONFIRMED | 8 | held | `netray email --check-config` on fixture with `metrics_bind = "0.0.0.0"` → `config ok`, rc 0 |
| F3 lens `badges.ttl_seconds = 0` not validated | CONFIRMED | 9 | held | `netray lens --check-config` on fixture plus `[badges] ttl_seconds = 0` → `config ok`, rc 0 |
| F4 tlsight reload drops the whole config on a missing `custom_ca_dir` | CONFIRMED | 8 | held | read `crates/tlsight/src/reload.rs:49-56` and `state.rs:155-161` |

### Summary

Before refutation: 3 blockers, 1 major, 0 minors. After: 3 / 1 / 0. Verified 4, held 4. No `[principles]` declared, no roll call.

## c620c7c..3544642

### Reader

COUNTS blockers=1 majors=0 minors=1
LENSES Engineering, Security, Testing
BLOCKER | crates/common/src/telemetry.rs:135 | `netray <sub> --check-config` still prints `config ok` and exits 0 for a config that startup refuses. The commit and spec R3 promise that check-config rejects "every value startup rejects", but no `validate()` checks the OTLP endpoint, and startup panics on a bad one. | I confirmed this on the existing `target/debug/netray`. I took `crates/beacon/tests/fixtures/beacon.production.toml` and added `otlp_endpoint = "http://bad host:4318"` under `[telemetry]`. `netray email --check-config` then returned rc=0, while `netray email <same file>` returned rc=101 with `failed to initialize OpenTelemetry: InvalidUri("http://bad host:4318", "invalid uri character")`. Doing the same to the lens fixture (adding `enabled = true` plus the bad endpoint) also returned rc=0. The panic site is shared code that all six subcommands use, so a config that crash-loops passes the operator check on every one of them.
MINOR | crates/tlsight/src/config.rs:276 | The new `check_startup` copies only the `is_dir` half of the startup check. Startup also panics when `read_dir` fails (`crates/tlsight/src/state.rs:89`), so `--check-config` passes a CA directory that startup refuses. | I confirmed this on the binary. I set `custom_ca_dir` to an existing directory with mode 000. `netray tls --check-config` returned rc=0 (`config ok`), while `netray tls <same file>` returned rc=101 with `failed to read custom_ca_dir …: Permission denied (os error 13)`. I rate this minor because spec R3 commits only to a *missing* CA directory and puts host-side file state out of scope.

```quote crates/common/src/telemetry.rs:135
        let otel_layer = init_otel_layer(config).expect("failed to initialize OpenTelemetry");
```

```quote crates/tlsight/src/config.rs:276
            && !std::path::Path::new(dir).is_dir()
```

```quote crates/tlsight/src/state.rs:89
        .unwrap_or_else(|e| panic!("failed to read custom_ca_dir {ca_dir}: {e}"));
```

### Refuted

None.

### Calibration

| Finding | Refuter | Confidence | Result | Command |
|---|---|---|---|---|
| F1 invalid OTLP endpoint passes `--check-config`, panics at startup | CONFIRMED | 9 | held | `netray email --check-config` on beacon fixture with `otlp_endpoint = "http://bad host:4318"` → `config ok`, rc 0 |

### Summary

Before refutation: 1 blocker, 0 majors, 1 minor. After: 1 / 0 / 1. Verified 1, held 1. No `[principles]` declared, no roll call. The minor (tlsight CA dir readable, not only present) is host state, outside R3.

## 3544642..d2c4218

### Reader

COUNTS blockers=0 majors=0 minors=1
LENSES Engineering, Testing

MINOR | tests/repo/test_check_config.sh:114 | The `grep 'otlp_endpoint'` check is meant to stop a row from passing on an unknown-field error (the comment at lines 93-94 says so), but it cannot catch that case. serde's unknown-field message lists every expected field, and `otlp_endpoint` is one of them. | A beacon row with `enabled = true` injected exits 1 with "unknown field `enabled`, expected one of `log_format`, `otlp_endpoint`, …" and passes both checks; the six committed rows avoid this, so each does exercise its service's validate wiring today.

```quote tests/repo/test_check_config.sh:114
    grep -q 'otlp_endpoint' <<<"$out" || fail "$sub: invalid otlp_endpoint error does not name otlp_endpoint ($out)"
```

Traced and sound: `validate` parses with the same `http` 1.5.0 `Uri` parser opentelemetry-otlp 0.31.1 uses; all six `Config::load` validate before `init_subscriber` (also `ip --check`, SIGHUP reloads); `otlp_endpoint = ""` with telemetry enabled is now refused.

### Summary

Before refutation: 0 / 0 / 1. After: 0 / 0 / 1. Verified 0, held 0 (no blocker or major). No roll call. The minor is repaired: the check now greps for `telemetry.otlp_endpoint`, which the unknown-field message never contains.

Measurement note: the review row for `3544642..d2c4218` was recorded with insertions 0 / deletions 0; the range is 89 / 1.
