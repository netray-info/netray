---
class: check-passes-without-observing
repeat-of: specs/solutions/2026-10-17-persistence-test-could-not-fail.md
---
# A page test that a component is present passed on the inlined stylesheet

**What failed.** `crates/web/tests/entries_diff.rs` asserted `html.contains("diff-block")` for "the diff block is present" and `!html.contains("diff-block")` for "absent"; `entries_page.rs` asserted `!html.contains("grade__score")` for "no score". The entry, gone and privacy pages inline every component stylesheet (`crates/web/src/entries/render.rs`), and `static/diff_block.css` holds `diff-block` 14 times: the presence check passed with no diff block rendered, and both absence checks failed on correct pages.
**What worked.** Search the markup: `html.contains(r#"class="diff-block""#)` and `r#"class="grade__score""#` (commit c977ddb, `ADLC-Test-Change`).
**How to notice next time.** A page test that greps a bare component class name while the page carries a `<style>` element.
**Check to build.** A repo check over `crates/web/tests/*.rs` refusing `contains("<component class>")` for the ten R44 class roots unless the literal starts with `class="` (in `tests/repo/`, next to `test_web_crate.sh`).
