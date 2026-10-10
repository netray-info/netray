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
COPY --from=frontend /build/crates/email/frontend/dist crates/email/frontend/dist/
COPY --from=frontend /build/crates/ip/frontend/dist crates/ip/frontend/dist/
COPY --from=frontend /build/crates/lens/frontend/dist crates/lens/frontend/dist/
COPY --from=frontend /build/crates/mhost-prism/frontend/dist crates/mhost-prism/frontend/dist/
COPY --from=frontend /build/crates/http/frontend/dist crates/http/frontend/dist/
COPY --from=frontend /build/crates/tlsight/frontend/dist crates/tlsight/frontend/dist/
RUN cargo build --release -p netray \
 && mkdir /out \
 && cp "$(find /build/target -xdev -type f -path '*/release/*' -name netray | head -n1)" /out/


FROM alpine:3.21
RUN apk add --no-cache ca-certificates wget \
 && addgroup -S netray && adduser -S netray -G netray
WORKDIR /netray
COPY --from=builder /out/ /usr/local/bin/
COPY crates/email/beacon.toml crates/ip/ifconfig.example.toml crates/lens/lens.example.toml \
     crates/mhost-prism/prism.example.toml crates/http/spectra.example.toml \
     crates/tlsight/tlsight.example.toml ./
COPY site ./site
RUN chown -R netray:netray /netray
USER netray
# One image, one binary; the subcommand picks the service:
#   docker run netray:local lens lens.example.toml
#   docker run netray:local site --bind 0.0.0.0:8080
ENTRYPOINT ["netray"]
