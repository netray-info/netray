# Review: production runs only the acceptance subset

Context: on 2026-10-08 the full acceptance suite against production made the host's fail2ban `traefik-botsearch` jail ban the runner IP (45.130.93.240). Operator decision: production gets only the Playwright project `prod` (health, ready, security headers/CORS, TLS handshakes, assets/MTA-STS, meta, docs). `just acceptance` runs `--project prod`; the full suite stays local.

## main..175c4c2

### Reader

COUNTS blockers=0 majors=0 minors=3
LENSES Engineering, Testing

- MINOR: `tests/repo/test_acceptance_prod_subset.sh` matches literal file names, so a glob in the prod `testMatch` (e.g. `**/*.spec.ts`) would pass the guard while collecting the forbidden specs.
- MINOR: the guard's extracted block starts at `name: 'prod'`, so a `dependencies` key placed above `name` goes unseen.
- MINOR: file-selected runs (`just acceptance-local` default, or `npx playwright test <files>`) now run each prod spec twice, once in its own project and once in `prod`; results unchanged, requests doubled.

Traced: `--list --project prod` collects exactly the 7 files (67 tests); every requested path exists in production; the lens tests follow the Traefik 301 to the apex, where lens's router takes the paths; tls-certs sends no HTTP; none of the requests can produce 400/401/403, the codes the botsearch filter bans on.

### Summary

0 / 0 / 3. Verified 0, held 0. The three minors are listed, not repaired: a guard that lists the project through `npx playwright test --list` would close the first two but needs the acceptance node_modules in the gate.
