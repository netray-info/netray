## 99b68f2..ef04ce1

### Reader

COUNTS blockers=1 majors=0 minors=2 (classes: BLOCKER 1, AMENDMENT 1, NIT 2)

BLOCKER | .github/workflows/release.yml:117 | `[ -d /netray/data ] && find …` returns 1 on a clean image, aborting the step under `bash -e`; every release stops. | Image without /netray/data.
AMENDMENT | .github/workflows/release.yml:116 | The search ran as user netray and could not see root-only directories. | `.mmdb` under /root passes.
NIT | tests/repo/test_image_data.sh:17 | relative COPY destination not caught.
NIT | README.md:28 | data paths come from the production config, not the image.

### Summary

All repaired test-first (6c1e3b0, 3ef8cb7, ef04ce1). verified 2, held 2 (both reproduced by simulation under bash -e).

## ef04ce1 (second pass)

### Reader

COUNTS blockers=0 majors=0 minors=4 (classes: DEFERRED 1, AMENDMENT 1, NIT 3)

DEFERRED | .github/workflows/release.yml:117 | runtime search misses files hidden by a later layer or in a VOLUME; they still ship. | `COPY x.mmdb /tmp/` then `RUN rm`.
AMENDMENT | tests/repo/test_image_data.sh:30 | no test requires `sh -e`.
NIT | tests/repo/test_image_data.sh:31 | clean-image guard matches one spelling only.
NIT | .github/workflows/release.yml:117 | a symlinked /netray/data is not followed.
NIT | README.md:28 | asn_patterns.toml is a repository file, not fetched.

### Summary

DEFERRED, AMENDMENT and the README NIT repaired (0d89129, 0d10e20); layer scan simulated (clean passes; whiteout, data-dir, empty fail).

## ef04ce1..0d10e20

### Reader

COUNTS blockers=0 majors=0 minors=2

MINOR | tests/repo/test_image_data.sh:35 | the layer-scan assertion only checks that `docker save` precedes `docker push`; deleting the scan loop or neutering its pattern still passes.
MINOR | tests/repo/test_image_data.sh:33 | the `sh -e` assertion is position-sensitive (`-ec '` must start a line) and not tied to the data-check step.

Sound: the loop on mock `docker save` layouts passes clean images, fails `.mmdb` or `netray/data/` in any layer and an empty export; JSON blobs are skipped; `bash -e` does not abort a clean run; the final stage creates no `netray/data` entry.

### Summary

0/0/2. Both minors are the text-check class recorded in specs/solutions/2026-10-08-text-checks-pass-without-observing.md.
