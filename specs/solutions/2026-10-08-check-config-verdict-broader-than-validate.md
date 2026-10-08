---
class: verdict-broader-than-the-check
repeat-of: none
---
# `--check-config` said "config ok" for configs that crash at startup

**What failed.** `netray <sub> --check-config` printed `config ok` after `Config::load` + `validate()`, but `validate()` covered a hand-picked list. Three review passes on monorepo-p3 Phase 1 each found more values startup refuses: unparsable bind addresses (ifconfig-rs `lib.rs:247`, beacon `lib.rs:141`), lens `badges.ttl_seconds = 0` (`state.rs:75`), then an invalid OTLP endpoint in all six subcommands (`common/src/telemetry.rs:135`).
**What worked.** An inventory of every startup site that can fail on a config value (`.expect`, `panic!`, `NonZero`, `parse::<SocketAddr>`, telemetry init), per service, then one `startup_rejects` / `telemetry_rejects` row per site in `tests/repo/test_check_config.sh` and a `validate()` rule for each (commits `3544642`, `d2c4218`).
**How to notice next time.** A check whose verdict is "ok" for a whole artefact while its implementation enumerates cases is broader than what it checks; inventory the failure sites before writing it.
