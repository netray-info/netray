adlc-verify: test-repo fmt-check clippy test-rust-offline test-frontend

test-repo:
    echo repo
fmt-check:
    echo fmt
clippy:
    echo clippy
test-frontend:
    npm test --workspaces
deny:
    echo deny
test-rust-offline:
    cargo test --workspace --exclude netray-ip
    cargo test -p netray-ip --lib

check: test-repo fmt-check clippy test-frontend deny test-rust

test-rust:
    #!/usr/bin/env bash
    if [ -f data.mmdb ]; then
        cargo test --workspace --exclude netray-web
    else
        just test-rust-offline
    fi
