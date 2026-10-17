# Plan: v2-pages

## Phase 1 — 4a deferrals

### Groups

G1: C1, C4 · G2: C2, C5 · G3: C3, C6, C7, C8. No shared production file, no dependency between them.

### Plan

#### G1
1. `crates/web/build.rs` — in `main`, after the `[data-theme="dark"]` rule and before the phone media block, `block(&mut css, "", "[data-theme=\"light\"]", Some("light"), &light);` (`light` already holds every `:root` value, `none` per `status.glow.*`).
2. `crates/web/src/tokens.rs` — doc comment on `CSS` names the light rule.
3. `specs/sdd/v2.md` — §8.2's build.rs sentence adds the light values under `[data-theme="light"]` after the dark media block (same commit as build.rs).

#### G2
1. `crates/web/static/logo.svg` — root `id="netray-logo"`, selectors `#netray-logo text`, `#netray-logo text, #netray-logo rect` (both outside and inside the dark media block).
2. `crates/web/static/favicon.svg` — root `id="netray-favicon"`, `#netray-favicon path`.
3. `crates/web/static/badge-mark.svg` — root `id="netray-badge-mark"`, `#netray-badge-mark .tile`, `#netray-badge-mark .mark`.
Colours and geometry byte-identical.

#### G3
1. `crates/web/src/gate.rs`:
   - `parse_attrs` takes `askama: &[(usize, usize)]`; in the unquoted-value branch an askama span starting at the value position (only whitespace between `=` and the span start) is the value, and parsing resumes after it.
   - `tags` takes and forwards the spans; `scan_template` passes its `askama` vec; `data_uri_colours` passes `&[]`.
   - `has_src`'s second disjunct (askama src parsed as the next token) becomes dead and goes.
   - The handler message already names the attribute.
