# Report: v2-inspectors

## Phase 1 — One renderer over the six shapes

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1: one function `Block` → `EvidenceBlockView` matching on `Shape` only, six shapes, typed value formatting, no module/block name or key in the renderer | green | crates/web/tests/evidence_render_shapes.rs, evidence_render_values.rs |
| C2 | R2: `evidence_block` carries no `style` attribute; tree depth `data-depth` 0–7 (clamped) styled by `evidence_block.css`; `id="ev-<block id>"`; values escaped | green | crates/web/tests/evidence_render_depth.rs, evidence_render_escape.rs |
| C3 | S: block of each shape → `data-shape`, `id="ev-<block id>"`, every value in order | green | crates/web/tests/evidence_render_shapes.rs |
| C4 | S: fields `int 3`, `bool true`, `time`, `duration 120`, `addr 2001:db8::1` → `3`, `true`, `2026-10-08T14:32:00Z`, `120 ms`, `2001:db8::1` | green | crates/web/tests/evidence_render_values.rs |
| C5 | S: tree of depth 9 → `data-depth` 0..7, deepest two clamped, no `style` attribute | green | crates/web/tests/evidence_render_depth.rs |
| C6 | S: text block `<script>alert(1)</script>` → escaped | green | crates/web/tests/evidence_render_escape.rs |
| C7 | S: renderer source holds no block name (`dns.`, `tls.`, `http.`, `email.`, `ip.`) | green | crates/web/tests/evidence_render_source.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 1 | sonnet | 42817 | 54 |

### Review

Reader: no findings.

### Behavioural verification

skipped: Phase 1 adds a library function and a macro change, no route or command; the rendered markup is asserted by the phase's tests.

### Verify

`adlc verify`: verified, 237 tests collected.
