---
class: check-passes-without-observing
repeat-of: specs/solutions/2026-10-08-text-checks-pass-without-observing.md
---
# Release guards were tested by text patterns, so a guard that blocked every release passed its test

**What failed.** The `release.yml` step meant to prove the image carries no data ended in `[ -d /netray/data ] && find …`. On a clean image that returns 1, and GitHub's `bash -e` would have stopped every release; a `find` error was also masked by a trailing `if`. `tests/repo/test_image_data.sh` checked only that the strings `mmdb`, `/netray/data` and `docker push` appeared in the right order, so it passed throughout. Readers found both defects; a later reader showed the layer-scan assertion still passes with the scan loop deleted.
**What worked.** Running the step's shell under `bash -e` against hand-built cases — clean, offending, unreadable, empty — in a scratch directory, and fixing until each case gave the right exit code (`if … fi`, `sh -ec`, `--user 0`, a `docker save` layer scan with a no-layer guard).
**How to notice next time.** A guard's test can be satisfied by editing the guard into a no-op; then the logic has to move into an executable script with fixtures (workflow-rules R-R8).
