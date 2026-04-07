# ── Build stage ─────────────────────────────────────────────────────────────
FROM rust:alpine AS builder

RUN apk add --no-cache musl-dev nodejs npm

WORKDIR /build
COPY . .

# Build frontend first (embedded into binary via rust-embed)
RUN cd frontend && npm ci && npm run build

# Build Rust binary (release, static musl)
ENV RUSTFLAGS="-C target-feature=+crt-static"
RUN cargo build --release --target x86_64-unknown-linux-musl 2>/dev/null || \
    cargo build --release

# ── Runtime stage ───────────────────────────────────────────────────────────
FROM alpine:3

RUN addgroup -S lens && adduser -S lens -G lens
WORKDIR /app
COPY --from=builder /build/target/*/release/lens /app/lens
COPY --from=builder /build/profiles/ /app/profiles/

USER lens
EXPOSE 8082 8090
ENTRYPOINT ["/app/lens"]
