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

## Phase 2 — Evidence view model

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R3: view of `p` = run line; one check row per catalogue check of `p` in catalogue order (short, tone, title, findings or catalogue finding, fix_hint on warn/fail, guide link, one link per referenced block); owned blocks in run order with feeds; `not_applicable` reason above rows | green | crates/web/tests/evidence_section_rows.rs |
| C2 | R4: a block of another module is not rendered; a referencing check yields one line with the block's title linking to the owner's view `#ev-<block id>` | green | crates/web/tests/evidence_section_shared.rs |
| C3 | R5: run line `run of <YYYY-MM-DD HH:MM> UTC · entry <id>` (created_at); live incomplete `incomplete, not recorded` | green | crates/web/tests/evidence_section_runline.rs |
| C4 | R7: the section renderer takes a live run and renders the same markup as the stored view | green | crates/web/tests/evidence_section_href.rs |
| C5 | S: `tls.chain_trusted` referencing `tls.chain@192.0.2.1:443` → row links `#ev-tls.chain@192.0.2.1:443`, block feeds list its short | green | crates/web/tests/evidence_section_rows.rs |
| C6 | S: `tls.caa_compliant` referencing `dns.caa` → not rendered in TLS view, one line links DNS view `#ev-dns.caa`; DNS view renders it | green | crates/web/tests/evidence_section_shared.rs |
| C7 | S: warn check without findings → body catalogue `finding`, fix catalogue `fix_hint` | green | crates/web/tests/evidence_section_bodies.rs |
| C8 | S: stored check without findings/evidence → catalogue texts, no block link | green | crates/web/tests/evidence_section_bodies.rs |
| C9 | S: TLS section `not_applicable` "no A or AAAA record" → reason shown, every TLS row `untested` | green | crates/web/tests/evidence_section_scope.rs |
| C10 | S: entry `Ab3dE5fG` created 2026-10-08 14:32 UTC → `run of 2026-10-08 14:32 UTC · entry Ab3dE5fG`; live incomplete → `incomplete, not recorded` | green | crates/web/tests/evidence_section_runline.rs |
| C11 | S: check rows follow catalogue order and cover every check of the protocol | green | crates/web/tests/evidence_section_scope.rs |
| C12 | S: `href("Ab3dE5fG", Tls, Some(tls.chain@192.0.2.1:443))` → `/r/Ab3dE5fG/tls#ev-tls.chain@192.0.2.1:443` | green | crates/web/tests/evidence_section_href.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 2 | sonnet | 50083 | 63 |
| G1 (review fix) | 1 | sonnet | 32049 | 34 |

### Review

Pass 1: BLOCKER feeds listed only the view's own checks (fixed: every referencing check of any protocol); AMENDMENT path-relative `<owner>#ev-` links break from `/<domain>` and re-read the latest entry (repaired in phase: `entry_section` links `/r/<id>/<owner>#ev-`, the live `section` links `#ev-` on the same page; I8 and the decision log amended); DEFERRED same as the amendment for `/<domain>/<p>` (resolved by it). Pass 2: no findings.

Test fixes: `evidence_section_rows.rs`/`evidence_section_href.rs` `created_at` was 2026-05-08 17:44, not the asserted 2026-10-08 14:32.

### Behavioural verification

skipped: Phase 2 adds pure view-model functions and a partial, no route; Phase 4 serves them.

### Verify

`adlc verify`: verified.
