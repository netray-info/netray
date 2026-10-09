# Report: backend correctness

## Phase 4 — `@system`

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | Req 6: `/api/config` reports whether system resolvers are allowed; the UI lists and documents `@system` only when it said so (hidden before the answer and on failure); the server keeps refusing it; the production fixture sets `[dns] allow_system_resolvers = false` | green | covered by C2–C7 |
| C2 | GIVEN `allow_system_resolvers = false` WHEN `/api/config` is read THEN it reports system resolvers off | green | crates/mhost-prism/tests/system_resolvers.rs |
| C3 | GIVEN the config off WHEN the query input suggests servers and the help opens THEN no `@system` suggestion and no help row | green | crates/mhost-prism/frontend/src/lib/servers.test.ts |
| C4 | GIVEN the config on WHEN the same THEN both appear | green | crates/mhost-prism/frontend/src/lib/servers.test.ts |
| C5 | GIVEN `/api/config` not yet answered or failing WHEN the input renders THEN no `@system` suggestion | green | crates/mhost-prism/frontend/src/lib/servers.test.ts |
| C6 | GIVEN `allow_system_resolvers = false` WHEN a query names `@system` THEN `SYSTEM_RESOLVERS_DISABLED`, as today | already_implemented | crates/mhost-prism/tests/system_resolvers.rs |
| C7 | GIVEN `prism.production.toml` WHEN loaded THEN `allow_system_resolvers` is false | green | crates/mhost-prism/tests/production_fixture.rs |
| C8 | GIVEN `allow_system_resolvers = false` WHEN `POST /api/parse` completes `example.com @sy` or `example.com ` THEN no `@system` completion; GIVEN it true THEN `@system` is offered (added by the operator's amendment of req 6) | green | crates/mhost-prism/tests/parse_system.rs |

C6 held before the phase; its test `system_server_is_refused_when_system_resolvers_disabled` (GET `@system` and POST `"system"`) stays as a pin. C3–C5 test the pure helpers in `frontend/src/lib/servers.ts`, which `QueryInput` and the help modal now render from: prism's vitest runs in `node` without jsdom.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 (C2) + G2 (C7), one coder | 1 | sonnet | 17042 | 19 |
| G3 (C3, C4, C5) | 1 | sonnet | 25132 | 41 |
| G5 (C8) | 1 | sonnet | 18519 | 25 |

### Review

- BLOCKER (fixed) | `crates/mhost-prism/frontend/src/components/QueryInput.tsx:4` | the `@codemirror/autocomplete` import was corrupted by an orchestrator re-indent edit after the coder's green run; `tsc` failed and the embedded dist would have stayed stale. Restored; `tsc --noEmit` clean, vitest 72/72, `npm run build` green, the rebuilt bundle reads `allow_system_resolvers`.
- DEFERRED (raised as BLOCKER, refuted) | `crates/mhost-prism/frontend/src/lib/servers.test.ts:2` | C3–C5 test the pure helpers, not `QueryInput`/`App` rendering, and the gate runs no `tsc` on prism's frontend. Not a wrong result: the pure-function test was chosen because prism's vitest has no jsdom; a rendering test belongs with the jsdom setup another branch adds. The gate not type-checking the frontend is a gate gap outside this phase.
- AMENDMENT | `crates/mhost-prism/src/api/parse.rs:89` | `POST /api/parse` (public: "Parse a query string and get completions. Useful for building UIs.") always offers `@system` as a completion; its handler takes no `State`. With the production config, `{"input":"example.com @sy"}` completes to `@system`, which the query then refuses. Requirement 6 named only `/api/config` and the UI. | affected_phase: 4 | repaired_in_phase: yes — the operator amended req 6 (2026-10-09, `a427d44`); C8 covers it, and `parse_handler` takes `State` and drops `@system` when disallowed.
- Second reading, over the `/api/parse` diff: no BLOCKER, no AMENDMENT. The filter works on constant labels, so `@System`/`@SY` still yield and lose `@system`; `@public`/`@all` never expand to system resolvers; `completions_at` has no other caller; completion and refusal read the same `state.config` field.
- DEFERRED | `crates/mhost-prism/src/api/parse.rs:264` | older than this phase: `cursor_pos` is a byte offset clamped only to `input.len()`, so a cursor inside a multi-byte character panics on the slice and drops the connection (`{"input":"example.com @é","cursor_pos":14}`).
- Traced sound by the reader: hot reload (both `/api/config` and `QueryPolicy` read the startup `state.config`); Solid props are read lazily in the completion source; `For` over `helpServerRows(allowSystem())` is reactive; the signal starts false and a failed fetch leaves it false; no other place lists `@system` (frontend, README, site/, `/api/servers`).

### Behavioural verification

`netray dns` with the production fixture, then with defaults (`PRISM_SERVER__BIND=127.0.0.1:18089`):

```
--- netray dns crates/mhost-prism/tests/fixtures/prism.production.toml
$ curl -s localhost:18089/api/config
{"site_name":"prism","version":"0.22.2","ifconfig_url":"https://ip.example.com","tls_url":"https://tls.example.com","allow_system_resolvers":false,"ecosystem":{...}}
$ curl -s "localhost:18089/api/query?q=example.com%20A%20@system"
{"error":{"code":"SYSTEM_RESOLVERS_DISABLED","message":"system resolvers disabled"}}
--- netray dns (defaults)
$ curl -s localhost:18089/api/config
{"site_name":"prism","version":"0.22.2","ifconfig_url":null,"tls_url":null,"allow_system_resolvers":true}
```

`/api/parse` after the amendment, same two configs:

```
--- netray dns crates/mhost-prism/tests/fixtures/prism.production.toml
$ curl -s -X POST -H 'content-type: application/json' -d '{"input":"example.com @sy"}' localhost:18089/api/parse | jq -c '[.completions[].label]'
[]
--- netray dns (defaults)
$ curl -s -X POST -H 'content-type: application/json' -d '{"input":"example.com @sy"}' localhost:18089/api/parse | jq -c '[.completions[].label]'
["@system"]
```

After `npm run build` and a rebuild, the served bundle `/assets/index-D1f3bPxI.js` contains `allow_system_resolvers===!0`, the strict check in `systemResolversAllowed`. No headless click: this worktree has no Playwright install (`tests/acceptance` has no `node_modules`), so the suggestion list and the help rows are verified through `servers.test.ts` only.
