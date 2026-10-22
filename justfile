# The verbs of the netray monorepo (specs/features/monorepo-p1/spec.md, requirement 8).

test_env := env_var_or_default("TEST_ENV", "production")

default: adlc-verify

# --- the contract ------------------------------------------------------------

# The ADLC gate: offline, no browser. Needs `just adlc-setup` once (netray-web embeds the islands bundle).
adlc-verify: test-repo check-frontend-dist fmt-check clippy test-rust-offline test-frontend

# Every test the gate runs, plus the full Rust suite (ifconfig-rs integration tests need `just ifconfig-data`).
# test-rust stands in for test-rust-offline: one feature set, so the workspace tests compile once
# (tests/repo/lib/verbs.sh holds it to a superset). With GeoIP data the tests run under the unified
# workspace features only; the gate's own feature sets (--exclude netray-ip, -p netray-ip) run in
# `adlc verify`, whose receipt every push needs.
check: test-repo check-frontend-dist fmt-check clippy test-frontend deny test-rust

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

# netray-web embeds the islands bundle; without it the pages ship no islands.
check-frontend-dist:
    #!/usr/bin/env bash
    if [ ! -f crates/web/islands/dist/islands.js ]; then
        echo "crates/web/islands/dist/islands.js is missing; run 'just adlc-setup'" >&2
        exit 1
    fi

fmt-check:
    cargo fmt --all -- --check

clippy:
    cargo clippy --workspace --all-features -- -D warnings

# ifconfig-rs integration tests need GeoIP data (`just ifconfig-data`), so offline only its unit tests and the module tests run.
# adlc counts a test file as run when it is a target's src_path; each crate's `it` target also ran its
# tests/it module files (tests/repo/test_test_binaries.sh holds every one to a mod line): ADLC-RAN names them.
test-rust-offline:
    cargo test --workspace --exclude netray-ip
    cargo test -p netray-ip --lib
    cargo test -p netray-ip --features testing --test module --test module_data --test module_ua --test module_v3 --test contract_status --test evidence
    @git ls-files 'crates/*/tests/it/*.rs' 'crates/*/tests/it/**/*.rs' | grep -v '/main\.rs$' | sed 's/^/ADLC-RAN /'

# The full Rust suite. Most ifconfig-rs integration tests need GeoIP data (`just ifconfig-data`,
# MaxMind licence); without it they are skipped with a notice and run in CI with the data image.
test-rust:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -f crates/ip/data/GeoLite2-City.mmdb ]; then
        cargo test --workspace
    else
        echo "notice: no GeoIP data; ifconfig-rs integration tests skipped (just ifconfig-data, then just test-ifconfig-data)" >&2
        just test-rust-offline
    fi

# ifconfig-rs including the tests that need GeoIP data.
test-ifconfig-data:
    @[ -f crates/ip/data/GeoLite2-City.mmdb ] || { echo "crates/ip/data/GeoLite2-City.mmdb is missing; run 'just ifconfig-data'" >&2; exit 1; }
    cargo test -p netray-ip -- --include-ignored

test-frontend:
    npm test --workspaces --if-present

# --- build and ship ----------------------------------------------------------

# The islands bundle first (netray-web embeds it), then the release binary.
build:
    npm run build --workspaces --if-present
    cargo build --release -p netray

image:
    docker build -t netray:local .

# Playwright acceptance against TEST_ENV (default: production): only the `prod` project, the
# subset that hits known paths. The full suite trips production's fail2ban; run it with
# `just acceptance-local`.
acceptance:
    cd tests/acceptance && TEST_ENV={{test_env}} npx playwright test --project prod

# Playwright acceptance against one local `netray serve` on a free 127.0.0.1 port, apex `localhost`
# (inspector hosts `<tool>.localhost`), over a temporary store seeded from
# tests/acceptance/fixtures/history. ACCEPTANCE_ENV adds NETRAY_ keys to its environment
# (`ACCEPTANCE_ENV="NETRAY_ADMISSION__CLIENT_BUDGET_PER_HOUR=0"`). Default specs: the two that need no network.
acceptance-local *args:
    #!/usr/bin/env bash
    set -uo pipefail
    source tests/repo/lib/netray.sh
    bin=$(netray_bin) || exit 1
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"; _netray_cleanup' EXIT

    cargo run -q -p netray-store --example seed -- "$tmp/history.db" tests/acceptance/fixtures/history || { echo "seed failed" >&2; exit 1; }

    port=$(free_port)
    # shellcheck disable=SC2086
    start_bg "$tmp/serve.log" env NETRAY_SERVER__BIND="127.0.0.1:$port" NETRAY_SERVER__METRICS_BIND="127.0.0.1:$(free_port)" \
        NETRAY_SERVER__APEX=localhost NETRAY_STORE__DB_PATH="$tmp/history.db" ${ACCEPTANCE_ENV:-} \
        "$bin" serve crates/netray/netray.dev.toml
    wait_http "http://127.0.0.1:$port/health" 30 \
        || { echo "netray serve did not answer on 127.0.0.1:$port: $(tail -n 1 "$tmp/serve.log")" >&2; exit 1; }
    export LOCAL_URL="http://localhost:$port" LOCAL_APEX=localhost

    specs=({{args}})
    [ ${#specs[@]} -gt 0 ] || specs=(smoke/security-headers.spec.ts smoke/health.spec.ts)
    specs=("${specs[@]#tests/acceptance/}")
    cd tests/acceptance
    # Playwright matches a file filter against the absolute path: anchor an existing relative
    # spec path here; absolute paths, options and option values pass unchanged.
    prev=""
    for i in "${!specs[@]}"; do
        arg="${specs[$i]}"
        case "$arg" in
            /*|-*) ;;
            *) case "$prev" in --*) ;; *) [ -e "${arg%%:*}" ] && specs[$i]="$PWD/$arg" ;; esac ;;
        esac
        prev="$arg"
    done
    TEST_ENV=local npx playwright test --no-deps "${specs[@]}"
    status=$?
    exit $status

# V2 pages against the private design canvas (specs/rules/design-rules.md): seeds a history,
# starts one `netray serve` on a free port, writes target/design/. Exit 0 within
# tolerance, 1 deviations, 2 setup error. Manifest: the argument, else $NETRAY_DESIGN_DIR/manifest.json
# (default: the netray.info checkout beside the main checkout).
design-check manifest="":
    #!/usr/bin/env bash
    set -uo pipefail
    source tests/repo/lib/netray.sh
    source tests/acceptance/design/stack.sh
    main_checkout=$(dirname "$(git rev-parse --path-format=absolute --git-common-dir)")
    export NETRAY_DESIGN_DIR="${NETRAY_DESIGN_DIR:-$main_checkout/../netray.info/specs/design/canvas}"
    m={{quote(manifest)}}
    [ -n "$m" ] || m="$NETRAY_DESIGN_DIR/manifest.json"
    loader=tests/acceptance/design/manifest.mjs
    node "$loader" "$m" >/dev/null || exit 2
    fixtures=$(node "$loader" --fixtures "$m") || exit 2
    hosts=$(node "$loader" --hosts "$m") || exit 2
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"; _netray_cleanup' EXIT
    # shellcheck disable=SC2086
    design_stack_up "$tmp" "$fixtures" $hosts || exit 2
    node tests/acceptance/design/check.mjs "$m" "$REPO_ROOT/target/design"

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
