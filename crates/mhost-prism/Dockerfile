# ---------------------------------------------------------------------------
# Stage 1: Build frontend
# ---------------------------------------------------------------------------
FROM node:22-slim AS frontend
WORKDIR /app

COPY frontend/package.json frontend/package-lock.json ./frontend/
RUN cd frontend && npm ci

COPY frontend/ ./frontend/
RUN cd frontend && npm run build

# ---------------------------------------------------------------------------
# Stage 2: Build Rust binary
# ---------------------------------------------------------------------------
FROM rust:1-slim AS builder
WORKDIR /app

# Copy manifests + build script to cache dependency compilation as a layer.
COPY Cargo.toml Cargo.lock build.rs ./

# Stub src/main.rs and frontend/dist/index.html so build.rs is satisfied and
# all crate dependencies compile without the real application source.
RUN mkdir -p src frontend/dist \
 && echo 'fn main() {}' > src/main.rs \
 && echo '' > frontend/dist/index.html \
 && cargo build --release --locked \
 && rm -rf target/release/deps/prism-* target/release/.fingerprint/prism-*

# Copy real source and the built frontend (overwrites the stub dist/).
COPY src/ ./src/
COPY --from=frontend /app/frontend/dist ./frontend/dist/

# Recompile only prism; dependencies are already cached in the layer above.
RUN touch src/main.rs && cargo build --release --locked

# ---------------------------------------------------------------------------
# Stage 3: Runtime image
# ---------------------------------------------------------------------------
FROM debian:bookworm-slim

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/prism /usr/local/bin/prism

# API + frontend
EXPOSE 8080
# Prometheus metrics
EXPOSE 9090

ENTRYPOINT ["/usr/local/bin/prism"]
