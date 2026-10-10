---
class: check-passes-without-observing
repeat-of: specs/solutions/2026-10-08-text-checks-pass-without-observing.md
---
# Byte-identical goldens passed while the in-process move regressed production behaviour

**What failed.** v2-modules moved lens's five HTTP backends in-process. Every phase kept `lens-*.json` and the full-output goldens byte-identical, yet the phase readers found production regressions the goldens could not see, because golden modules replay a contract and never touch config, limits or input:
- HTTP lost spectra's per-target limit.
- Email lost beacon's DKIM selector cap and domain parse; a sixth selector was silently never queried.
- A data-less `[modules.ip]` passed reputation for Tor exits.
- TLS charged the per-target limit before the target policy, refusing large hosts forever.
- The engine's resolve stage shortened every section's window.
- The facts' owner filter never matched a live, unqualified query name.

**What worked.** After every phase, a reader got the explicit question "what did the service route apply to lens's calls that the module drops, and what does production do with this config", plus the earlier readers' finding class. Each finding got a test (module_target_limit.rs, module_input.rs, test_check_config.sh rows, engine run.rs timing cases) before the repair.

**How to notice next time.** When a change keeps results equal by replaying recorded inputs, ask what the replay skips: config loading, limits, validation and timing are invisible to it.
