---
class: check-passes-without-observing
repeat-of: specs/solutions/2026-10-15-render-tests-saw-classes-not-drawing.md
---
# A line-based grep guard passed a grouped import that rustfmt wraps over several lines

**What failed.** `tests/repo/test_api_no_verdict.sh` matched `netray_engine::\{[^}]*\bscore\b` with `grep -rnE`, one line at a time. A rustfmt-wrapped `use netray_engine::{\n    …,\n    score,\n};` puts `{` and `score` on different lines, so a scoring call under `crates/web/src/api/` passed the guard; its self-test planted only the one-line form (branch review of v2-api, `cd0c2a12..eaaf3037`).
**What worked.** A second, whole-file scan (`perl -0777` over each file, the same pattern with `/s`) and a `planted5` fixture with the wrapped form (061ab907).
**How to notice next time.** A guard over Rust source whose pattern spans a `{…}` list and whose fixtures hold only one-line forms.
**Check to build.** `test_api_no_verdict.sh` scans every grouped-import form it bans across lines, `netray_model::diff::{…, diff}` included, with a wrapped fixture per form.
