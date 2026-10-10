# The verbs of the netray monorepo (specs/features/monorepo-p1/spec.md, requirement 8).

test_env := env_var_or_default("TEST_ENV", "production")

default: adlc-verify

# --- the contract ------------------------------------------------------------

# The ADLC gate: offline, no browser. Needs `just adlc-setup` once (frontend/dist is embedded by the crates).
adlc-verify: test-repo check-frontend-dist fmt-check clippy test-rust-offline test-frontend validate-site check-sitemap

# Everything the gate runs, plus the full Rust suite (ifconfig-rs integration tests need `just ifconfig-data`).
check: adlc-verify deny test-rust

# Deterministic supply-chain checks; advisories run on a schedule elsewhere (workflow-rules R-J6).
deny:
    cargo deny check bans licenses sources

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
    cargo test --workspace --exclude netray-ip
    cargo test -p netray-ip --lib

# The full Rust suite. Most ifconfig-rs integration tests need GeoIP data (`just ifconfig-data`,
# MaxMind licence); without it they are skipped with a notice and run in CI with the data image.
test-rust:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -f crates/ip/data/GeoLite2-City.mmdb ]; then
        cargo test --workspace
    else
        echo "notice: no GeoIP data; ifconfig-rs integration tests skipped (just ifconfig-data, then just test-ifconfig-data)" >&2
        cargo test --workspace --exclude netray-ip
        cargo test -p netray-ip --lib
    fi

# ifconfig-rs including the tests that need GeoIP data.
test-ifconfig-data:
    @[ -f crates/ip/data/GeoLite2-City.mmdb ] || { echo "crates/ip/data/GeoLite2-City.mmdb is missing; run 'just ifconfig-data'" >&2; exit 1; }
    cargo test -p netray-ip -- --include-ignored

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

# Playwright acceptance against TEST_ENV (default: production): only the `prod` project, the
# subset that hits known paths. The full suite trips production's fail2ban; run it with
# `just acceptance-local`.
acceptance:
    cd tests/acceptance && TEST_ENV={{test_env}} npx playwright test --project prod

# Playwright acceptance against a local stack: `netray site` and the six services on free 127.0.0.1 ports
# (passed to the suite as LOCAL_<NAME>_URL), from their dev configs. Default specs: the two that need no network.
acceptance-local *args:
    #!/usr/bin/env bash
    set -uo pipefail
    source tests/repo/lib/netray.sh
    bin=$(netray_bin) || exit 1
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"; _netray_cleanup' EXIT
    mkdir -p "$tmp/custom_cas"

    site_port=$(free_port)
    start_bg "$tmp/site.log" "$bin" site --bind "127.0.0.1:$site_port" --root site
    wait_http "http://127.0.0.1:$site_port/" 30 || { echo "netray site did not answer on 127.0.0.1:$site_port" >&2; exit 1; }
    export LOCAL_SITE_URL="http://localhost:$site_port"

    # sub | crate dir | config | bind var | metrics bind var | extra env (see tests/repo/test_smoke_services.sh)
    rows=(
      "lens|lens|lens.dev.toml|LENS_SERVER__BIND|LENS_SERVER__METRICS_BIND|LENS_SNAPSHOTS__DB_PATH=$tmp/snapshots.db"
      "dns|mhost-prism|prism.dev.toml|PRISM_SERVER__BIND|PRISM_SERVER__METRICS_BIND|"
      "tls|tls|tlsight.dev.toml|NETRAY_TLS_SERVER__BIND|NETRAY_TLS_SERVER__METRICS_BIND|NETRAY_TLS_VALIDATION__CUSTOM_CA_DIR=$tmp/custom_cas"
      "http|http|spectra.dev.toml|NETRAY_HTTP_SERVER__BIND|NETRAY_HTTP_SERVER__METRICS_BIND|"
      "email|email|beacon.dev.toml|NETRAY_EMAIL_SERVER__BIND|NETRAY_EMAIL_SERVER__METRICS_BIND|"
      "ip|ip|$REPO_ROOT/tests/repo/fixtures/ifconfig.smoke.toml|NETRAY_IP_SERVER__BIND|NETRAY_IP_SERVER__ADMIN_BIND|"
    )
    for row in "${rows[@]}"; do
        IFS='|' read -r sub dir cfg bind_var metrics_var extra <<<"$row"
        port=$(free_port)
        (
            cd "$REPO_ROOT/crates/$dir" || exit 1
            # shellcheck disable=SC2086
            start_bg "$tmp/$sub.log" env "$bind_var=127.0.0.1:$port" "$metrics_var=127.0.0.1:$(free_port)" $extra \
                "$bin" "$sub" "$cfg"
            echo "$!" >"$tmp/$sub.pid"
        )
        _NETRAY_PIDS+=("$(cat "$tmp/$sub.pid")")
        wait_http "http://127.0.0.1:$port/health" 30 \
            || { echo "netray $sub did not answer on 127.0.0.1:$port: $(tail -n 1 "$tmp/$sub.log")" >&2; exit 1; }
        export "LOCAL_$(printf %s "$sub" | tr a-z A-Z)_URL=http://localhost:$port"
    done

    cd tests/acceptance
    TEST_ENV=local npx playwright test --no-deps {{ if args == "" { "smoke/security-headers.spec.ts static-site/assets.spec.ts" } else { args } }}
    status=$?
    exit $status

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
    @command -v cargo-deny >/dev/null || cargo install cargo-deny --locked
    npm ci
    npm run build:types -w @netray-info/common-frontend
    npm run build --workspaces --if-present

# --- data (network) ----------------------------------------------------------

# Fetch the ifconfig-rs runtime data (GeoIP needs geoipupdate and data/.geoip.conf).
ifconfig-data:
    crates/ip/data/fetch.sh

# Build the ifconfig-rs-data image (multi-arch); push="true" pushes it instead of loading it.
ifconfig-data-image push="false": ifconfig-data
    cd crates/ip/data && docker buildx build --platform linux/amd64,linux/arm64 \
        --tag ghcr.io/netray-info/ifconfig-rs-data:latest \
        {{ if push == "true" { "--push" } else { "--load" } }} \
        .

# Refresh crates/tls/data/caa_domains.tsv (committed) from SSLMate and CCADB.
tlsight-data:
    cd crates/tls/data && curl -fsSL https://web.api.sslmate.com/caahelper/issuers -o sslmate_issuers.json
    cd crates/tls/data && curl -fsSL https://ccadb.my.salesforce-sites.com/ccadb/AllCAAIdentifiersReportCSVV2 -o ccadb_caa_identifiers.csv
    cd crates/tls/data && python3 process.py

# Per-crate Playwright e2e suite (ip or tls) against a running service (BASE_URL). Network and browser.
e2e crate:
    cd crates/{{crate}}/tests/e2e && npm install && npx playwright install && npx playwright test --reporter=list
