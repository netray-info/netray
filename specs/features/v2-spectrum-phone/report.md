# Report: v2-spectrum-phone

## Phase 1 — Phone placement and drawing

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1 phone placement: layout places each run's labels at the 358 px phone plate, own pitch, phone caption, same rule; each run carries phone labels and rows | green | crates/web/tests/it/spectrum_phone.rs |
| C2 | R2 phone drawing: at 480 px or narrower labels, leaders and band read the phone placement; wider unchanged | green | crates/web/tests/it/spectrum_phone.rs |
| C3 | R3 a run clear at the desktop placement is clear at the phone placement on every history fixture; SDD 5.5 amended | green | crates/web/tests/it/spectrum_phone.rs |
| C4 | R4 screenshots at 1280 and 390, light and dark, of the dense fixture result pages; design-check run, board M deviations logged as proposals | green | behavioural verification |
| C5 | S 57 lines, one failing: plate 358, pitch 358/57, box 0.62 em x 10 px per char plus 4, at its leader | green | crates/web/tests/it/spectrum_phone.rs |
| C6 | S any 2 to 60 lines placed for the phone: every flagged line labelled, rows at least three, rows disjoint, crossings only in line-order clusters | green | crates/web/tests/it/spectrum_leaders.rs |
| C7 | S each fixture: run phone labels and rows equal the phone placement of its tones | green | crates/web/tests/it/spectrum_phone.rs |
| C8 | S each fixture: clear at desktop means clear at phone; only the two dense example.com fixtures cross, at both widths | green | crates/web/tests/it/spectrum_phone.rs |
| C9 | S HSTS, XFO, XCTO warn: desktop placement drawn at the phone crosses, phone placement does not | green | crates/web/tests/it/spectrum_phone.rs |
| C10 | S example.com spectrum: root carries phone extra rows; labels and leaders carry phone rows, labels phone width and leader position | green | crates/web/tests/it/spectrum_phone.rs |
| C11 | S slim inspector band root carries no phone rows | green | crates/web/tests/it/spectrum_phone.rs |
| C12 | S stylesheet at 480 px or narrower: label bottom and clamp, leader height, band height read the phone values | green | crates/web/tests/it/spectrum_phone.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 spectrum, component, stylesheet, SDD | 0 | orchestrator | 0 | 0 |

The change is one placement function, one view field pair and four phone rules, built in one pass by the orchestrator rather than by a coder; the tests were written beside the spec before it (commit d3bb72eb carries the baseline).

### Behavioural verification

The worktree binary served the seeded history fixtures (`netray lens`, port 18090); the main checkout's binary (the desktop placement drawn at the phone) served the same database on 18092. A Playwright script measured, in the shown label set, every leader whose x lies more than half a px inside another label's text (its box less the 4 px padding) and rises above that label's bottom, and every overlap of two label boxes; screenshots of `#spectrum` at 1280 and 390 px, light and dark.

```
before (main) 390 light: example.com 18 crossings, overlap HSTS/XFO; shop.example.com 4 (ECH>HSTS, DKIM>DMARC, DKIM>MTA-STS, DMARC>MTA-STS); checkout.eu-central.storefront.example.com 1 (ECH>HSTS); new.example.com 1 (ECH>HSTS)
after 390 light and dark: shop.example.com 0, checkout... 0, new.example.com 0; example.com 18 (the dense HTTP cluster, line-order fallback), no overlap
after 1280 light and dark: shop 0, checkout 0, new 0; example.com 7 (the same dense cluster, unchanged from main)
scrollWidth equals the viewport on every capture; plate 358 px at 390
label band at 390: example.com 70 px (five phone rows), the others 46 px, as on main
```

`just design-check` (worktree build copied to the main checkout's binary path, restored after; NETRAY_BIN set): exit 0, 0 open deviations, 0 stale accepted entries.

On shop.example.com at 390 the MTA-STS box is pushed in from the plate's right edge (its line at 317.2 px plus 47.4 px passes 358 px) and DMARC's leader passes through its leading padding, 3.2 px left of the text; the measure above counts text, so it is no crossing. The desktop box is not shifted (its line at 404 px plus 51.7 px ends inside 456 px).

### Review

| class | finding | disposition |
|---|---|---|
| AMENDMENT | the SDD's "a run clear at the desktop placement is clear at the phone" fails where a cluster is dense only at the phone (Perms, COOP, MX, DKIM) | repaired in phase: SDD §5.5 and requirement 3 narrowed to the fixtures; proposal for the phone fallback threshold |
| AMENDMENT | the edge exemption ("the edge rarely holds two labels") reaches the last seven or eight lines at the phone; two shifted boxes cover each other's leaders (MTA-STS, TLS-RPT) | repaired in phase: SDD §5.5 names it; no row order clears mutual covers; proposal |
