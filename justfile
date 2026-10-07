# The verbs of the netray monorepo (specs/features/monorepo-p1/spec.md, requirement 8).

test_env := env_var_or_default("TEST_ENV", "production")

default: adlc-verify

# --- the contract ------------------------------------------------------------

# The ADLC gate: offline, no browser. Needs `just adlc-setup` once (frontend/dist is embedded by the crates).
adlc-verify: test-repo check-frontend-dist fmt-check clippy test-rust-offline test-frontend validate-site check-sitemap

# Everything the gate runs, plus the full Rust suite (ifconfig-rs integration tests need `just ifconfig-data`).
check: adlc-verify test-rust

# Repository structure checks: one test per file, each exits non-zero on failure.
test-repo:
    #!/usr/bin/env bash
    set -uo pipefail
    fail=0
    for t in tests/repo/test_*.sh; do
        [ -e "$t" ] || continue
        if bash "$t"; then echo "ok   $t"; else echo "FAIL $t"; fail=1; fi
    done
    exit $fail

# The crates embed frontend/dist (RustEmbed); without it nothing compiles.
check-frontend-dist:
    #!/usr/bin/env bash
    missing=0
    for d in crates/*/frontend/dist; do
        [ -d "$d" ] || { echo "$d is missing" >&2; missing=1; }
    done
    if [ "$missing" -ne 0 ]; then
        echo "the crates embed frontend/dist (RustEmbed) and do not compile without it; run 'just adlc-setup'" >&2
        exit 1
    fi

fmt-check:
    cargo fmt --all -- --check

clippy:
    cargo clippy --workspace --all-features -- -D warnings

# ifconfig-rs integration tests need GeoIP data (`just ifconfig-data`), so only its unit tests run offline.
test-rust-offline:
    cargo test --workspace --exclude ifconfig-rs
    cargo test -p ifconfig-rs --lib

test-rust:
    cargo test --workspace

# ifconfig-rs including the tests that need GeoIP data.
test-ifconfig-data:
    @[ -f crates/ifconfig-rs/data/GeoLite2-City.mmdb ] || { echo "crates/ifconfig-rs/data/GeoLite2-City.mmdb is missing; run 'just ifconfig-data'" >&2; exit 1; }
    cargo test -p ifconfig-rs -- --include-ignored

test-frontend:
    npm test --workspaces --if-present

# Every site/ HTML file is well-formed.
validate-site:
    @find site/ -name '*.html' -print0 | sort -z | xargs -0 -I{} \
      python3 -c "import html.parser, sys; html.parser.HTMLParser().feed(open(sys.argv[1]).read()); print('ok', sys.argv[1])" "{}" \
        || { echo "FAIL"; exit 1; }

# sitemap.xml matches what site/ would generate.
check-sitemap:
    #!/usr/bin/env bash
    set -euo pipefail
    generated="$(mktemp "${TMPDIR:-/tmp}/sitemap.XXXXXX")"
    trap 'rm -f "$generated"' EXIT
    node site/scripts/build-sitemap.mjs --stdout > "$generated"
    if ! diff -u site/sitemap.xml "$generated"; then
        echo "sitemap drift: run 'node site/scripts/build-sitemap.mjs' and commit the result" >&2
        exit 1
    fi
    echo "ok site/sitemap.xml is up to date"

# --- build and ship ----------------------------------------------------------

# Frontends first (the crates embed their dist), then the release binaries.
build:
    npm run build:types -w @netray-info/common-frontend
    npm run build --workspaces --if-present
    cargo build --release -p netray

image:
    docker build -t netray:local .

# Playwright acceptance suite against TEST_ENV (default: production). Network and browser.
acceptance:
    cd tests/acceptance && TEST_ENV={{test_env}} npx playwright test

# Set the workspace version, open a changelog section, commit and tag. Never pushes.
release version:
    #!/usr/bin/env bash
    set -euo pipefail
    v={{quote(version)}}
    [[ "$v" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "version must be X.Y.Z, got '$v'" >&2; exit 1; }
    if git rev-parse -q --verify "refs/tags/v$v" >/dev/null; then
        echo "release refused: tag v$v already exists" >&2
        exit 1
    fi
    current=$(awk '/^\[/ { in_ws = ($0 == "[workspace.package]") } in_ws && /^version = / { gsub(/"/, "", $3); print $3 }' Cargo.toml)
    if [ "$(printf '%s\n%s\n' "$current" "$v" | sort -V | head -1)" != "$current" ]; then
        echo "release refused: $v is lower than the current version $current" >&2
        exit 1
    fi
    if ! { git diff --quiet && git diff --cached --quiet; }; then
        echo "release refused: the working tree has uncommitted changes; commit or stash them first" >&2
        exit 1
    fi
    awk -v v="$v" '/^\[/ { in_ws = ($0 == "[workspace.package]") } in_ws && /^version = / { $0 = "version = \"" v "\"" } { print }' Cargo.toml > Cargo.toml.new
    mv Cargo.toml.new Cargo.toml
    cargo update --workspace
    awk -v v="$v" -v d="$(date +%F)" '{ print } /^## \[Unreleased\]/ { print ""; print "## [" v "] - " d }' CHANGELOG.md > CHANGELOG.md.new
    mv CHANGELOG.md.new CHANGELOG.md
    git commit -m "chore: release v$v" -- Cargo.toml Cargo.lock CHANGELOG.md
    git tag -s "v$v" -m "Release v$v"
    echo "tagged v$v; push with: git push && git push origin v$v"

# --- setup -------------------------------------------------------------------

# What makes a fresh checkout able to run the contract: the tools, the dependencies, the frontend builds.
adlc-setup:
    @for t in cargo node npm bash; do command -v "$t" >/dev/null || { echo "$t is not on PATH" >&2; exit 1; }; done
    npm ci
    npm run build:types -w @netray-info/common-frontend
    npm run build --workspaces --if-present

# --- data (network) ----------------------------------------------------------

# Fetch the ifconfig-rs runtime data (GeoIP needs geoipupdate and data/.geoip.conf).
ifconfig-data:
    crates/ifconfig-rs/data/fetch.sh

# Build the ifconfig-rs-data image (multi-arch); push="true" pushes it instead of loading it.
ifconfig-data-image push="false": ifconfig-data
    cd crates/ifconfig-rs/data && docker buildx build --platform linux/amd64,linux/arm64 \
        --tag ghcr.io/netray-info/ifconfig-rs-data:latest \
        {{ if push == "true" { "--push" } else { "--load" } }} \
        .

# Refresh crates/tlsight/data/caa_domains.tsv (committed) from SSLMate and CCADB.
tlsight-data:
    cd crates/tlsight/data && curl -fsSL https://web.api.sslmate.com/caahelper/issuers -o sslmate_issuers.json
    cd crates/tlsight/data && curl -fsSL https://ccadb.my.salesforce-sites.com/ccadb/AllCAAIdentifiersReportCSVV2 -o ccadb_caa_identifiers.csv
    cd crates/tlsight/data && python3 process.py

# Per-crate Playwright e2e suite (ifconfig-rs or tlsight) against a running service (BASE_URL). Network and browser.
e2e crate:
    cd crates/{{crate}}/tests/e2e && npm install && npx playwright install && npx playwright test --reporter=list
