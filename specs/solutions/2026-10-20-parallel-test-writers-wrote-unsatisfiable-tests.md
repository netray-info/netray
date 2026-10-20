---
class: structurally-unsatisfiable-gate
repeat-of: none
---
# Tests written in parallel against a markup contract can be unsatisfiable by any implementation

**What failed.** In v2-inspector-shell, test writers working from a prose markup contract wrote
assertions no markup could meet: `split('>').nth(1)` on an anchor whose span holds a `>`
(`shell_run_head.rs`), `" · "` between run-title parts that are separate spans (`shell_shared.rs`),
byte-stepping a slice over a multibyte `×` (`shell_frame.rs`), one `<dialog` where the page already
had the share dialog (`shell_pages.rs`), quotes the SSE JSON escapes (`shell_email.rs`). Each cost
a coder run spent before the coder reported the contradiction.
**What worked.** The coder names the contradicting assertion instead of editing the test; the
orchestrator fixes the test under `ADLC-Test-Change`, and the next coder run goes green.
**How to notice next time.** A coder green on everything but one assertion that reads markup
through string splitting is looking at a test defect, not a code defect.
