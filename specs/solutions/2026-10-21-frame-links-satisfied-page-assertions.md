---
class: check-passes-without-observing
repeat-of: specs/solutions/2026-10-21-nothing-lost-needles-matched-other-text.md
---
# Page assertions passed on links the site frame carries

**What failed.** Once every page carried the Inspectors menu and the full footer, whole-page needles
found the same links there: `site_nav_legend.rs` c3 (`<a href="/inspect/email">Email</a>` in the
home inspector line) passed before the line linked Email, because the footer does; the lens test
`result_mount.rs` recognised the idle page by `/inspect/dns`, which the 404 page now carries; the
featured-guides check of `site_nav_header.rs` read on into the footer's Guides link.

**What worked.** Scoping each needle to its element: the inspector line from "One protocol only:" to
its `</p>`, the featured guides up to `</main>`, the idle page by its own sentence.

**How to notice next time.** A test written while the frame changes passes before the behaviour it
names exists.

**Check to build.** A page test that asserts an `href` the frame (`templates/base.html` nav or
footer) also renders must slice the page first; a repo check could grep tests under
`crates/web/tests/it` for `body.contains(...href="/inspect/` or `href="/guide/` on an unsliced body.
