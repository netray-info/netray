FROM node:22-alpine AS frontend
WORKDIR /build
COPY . .
RUN --mount=type=cache,target=/root/.npm npm ci
RUN npm run build:types -w @netray-info/common-frontend \
 && npm run build --workspaces --if-present

FROM clux/muslrust:stable AS builder
WORKDIR /build
# `.dockerignore` prunes target/, node_modules/, frontend/dist/ and .git/.
COPY . .
COPY --from=frontend /build/crates/beacon/frontend/dist crates/beacon/frontend/dist/
COPY --from=frontend /build/crates/ifconfig-rs/frontend/dist crates/ifconfig-rs/frontend/dist/
COPY --from=frontend /build/crates/lens/frontend/dist crates/lens/frontend/dist/
COPY --from=frontend /build/crates/mhost-prism/frontend/dist crates/mhost-prism/frontend/dist/
COPY --from=frontend /build/crates/spectra/frontend/dist crates/spectra/frontend/dist/
COPY --from=frontend /build/crates/tlsight/frontend/dist crates/tlsight/frontend/dist/
RUN cargo build --release --workspace --bins \
 && mkdir /out \
 && for b in beacon ifconfig-rs lens prism spectra tlsight; do \
      cp "$(find /build/target -xdev -type f -path '*/release/*' -name "$b" | head -n1)" /out/; \
    done

FROM alpine:3.21
RUN apk add --no-cache ca-certificates wget \
 && addgroup -S netray && adduser -S netray -G netray
WORKDIR /netray
COPY --from=builder /out/ /usr/local/bin/
COPY crates/beacon/beacon.toml crates/ifconfig-rs/ifconfig.example.toml crates/lens/lens.example.toml \
     crates/mhost-prism/prism.example.toml crates/spectra/spectra.example.toml \
     crates/tlsight/tlsight.example.toml ./
RUN chown -R netray:netray /netray
USER netray
# One image, several binaries until the single `netray` binary replaces them:
#   docker run netray:local lens lens.example.toml
