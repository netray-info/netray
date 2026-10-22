FROM node:22-alpine AS frontend
WORKDIR /build
COPY . .
RUN --mount=type=cache,target=/root/.npm npm ci
RUN npm run build --workspaces --if-present

FROM clux/muslrust:stable AS builder
WORKDIR /build
# `.dockerignore` prunes target/, node_modules/, the islands' dist/ and .git/.
COPY . .
COPY --from=frontend /build/crates/web/islands/dist crates/web/islands/dist/
RUN cargo build --release -p netray \
 && mkdir /out \
 && cp "$(find /build/target -xdev -type f -path '*/release/*' -name netray | head -n1)" /out/


FROM alpine:3.21
RUN apk add --no-cache ca-certificates wget \
 && addgroup -S netray && adduser -S netray -G netray
WORKDIR /netray
COPY --from=builder /out/ /usr/local/bin/
COPY crates/netray/netray.example.toml ./
RUN chown -R netray:netray /netray
USER netray
# One image, one binary, one process for every host:
#   docker run -v ./netray.toml:/netray/netray.toml netray:local /netray/netray.toml
ENTRYPOINT ["netray", "serve"]
