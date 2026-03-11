# ---------------------------------------------------------------------------
# Stage 1: Build frontend
# ---------------------------------------------------------------------------
FROM node:22-alpine AS frontend
WORKDIR /build/frontend
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci
COPY frontend/ .
RUN npm run build

# ---------------------------------------------------------------------------
# Stage 2: Build Rust binary
# ---------------------------------------------------------------------------
FROM clux/muslrust:stable AS builder
WORKDIR /build
COPY Cargo.toml Cargo.lock build.rs ./
COPY src src/
COPY --from=frontend /build/frontend/dist frontend/dist/
RUN cargo build --release --bins && cp $(find /build -xdev -name prism) /

# ---------------------------------------------------------------------------
# Stage 3: Runtime image
# ---------------------------------------------------------------------------
FROM alpine:3.21
RUN apk add --no-cache ca-certificates wget \
  && addgroup -S prism && adduser -S prism -G prism
WORKDIR /prism
COPY --from=builder /prism .
RUN chown -R prism:prism /prism
USER prism

# API + frontend
EXPOSE 8080
# Prometheus metrics — must NOT be published externally (Prometheus scraper access only).
# Do not use -p 9090:9090 or equivalent. Restrict access at the network/firewall level.
EXPOSE 9090

ENTRYPOINT ["./prism"]
