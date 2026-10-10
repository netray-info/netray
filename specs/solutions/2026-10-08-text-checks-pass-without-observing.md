---
class: check-passes-without-observing
repeat-of: none
---
# Text-pattern checks in tests/repo passed without observing what they claim

**What failed.** Several `tests/repo/*.sh` checks matched text instead of observing behaviour, and passed when the property did not hold: a scan over a here-string printed `PASS` after bash could not create the temp file (`cannot create temp file for here document`), so no file was scanned; the CI no-clobber guard accepted a `docker cp` into `./crates/ip/data/` as long as any `cp -n` appeared elsewhere; the first schema variants were "rejected" because of an unrelated `site` key, not the URL rule they named.
**What worked.** Parse instead of grep where a parser exists (`ruby -ryaml` for workflows, ajv for the schema); give every rejection a positive control that must pass (`lens with https://email.example.com validates`); fail closed when an input or temp file is missing; feed loops from pipes, not here-strings.
**How to notice next time.** A new check passes on the first run against a tree that should fail, or a reviewer can name an input that satisfies the pattern but violates the claim.
