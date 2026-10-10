# Review: changelog 0.24.0

## main..HEAD

### Reader

COUNTS blockers=1 majors=1 minors=3

| severity | at | finding | outcome |
|---|---|---|---|
| BLOCKER | CHANGELOG.md:19 | "metric names unchanged" though `lens_unknown_verdict_total` is no longer emitted | repaired: the entry names its removal |
| MAJOR | CHANGELOG.md:14 | "no longer call the services over HTTP" though the modules still enrich from `netray ip` | repaired: names the remaining enrichment route |
| MINOR | CHANGELOG.md:17 | "checks configuration only" though `custom_ca_dir` is still checked; "no longer" misdescribes 0.23 | repaired |
| MINOR | CHANGELOG.md:15 | section enablement moved from `url` to the table's presence | repaired |
| MINOR | CHANGELOG.md:19 | log targets follow the crate names | repaired |

### Summary

1/1/3, all repaired in the same file; verified by reading the entries against the reader's anchors (crates/lens/src/state.rs:116, crates/tls/src/config.rs:122, crates/tls/src/lib.rs:47, crates/lens/tests/fixtures/lens.production.toml:112).

## HEAD~1..HEAD (repair)

### Reader

COUNTS blockers=0 majors=2 minors=0

| severity | at | finding | outcome |
|---|---|---|---|
| MAJOR | CHANGELOG.md:15 | only HTTP and email are switched on by their table; DNS/TLS always run, IP by `[modules.ip]` | repaired |
| MAJOR | CHANGELOG.md:14 | DNS and TLS enrich via `backends.ip.url`, not `ip_url` | repaired: names each key |

### Summary

0/2/0, repaired; checked against crates/lens/src/state.rs:100 and crates/{dns,tls}/src/config.rs:201,199.
