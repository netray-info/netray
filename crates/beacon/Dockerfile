FROM node:22-alpine AS frontend
WORKDIR /build/frontend
COPY frontend/package.json frontend/package-lock.json frontend/.npmrc ./
RUN --mount=type=secret,id=NODE_AUTH_TOKEN,env=NODE_AUTH_TOKEN npm ci
COPY frontend/ .
RUN npm run build

FROM clux/muslrust:stable AS builder
WORKDIR /build
COPY Cargo.toml Cargo.lock build.rs ./
COPY src src/
COPY --from=frontend /build/frontend/dist frontend/dist/
RUN cargo build --release --bins && cp $(find /build -xdev -name mail-inspector) /

FROM alpine:3.21
RUN apk add --no-cache ca-certificates wget \
 && addgroup -S mailcheck && adduser -S mailcheck -G mailcheck
WORKDIR /mail-inspector
COPY mail-inspector.toml mail-inspector.toml
ENV MAIL__SERVER__BIND=0.0.0.0:3000
COPY --from=builder /mail-inspector .
RUN chown -R mailcheck:mailcheck /mail-inspector
USER mailcheck
EXPOSE 3000 9090
CMD ["./mail-inspector", "mail-inspector.toml"]
