# CLAUDE.md — acceptance suite

Playwright against a deployed environment (own `package-lock.json`). Repo-wide rules: the root [`CLAUDE.md`](../../CLAUDE.md).

- `just acceptance-local` starts one `netray serve` on a free port with apex `localhost` (`LOCAL_URL`, `LOCAL_APEX`; inspector hosts `<tool>.localhost`); never assume 8000 or 8080 are free.
- It seeds a temporary `history.db` from `fixtures/history/` (`cargo run -p netray-store --example seed`); a fixture is `{"domain","age_seconds","run"}` with a complete run holding every catalogue ID once, never an absolute time.
- Spec paths given to it replace the default list; `@live` tests run only with `--grep @live`. Pass extra config keys as `ACCEPTANCE_ENV="NETRAY_…"` (`serve` refuses legacy-prefixed variables).
- Specs read `/api/v1` once per worker (module-level cache): a read per test hits the per-client limit and answers `429`.
- A test serving fixtures from its own process spawns children with async `spawn`: `spawnSync` blocks the event loop.
