# Contributing to netray

netray is maintainer-led: the maintainer decides scope, design and what is merged.

Issues are welcome: bug reports, wrong results, unclear docs.

Pull requests by arrangement: open an issue first and wait for the go-ahead.

Self-hosting is not supported. The code is open source; netray.info is the only supported deployment.

MIT, see `LICENSE`.

## Working on the code

- `just adlc-setup` once per checkout (npm workspaces, frontend builds).
- `just check` runs the gate plus the full Rust suite.
- ifconfig-rs data-dependent tests need `just ifconfig-data` first, then `just test-ifconfig-data`.
